#[cfg(feature = "gpu")]
mod inner {
    use rustc_hash::FxHashMap;
    use std::time::Instant;

    use super::super::bridge::{CreatureInput, CreatureOutput, CreatureReward};
    use super::super::genome::{Genome, LearningGene, NodeType};
    use super::super::slot_alloc::SlotAllocator;
    use super::super::thread::TickExecutor;

    // ⚠️ 容量上限三处硬编码同步：
    // - 本处：MAX_NODES / MAX_CONNS
    // - CPU：src/neural/spiking.rs::SpikingNetwork::from_genome 内同名常量
    // - Shader：src/neural/snn_tick.wgsl 内 const MAX_NODES / MAX_CONNS
    // 改任意一处必须同步改另两处，否则 CPU/GPU 行为会偏离。
    pub const MAX_CREATURES: usize = 512;
    pub const MAX_NODES: usize = 256;
    pub const MAX_CONNS: usize = 512;
    pub const OUTPUTS_PER_CREATURE: usize = 8;

    const COUNTER_ELEMS: usize = MAX_CREATURES * OUTPUTS_PER_CREATURE;
    const COUNTER_BYTES: u64 = (COUNTER_ELEMS * 4) as u64; // u32 / f32 同宽
    const STAGING_BYTES: u64 = COUNTER_BYTES * 2; // spike_counts + first_outputs
    const TICK_PARAMS_BYTES: u64 = 16; // vec4<u32> 对齐

    // 学习相关常量
    const TRACES_TOTAL: usize = MAX_CREATURES * MAX_CONNS; // 65536
    const TRACES_BYTES: u64 = (TRACES_TOTAL * 4) as u64; // 262144 = 256KB
    const LEARNING_PARAMS_BYTES: u64 = (MAX_CREATURES * 16) as u64; // 8192 = 8KB

    /// tick_index 预填表的容量上限（最多支持单批 64 个 tick）
    /// 实际 snn_ticks = neural_tick_rate × dt，默认 10，64 留充足余量
    const TICK_BANK_CAPACITY: usize = 64;
    const TICK_BANK_BYTES: u64 = (TICK_BANK_CAPACITY * TICK_PARAMS_BYTES as usize) as u64;

    /// GPU 探测结果
    pub enum GpuProbeResult {
        Suitable {
            adapter_name: String,
            benchmark_us: u64,
        },
        Unsuitable {
            reason: String,
        },
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuNode {
        pub membrane: f32,
        pub decay: f32,
        pub threshold: f32,
        pub flags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuConnection {
        pub from_node: u32,
        pub to_node: u32,
        pub weight: f32,
        pub _pad: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuLearningParams {
        pub trace_decay: f32,
        pub learning_on: f32,
        pub _pad0: f32,
        pub _pad1: f32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuCreatureMeta {
        pub node_count: u32,
        pub conn_count: u32,
        pub input_count: u32,
        pub output_count: u32,
    }

    impl GpuNode {
        fn set_fired(&mut self, fired: bool) {
            if fired {
                self.flags |= 1;
            } else {
                self.flags &= !1;
            }
        }

        fn set_input(&mut self) {
            self.flags |= 2;
        }

        fn set_output(&mut self) {
            self.flags |= 4;
        }

        fn set_direct_read(&mut self) {
            self.flags |= 8;
        }

        fn set_refractory_period(&mut self, period: u8) {
            self.flags = (self.flags & 0xFF00FFFF) | ((period as u32) << 16);
        }
    }

    fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn bind_group_layout_entries() -> [wgpu::BindGroupLayoutEntry; 9] {
        [
            storage_entry(0, true),  // nodes_prev
            storage_entry(1, false), // nodes_next
            storage_entry(2, true),  // connections
            storage_entry(3, true),  // creature_meta
            storage_entry(4, false), // spike_counts (atomic)
            storage_entry(5, false), // first_outputs
            uniform_entry(6),        // tick_params
            storage_entry(7, false), // eligibility_traces (read_write)
            storage_entry(8, true),  // learning_params (read)
        ]
    }

    /// 探测 GPU 是否适合计算
    pub fn probe_gpu() -> GpuProbeResult {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })) {
                Some(a) => a,
                None => {
                    return GpuProbeResult::Unsuitable {
                        reason: "无可用 GPU 适配器".into(),
                    }
                }
            };

        let info = adapter.get_info();
        eprintln!("[GPU] 检测到适配器: {} ({:?})", info.name, info.device_type);

        match info.device_type {
            wgpu::DeviceType::IntegratedGpu => {
                return GpuProbeResult::Unsuitable {
                    reason: format!("集显 {} 不适合 GPU 计算（readback 慢）", info.name),
                };
            }
            wgpu::DeviceType::Cpu => {
                return GpuProbeResult::Unsuitable {
                    reason: "CPU 模拟适配器".into(),
                };
            }
            _ => {}
        }

        let (device, queue) = match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("snn-probe"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        )) {
            Ok(pair) => pair,
            Err(e) => {
                return GpuProbeResult::Unsuitable {
                    reason: format!("无法创建 device: {}", e),
                }
            }
        };

        // 验证 shader 编译 + 新 bind group layout
        let shader_source = include_str!("snn_tick.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("snn-tick-probe"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("snn-layout-probe"),
            entries: &bind_group_layout_entries(),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("snn-pipeline-layout-probe"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let _pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("snn-pipeline-probe"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        // benchmark：空 dispatch
        let start = Instant::now();
        for _ in 0..100 {
            let encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            queue.submit(Some(encoder.finish()));
            device.poll(wgpu::Maintain::Wait);
        }
        let elapsed_us = start.elapsed().as_micros() as u64 / 100;

        eprintln!("[GPU] benchmark: {}μs/tick (空 dispatch)", elapsed_us);

        if elapsed_us > 200 {
            GpuProbeResult::Unsuitable {
                reason: format!("benchmark {}μs/tick 超过 200μs 阈值", elapsed_us),
            }
        } else {
            GpuProbeResult::Suitable {
                adapter_name: info.name.clone(),
                benchmark_us: elapsed_us,
            }
        }
    }

    /// GPU 计算引擎（同步批处理）
    pub struct GpuCompute {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::ComputePipeline,

        // 节点双缓冲
        nodes_buf_a: wgpu::Buffer,
        nodes_buf_b: wgpu::Buffer,
        connections_buf: wgpu::Buffer,
        meta_buf: wgpu::Buffer,

        // 输出累加（GPU 端）
        spike_counts_buf: wgpu::Buffer,
        first_outputs_buf: wgpu::Buffer,

        // uniform：tick_index（dispatch 当前读取的）
        tick_params_buf: wgpu::Buffer,
        // 预填的 tick_index 表（device-local 只读），合并 dispatch 时
        // 通过 encoder.copy_buffer_to_buffer 在 GPU 时间线上把第 i 项搬到 tick_params_buf
        tick_index_bank_buf: wgpu::Buffer,

        // 回读 staging：连续存放 spike_counts(14336) + first_outputs(14336)
        staging_buf: wgpu::Buffer,

        // 学习相关 buffer
        eligibility_traces_buf: wgpu::Buffer,
        learning_params_buf: wgpu::Buffer,
        traces_staging_buf: wgpu::Buffer,

        // 预构建 bind group：读A→写B / 读B→写A
        bind_group_ping: wgpu::BindGroup,
        bind_group_pong: wgpu::BindGroup,

        ping: bool,

        // CPU 侧镜像（基因组/元数据上传路径使用）
        nodes_cpu: Vec<GpuNode>,
        connections_cpu: Vec<GpuConnection>,
        meta_cpu: Vec<GpuCreatureMeta>,

        // 清零用零缓冲（长度 = COUNTER_BYTES，复用避免分配）
        zero_counter_bytes: Vec<u8>,

        // 上一批读回的原始字节（spike_counts + first_outputs）
        last_raw: Vec<u8>,

        // 学习相关 CPU 镜像
        traces_cpu: Vec<f32>,
    }

    impl GpuCompute {
        pub fn new() -> Option<Self> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                }))?;

            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("snn-compute"),
                    ..Default::default()
                },
                None,
            ))
            .ok()?;

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("snn-tick"),
                source: wgpu::ShaderSource::Wgsl(include_str!("snn_tick.wgsl").into()),
            });

            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("snn-layout"),
                    entries: &bind_group_layout_entries(),
                });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("snn-pipeline-layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("snn-pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: "main",
                compilation_options: Default::default(),
                cache: None,
            });

            let total_nodes = MAX_CREATURES * MAX_NODES;
            let total_conns = MAX_CREATURES * MAX_CONNS;

            let nodes_size = (total_nodes * std::mem::size_of::<GpuNode>()) as u64;
            let conns_size = (total_conns * std::mem::size_of::<GpuConnection>()) as u64;
            let meta_size = (MAX_CREATURES * std::mem::size_of::<GpuCreatureMeta>()) as u64;

            let usage_storage_rw = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC;
            let usage_storage_r = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;

            let nodes_buf_a = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nodes_a"),
                size: nodes_size,
                usage: usage_storage_rw,
                mapped_at_creation: false,
            });
            let nodes_buf_b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nodes_b"),
                size: nodes_size,
                usage: usage_storage_rw,
                mapped_at_creation: false,
            });
            let connections_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("connections"),
                size: conns_size,
                usage: usage_storage_r,
                mapped_at_creation: false,
            });
            let meta_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("meta"),
                size: meta_size,
                usage: usage_storage_r,
                mapped_at_creation: false,
            });
            let spike_counts_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("spike_counts"),
                size: COUNTER_BYTES,
                usage: usage_storage_rw,
                mapped_at_creation: false,
            });
            let first_outputs_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("first_outputs"),
                size: COUNTER_BYTES,
                usage: usage_storage_rw,
                mapped_at_creation: false,
            });
            let tick_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tick_params"),
                size: TICK_PARAMS_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // 预填 tick_index bank：[0,0,0,0, 1,0,0,0, 2,0,0,0, ...]
            // 用 mapped_at_creation 同步写入，避免依赖后续 submit
            let tick_index_bank_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tick_index_bank"),
                size: TICK_BANK_BYTES,
                usage: wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            });
            {
                let mut view = tick_index_bank_buf.slice(..).get_mapped_range_mut();
                let u32_view: &mut [u32] = bytemuck::cast_slice_mut(&mut view);
                for i in 0..TICK_BANK_CAPACITY {
                    u32_view[i * 4] = i as u32;
                    u32_view[i * 4 + 1] = 0;
                    u32_view[i * 4 + 2] = 0;
                    u32_view[i * 4 + 3] = 0;
                }
            }
            tick_index_bank_buf.unmap();
            let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging"),
                size: STAGING_BYTES,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // 学习相关 buffer
            let eligibility_traces_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("eligibility_traces"),
                size: TRACES_BYTES,
                usage: usage_storage_rw,
                mapped_at_creation: false,
            });
            let learning_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("learning_params"),
                size: LEARNING_PARAMS_BYTES,
                usage: usage_storage_r,
                mapped_at_creation: false,
            });
            let traces_staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("traces_staging"),
                size: TRACES_BYTES,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bind_group_ping = Self::make_bind_group(
                &device,
                &bind_group_layout,
                &nodes_buf_a,
                &nodes_buf_b,
                &connections_buf,
                &meta_buf,
                &spike_counts_buf,
                &first_outputs_buf,
                &tick_params_buf,
                &eligibility_traces_buf,
                &learning_params_buf,
                "snn-bind-ping",
            );
            let bind_group_pong = Self::make_bind_group(
                &device,
                &bind_group_layout,
                &nodes_buf_b,
                &nodes_buf_a,
                &connections_buf,
                &meta_buf,
                &spike_counts_buf,
                &first_outputs_buf,
                &tick_params_buf,
                &eligibility_traces_buf,
                &learning_params_buf,
                "snn-bind-pong",
            );

            Some(Self {
                device,
                queue,
                pipeline,
                nodes_buf_a,
                nodes_buf_b,
                connections_buf,
                meta_buf,
                spike_counts_buf,
                first_outputs_buf,
                tick_params_buf,
                tick_index_bank_buf,
                staging_buf,
                eligibility_traces_buf,
                learning_params_buf,
                traces_staging_buf,
                bind_group_ping,
                bind_group_pong,
                ping: true,
                nodes_cpu: vec![GpuNode::default(); total_nodes],
                connections_cpu: vec![GpuConnection::default(); total_conns],
                meta_cpu: vec![GpuCreatureMeta::default(); MAX_CREATURES],
                zero_counter_bytes: vec![0u8; COUNTER_BYTES as usize],
                last_raw: vec![0u8; STAGING_BYTES as usize],
                traces_cpu: vec![0.0f32; TRACES_TOTAL],
            })
        }

        fn make_bind_group(
            device: &wgpu::Device,
            layout: &wgpu::BindGroupLayout,
            read_buf: &wgpu::Buffer,
            write_buf: &wgpu::Buffer,
            conn: &wgpu::Buffer,
            meta: &wgpu::Buffer,
            spike: &wgpu::Buffer,
            first: &wgpu::Buffer,
            params: &wgpu::Buffer,
            traces: &wgpu::Buffer,
            learning: &wgpu::Buffer,
            label: &str,
        ) -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: read_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: write_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: conn.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: meta.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: spike.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: first.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: traces.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: learning.as_entire_binding(),
                    },
                ],
            })
        }

        /// 上传基因组到指定 slot
        pub fn upload_genome(&mut self, slot: usize, genome: &Genome) {
            if slot >= MAX_CREATURES {
                return;
            }

            super::super::capacity::warn_nodes_overflow(genome.nodes.len(), MAX_NODES, "GPU-SNN");
            let enabled_conns = genome.connections.iter().filter(|c| c.enabled).count();
            super::super::capacity::warn_conns_overflow(enabled_conns, MAX_CONNS, "GPU-SNN");

            let node_base = slot * MAX_NODES;
            let conn_base = slot * MAX_CONNS;

            for i in 0..MAX_NODES {
                self.nodes_cpu[node_base + i] = GpuNode::default();
            }
            for i in 0..MAX_CONNS {
                self.connections_cpu[conn_base + i] = GpuConnection::default();
            }

            let mut id_to_local: FxHashMap<usize, u32> = FxHashMap::default();
            let mut input_count = 0u32;
            let mut output_count = 0u32;

            for (local_idx, node) in genome.nodes.iter().enumerate() {
                if local_idx >= MAX_NODES {
                    break;
                }
                id_to_local.insert(node.id, local_idx as u32);

                let mut gpu_node = GpuNode {
                    membrane: 0.0,
                    decay: node.decay as f32,
                    threshold: node.threshold as f32,
                    flags: 0,
                };
                gpu_node.set_refractory_period(node.refractory_period);

                match node.node_type {
                    NodeType::Input => {
                        gpu_node.set_input();
                        input_count += 1;
                    }
                    NodeType::Output => {
                        gpu_node.set_output();
                        output_count += 1;
                        if node.threshold == 0.0 {
                            gpu_node.set_direct_read();
                        }
                    }
                    NodeType::Block(_) => {}
                }

                self.nodes_cpu[node_base + local_idx] = gpu_node;
            }

            let mut conn_idx = 0;
            for conn in &genome.connections {
                if !conn.enabled {
                    continue;
                }
                if conn_idx >= MAX_CONNS {
                    break;
                }
                if let (Some(&from), Some(&to)) = (
                    id_to_local.get(&conn.in_node),
                    id_to_local.get(&conn.out_node),
                ) {
                    self.connections_cpu[conn_base + conn_idx] = GpuConnection {
                        from_node: from,
                        to_node: to,
                        weight: conn.weight as f32,
                        _pad: 0,
                    };
                    conn_idx += 1;
                }
            }

            self.meta_cpu[slot] = GpuCreatureMeta {
                node_count: genome.nodes.len().min(MAX_NODES) as u32,
                conn_count: conn_idx as u32,
                input_count,
                output_count,
            };

            // 上传到 GPU（两侧节点缓冲都刷一次，确保 ping/pong 初始一致）
            let node_offset = (node_base * std::mem::size_of::<GpuNode>()) as u64;
            let node_data = bytemuck::cast_slice(&self.nodes_cpu[node_base..node_base + MAX_NODES]);
            self.queue
                .write_buffer(&self.nodes_buf_a, node_offset, node_data);
            self.queue
                .write_buffer(&self.nodes_buf_b, node_offset, node_data);

            let conn_offset = (conn_base * std::mem::size_of::<GpuConnection>()) as u64;
            let conn_data =
                bytemuck::cast_slice(&self.connections_cpu[conn_base..conn_base + MAX_CONNS]);
            self.queue
                .write_buffer(&self.connections_buf, conn_offset, conn_data);

            let meta_offset = (slot * std::mem::size_of::<GpuCreatureMeta>()) as u64;
            let meta_data = bytemuck::cast_slice(&self.meta_cpu[slot..slot + 1]);
            self.queue
                .write_buffer(&self.meta_buf, meta_offset, meta_data);

            // 清零该 slot 的 eligibility traces
            self.clear_slot_traces(slot);
        }

        pub fn clear_slot(&mut self, slot: usize) {
            if slot >= MAX_CREATURES {
                return;
            }
            self.meta_cpu[slot] = GpuCreatureMeta::default();
            let meta_offset = (slot * std::mem::size_of::<GpuCreatureMeta>()) as u64;
            let meta_data = bytemuck::cast_slice(&self.meta_cpu[slot..slot + 1]);
            self.queue
                .write_buffer(&self.meta_buf, meta_offset, meta_data);
            self.clear_slot_traces(slot);
        }

        /// 清零指定 slot 的 eligibility traces（CPU 镜像 + GPU 缓冲）
        fn clear_slot_traces(&mut self, slot: usize) {
            let trace_base = slot * MAX_CONNS;
            for i in 0..MAX_CONNS {
                self.traces_cpu[trace_base + i] = 0.0;
            }
            self.upload_traces_slot(slot);
        }

        /// 上传学习参数到指定 slot
        fn upload_learning_params(&mut self, slot: usize, params: &GpuLearningParams) {
            let offset = (slot * std::mem::size_of::<GpuLearningParams>()) as u64;
            let data = bytemuck::cast_slice(std::slice::from_ref(params));
            self.queue
                .write_buffer(&self.learning_params_buf, offset, data);
        }

        /// 上传指定 slot 的 connections 到 GPU（权重更新后调用）
        fn upload_connections_slot(&mut self, slot: usize) {
            let conn_base = slot * MAX_CONNS;
            let offset = (conn_base * std::mem::size_of::<GpuConnection>()) as u64;
            let data =
                bytemuck::cast_slice(&self.connections_cpu[conn_base..conn_base + MAX_CONNS]);
            self.queue.write_buffer(&self.connections_buf, offset, data);
        }

        /// 上传指定 slot 的 traces 到 GPU（衰减写回后调用）
        fn upload_traces_slot(&mut self, slot: usize) {
            let trace_base = slot * MAX_CONNS;
            let offset = (trace_base * std::mem::size_of::<f32>()) as u64;
            let data = bytemuck::cast_slice(&self.traces_cpu[trace_base..trace_base + MAX_CONNS]);
            self.queue
                .write_buffer(&self.eligibility_traces_buf, offset, data);
        }

        /// 阻塞读回全部 eligibility traces（apply_rewards 前调用）
        fn readback_traces_blocking(&mut self) {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("readback-traces"),
                });
            encoder.copy_buffer_to_buffer(
                &self.eligibility_traces_buf,
                0,
                &self.traces_staging_buf,
                0,
                TRACES_BYTES,
            );
            self.queue.submit(Some(encoder.finish()));

            let slice = self.traces_staging_buf.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            self.device.poll(wgpu::Maintain::Wait);
            match rx.recv() {
                Ok(Ok(())) => {
                    let data = slice.get_mapped_range();
                    let f32_slice: &[f32] = bytemuck::cast_slice(&data);
                    self.traces_cpu.copy_from_slice(f32_slice);
                    drop(data);
                    self.traces_staging_buf.unmap();
                }
                _ => {
                    eprintln!("[gpu] traces readback 失败");
                }
            }
        }

        /// 上传感知输入到当前 read 侧的 nodes 缓冲
        pub fn upload_inputs(&mut self, slot: usize, perception: &[f64; 20]) {
            if slot >= MAX_CREATURES {
                return;
            }
            let node_base = slot * MAX_NODES;
            let input_count = self.meta_cpu[slot].input_count as usize;

            for i in 0..input_count.min(crate::neural::genome::Genome::INPUT_SIZE) {
                self.nodes_cpu[node_base + i].membrane = perception[i] as f32;
                self.nodes_cpu[node_base + i].set_fired(true);
            }

            let offset = (node_base * std::mem::size_of::<GpuNode>()) as u64;
            let input_size = input_count.min(MAX_NODES);
            let data = bytemuck::cast_slice(&self.nodes_cpu[node_base..node_base + input_size]);
            // 写入"下次 dispatch 将读取"的缓冲
            let target = if self.ping {
                &self.nodes_buf_a
            } else {
                &self.nodes_buf_b
            };
            self.queue.write_buffer(target, offset, data);
        }

        /// 清零 spike_counts 和 first_outputs（每个批开始时调用）
        pub fn clear_counters(&mut self) {
            self.queue
                .write_buffer(&self.spike_counts_buf, 0, &self.zero_counter_bytes);
            self.queue
                .write_buffer(&self.first_outputs_buf, 0, &self.zero_counter_bytes);
        }

        /// 写入当前 tick_index 到 uniform（单 tick 路径专用；批处理走 dispatch_batch）
        #[allow(dead_code)]
        fn write_tick_index(&mut self, tick_index: u32) {
            let data = [tick_index, 0u32, 0u32, 0u32];
            self.queue
                .write_buffer(&self.tick_params_buf, 0, bytemuck::cast_slice(&data));
        }

        /// 执行一个 GPU tick（独立 submit，保留作单 tick 调试用，正常路径走 dispatch_batch）
        #[allow(dead_code)]
        pub fn dispatch_tick(&mut self, tick_index: u32) {
            self.write_tick_index(tick_index);

            let bind_group = if self.ping {
                &self.bind_group_ping
            } else {
                &self.bind_group_pong
            };

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("snn-tick"),
                });

            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("snn-tick-pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                let workgroups = (MAX_CREATURES * MAX_NODES + 63) / 64;
                pass.dispatch_workgroups(workgroups as u32, 1, 1);
            }

            self.queue.submit(Some(encoder.finish()));
            self.ping = !self.ping;
        }

        /// 把 tick_count 个 dispatch 合并到一个 encoder 一次 submit
        /// 通过 encoder.copy_buffer_to_buffer 在 GPU 时间线内交替更新 tick_index_buf：
        ///   copy(bank[i] → tick_params) → compute_pass(读 tick_params + ping/pong nodes)
        /// wgpu 在 compute_pass 之间自动插入 buffer barrier，
        /// 节点 ping-pong 状态依赖与 10 次独立 submit 完全等价
        pub fn dispatch_batch(&mut self, tick_count: usize) {
            assert!(
                tick_count <= TICK_BANK_CAPACITY,
                "tick_count {} exceeds TICK_BANK_CAPACITY {}",
                tick_count,
                TICK_BANK_CAPACITY
            );

            let workgroups = ((MAX_CREATURES * MAX_NODES + 63) / 64) as u32;
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("snn-batch"),
                });

            for i in 0..tick_count {
                // 把 bank[i] 拷到 tick_params（GPU 时间线内有序）
                encoder.copy_buffer_to_buffer(
                    &self.tick_index_bank_buf,
                    (i as u64) * TICK_PARAMS_BYTES,
                    &self.tick_params_buf,
                    0,
                    TICK_PARAMS_BYTES,
                );

                let bind_group = if self.ping {
                    &self.bind_group_ping
                } else {
                    &self.bind_group_pong
                };

                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("snn-batch-pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.dispatch_workgroups(workgroups, 1, 1);
                }

                self.ping = !self.ping;
            }

            self.queue.submit(Some(encoder.finish()));
        }

        /// 阻塞读回 spike_counts + first_outputs（批结束时调用一次）
        pub fn readback_all_blocking(&mut self) {
            // 1. copy 两段到 staging
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("readback-all"),
                });
            encoder.copy_buffer_to_buffer(
                &self.spike_counts_buf,
                0,
                &self.staging_buf,
                0,
                COUNTER_BYTES,
            );
            encoder.copy_buffer_to_buffer(
                &self.first_outputs_buf,
                0,
                &self.staging_buf,
                COUNTER_BYTES,
                COUNTER_BYTES,
            );
            self.queue.submit(Some(encoder.finish()));

            // 2. map_async 阻塞等待
            let slice = self.staging_buf.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            // Maintain::Wait 让驱动自己推进，不做 CPU 空转
            self.device.poll(wgpu::Maintain::Wait);
            match rx.recv() {
                Ok(Ok(())) => {
                    let data = slice.get_mapped_range();
                    self.last_raw.copy_from_slice(&data);
                    drop(data);
                    self.staging_buf.unmap();
                }
                _ => {
                    // 读取失败，last_raw 保持原值
                }
            }
        }

        /// 从 last_raw 读取指定 slot 的脉冲计数和直读输出
        pub fn slot_raw(&self, slot: usize) -> ([u32; 8], [f32; 8]) {
            let mut spikes = [0u32; 8];
            let mut firsts = [0.0f32; 8];
            let base_bytes = slot * OUTPUTS_PER_CREATURE * 4;
            let spike_src = &self.last_raw[base_bytes..base_bytes + OUTPUTS_PER_CREATURE * 4];
            let first_base = COUNTER_BYTES as usize + base_bytes;
            let first_src = &self.last_raw[first_base..first_base + OUTPUTS_PER_CREATURE * 4];

            for i in 0..8 {
                spikes[i] = u32::from_le_bytes([
                    spike_src[i * 4],
                    spike_src[i * 4 + 1],
                    spike_src[i * 4 + 2],
                    spike_src[i * 4 + 3],
                ]);
                firsts[i] = f32::from_le_bytes([
                    first_src[i * 4],
                    first_src[i * 4 + 1],
                    first_src[i * 4 + 2],
                    first_src[i * 4 + 3],
                ]);
            }

            (spikes, firsts)
        }
    }

    /// GPU 执行器（实现 TickExecutor trait）
    pub struct GpuExecutor {
        gpu: GpuCompute,
        slots: SlotAllocator,
        /// 输出模式缓存：creature_id -> output_modes (true=直读)
        output_modes_cache: FxHashMap<u64, Vec<bool>>,
        /// 上批 tick 数（用于脉冲发放率计算）
        last_tick_count: u32,
        /// 上批整体耗时（所有生物均摊）
        last_batch_ns: u64,
        /// 学习基因缓存：creature_id -> LearningGene
        learning_genes: FxHashMap<u64, LearningGene>,
    }

    impl GpuExecutor {
        pub fn new() -> Option<Self> {
            let gpu = GpuCompute::new()?;
            Some(Self {
                gpu,
                slots: SlotAllocator::new(MAX_CREATURES),
                output_modes_cache: FxHashMap::default(),
                last_tick_count: 0,
                last_batch_ns: 0,
                learning_genes: FxHashMap::default(),
            })
        }
    }

    impl TickExecutor for GpuExecutor {
        fn apply_rewards(&mut self, rewards: &[CreatureReward]) {
            if rewards.is_empty() {
                return;
            }

            // 一次性读回全部 traces（256KB，PCIe 4.0 ~8μs 数据传输）
            self.gpu.readback_traces_blocking();

            for r in rewards {
                let Some(slot) = self.slots.get_slot(r.creature_id) else {
                    continue;
                };
                let Some(gene) = self.learning_genes.get(&r.creature_id) else {
                    continue;
                };

                if gene.learning_on < 0.5 {
                    continue;
                }
                if r.total_reward.abs() < 0.001 {
                    continue;
                }

                let sign = (gene.hebbian_sign - 0.5) * 2.0; // -1 ~ 1
                let rate = gene.hebbian_rate;
                let conn_count = self.gpu.meta_cpu[slot].conn_count as usize;
                let conn_base = slot * MAX_CONNS;

                for c in 0..conn_count {
                    let trace = self.gpu.traces_cpu[conn_base + c];
                    if trace.abs() < 0.001 {
                        continue;
                    }
                    let delta = rate * (trace as f64) * r.total_reward * sign;
                    let conn = &mut self.gpu.connections_cpu[conn_base + c];
                    conn.weight = (conn.weight as f64 + delta).clamp(-2.0, 2.0) as f32;

                    // Post-apply trace 衰减（对齐 CPU 版 apply_physiology 的 traces *= 0.1）
                    self.gpu.traces_cpu[conn_base + c] *= 0.1;
                }

                // 回写修改后的 connections 和 traces 到 GPU
                self.gpu.upload_connections_slot(slot);
                self.gpu.upload_traces_slot(slot);
            }
        }

        fn register(&mut self, id: u64, genome: &Genome) {
            if let Some(slot) = self.slots.allocate(id) {
                self.gpu.upload_genome(slot, genome);
                let output_modes: Vec<bool> = genome
                    .nodes
                    .iter()
                    .filter(|n| matches!(n.node_type, super::super::genome::NodeType::Output))
                    .map(|n| n.threshold == 0.0)
                    .collect();
                self.output_modes_cache.insert(id, output_modes);

                // 存储学习基因并上传学习参数到 GPU
                self.learning_genes.insert(id, genome.learning.clone());
                let params = GpuLearningParams {
                    trace_decay: genome.learning.eligibility_decay as f32,
                    learning_on: genome.learning.learning_on as f32,
                    _pad0: 0.0,
                    _pad1: 0.0,
                };
                self.gpu.upload_learning_params(slot, &params);
            }
        }

        fn unregister(&mut self, id: u64) {
            if let Some(slot) = self.slots.get_slot(id) {
                self.gpu.clear_slot(slot);
            }
            self.slots.free(id);
            self.output_modes_cache.remove(&id);
            self.learning_genes.remove(&id);
        }

        fn run_batch(&mut self, inputs: &[CreatureInput], tick_count: usize) {
            let t0 = Instant::now();
            let n = tick_count.max(1);
            self.last_tick_count = n as u32;

            // 1. 上传本批次输入到当前 read 侧缓冲
            for input in inputs {
                if let Some(slot) = self.slots.get_slot(input.creature_id) {
                    self.gpu.upload_inputs(slot, &input.perception);
                }
            }

            // 2. 清零 GPU 端计数器（spike_counts + first_outputs）
            self.gpu.clear_counters();

            // 3. 一次提交执行 N 个 tick（合并 submit，driver 调度开销由 N 次降为 1 次）
            self.gpu.dispatch_batch(n);

            // 4. 批末尾一次性读回
            self.gpu.readback_all_blocking();

            self.last_batch_ns = t0.elapsed().as_nanos() as u64;
        }

        fn read_outputs(&self) -> Vec<CreatureOutput> {
            let active: Vec<(u64, usize)> = self
                .slots
                .active_entries()
                .map(|(&id, &slot)| (id, slot))
                .collect();
            let active_count = active.len().max(1) as u64;
            let per_creature_ns = self.last_batch_ns / active_count;

            let mut results = Vec::with_capacity(active.len());
            for (creature_id, slot) in active {
                let (spikes, firsts) = self.gpu.slot_raw(slot);
                let mut final_outputs = [0.0f64; 8];
                for i in 0..8 {
                    final_outputs[i] = firsts[i] as f64;
                }

                if let Some(modes) = self.output_modes_cache.get(&creature_id) {
                    if self.last_tick_count > 0 {
                        for (j, &direct_read) in modes.iter().enumerate().take(8) {
                            if !direct_read {
                                let rate = spikes[j] as f64 / self.last_tick_count as f64;
                                final_outputs[j] = rate * 2.0 - 1.0;
                            }
                        }
                    }
                }

                results.push(CreatureOutput {
                    creature_id,
                    outputs: final_outputs,
                    compute_ns: per_creature_ns,
                });
            }
            results
        }
    }
}

#[cfg(feature = "gpu")]
pub use inner::*;

// 无 GPU feature 时的存根
#[cfg(not(feature = "gpu"))]
pub mod stub {
    pub enum GpuProbeResult {
        Suitable {
            adapter_name: String,
            benchmark_us: u64,
        },
        Unsuitable {
            reason: String,
        },
    }

    pub fn probe_gpu() -> GpuProbeResult {
        GpuProbeResult::Unsuitable {
            reason: "GPU feature 未启用".into(),
        }
    }
}

#[cfg(not(feature = "gpu"))]
pub use stub::*;
