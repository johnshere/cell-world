#[cfg(feature = "gpu")]
mod inner {
    use rustc_hash::FxHashMap;
    use std::time::Instant;

    use super::super::bridge::{CreatureInput, CreatureOutput};
    use super::super::genome::{Genome, NodeType};
    use super::super::slot_alloc::SlotAllocator;
    use super::super::thread::TickExecutor;

    pub const MAX_CREATURES: usize = 512;
    pub const MAX_NODES: usize = 64;
    pub const MAX_CONNS: usize = 128;
    pub const OUTPUTS_PER_CREATURE: usize = 7;

    const COUNTER_ELEMS: usize = MAX_CREATURES * OUTPUTS_PER_CREATURE;
    const COUNTER_BYTES: u64 = (COUNTER_ELEMS * 4) as u64; // u32 / f32 同宽
    const STAGING_BYTES: u64 = COUNTER_BYTES * 2; // spike_counts + first_outputs
    const TICK_PARAMS_BYTES: u64 = 16; // vec4<u32> 对齐

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

    fn bind_group_layout_entries() -> [wgpu::BindGroupLayoutEntry; 7] {
        [
            storage_entry(0, true),  // nodes_prev
            storage_entry(1, false), // nodes_next
            storage_entry(2, true),  // connections
            storage_entry(3, true),  // creature_meta
            storage_entry(4, false), // spike_counts (atomic)
            storage_entry(5, false), // first_outputs
            uniform_entry(6),        // tick_params
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

        // uniform：tick_index
        tick_params_buf: wgpu::Buffer,

        // 回读 staging：连续存放 spike_counts(14336) + first_outputs(14336)
        staging_buf: wgpu::Buffer,

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

            let usage_storage_rw =
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
            let usage_storage_r =
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;

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
            let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging"),
                size: STAGING_BYTES,
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
                staging_buf,
                bind_group_ping,
                bind_group_pong,
                ping: true,
                nodes_cpu: vec![GpuNode::default(); total_nodes],
                connections_cpu: vec![GpuConnection::default(); total_conns],
                meta_cpu: vec![GpuCreatureMeta::default(); MAX_CREATURES],
                zero_counter_bytes: vec![0u8; COUNTER_BYTES as usize],
                last_raw: vec![0u8; STAGING_BYTES as usize],
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
                ],
            })
        }

        /// 上传基因组到指定 slot
        pub fn upload_genome(&mut self, slot: usize, genome: &Genome) {
            if slot >= MAX_CREATURES {
                return;
            }

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
        }

        /// 上传感知输入到当前 read 侧的 nodes 缓冲
        pub fn upload_inputs(&mut self, slot: usize, perception: &[f64; 18]) {
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

        /// 写入当前 tick_index 到 uniform
        fn write_tick_index(&mut self, tick_index: u32) {
            let data = [tick_index, 0u32, 0u32, 0u32];
            self.queue
                .write_buffer(&self.tick_params_buf, 0, bytemuck::cast_slice(&data));
        }

        /// 执行一个 GPU tick（独立 submit）
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
        pub fn slot_raw(&self, slot: usize) -> ([u32; 7], [f32; 7]) {
            let mut spikes = [0u32; 7];
            let mut firsts = [0.0f32; 7];
            let base_bytes = slot * OUTPUTS_PER_CREATURE * 4;
            let spike_src =
                &self.last_raw[base_bytes..base_bytes + OUTPUTS_PER_CREATURE * 4];
            let first_base = COUNTER_BYTES as usize + base_bytes;
            let first_src =
                &self.last_raw[first_base..first_base + OUTPUTS_PER_CREATURE * 4];

            for i in 0..7 {
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
            })
        }
    }

    impl TickExecutor for GpuExecutor {
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
            }
        }

        fn unregister(&mut self, id: u64) {
            if let Some(slot) = self.slots.get_slot(id) {
                self.gpu.clear_slot(slot);
            }
            self.slots.free(id);
            self.output_modes_cache.remove(&id);
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

            // 3. 执行 N 个 tick
            for i in 0..n {
                self.gpu.dispatch_tick(i as u32);
            }

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
                let mut final_outputs = [0.0f64; 7];
                for i in 0..7 {
                    final_outputs[i] = firsts[i] as f64;
                }

                if let Some(modes) = self.output_modes_cache.get(&creature_id) {
                    if self.last_tick_count > 0 {
                        for (j, &direct_read) in modes.iter().enumerate().take(7) {
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
