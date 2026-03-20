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

    /// GPU 节点数据（C 布局，匹配 WGSL）
    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuNode {
        pub membrane: f32,
        pub decay: f32,
        pub threshold: f32,
        pub flags: u32,
    }

    /// GPU 连接数据
    #[repr(C)]
    #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct GpuConnection {
        pub from_node: u32,
        pub to_node: u32,
        pub weight: f32,
        pub _pad: u32,
    }

    /// 每个生物的元数据
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

        // 排除集显和 CPU 模拟
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

        // 尝试创建 device
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

        // 验证 shader 编译
        let shader_source = include_str!("snn_tick.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("snn-tick-probe"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // 创建最小 pipeline 验证
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("snn-layout-probe"),
            entries: &[
                storage_entry(0, true),  // nodes_prev (read)
                storage_entry(1, false), // nodes_next (read_write)
                storage_entry(2, true),  // connections (read)
                storage_entry(3, true),  // creature_meta (read)
                storage_entry(4, false), // outputs (read_write)
            ],
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

        // 简单 benchmark：空 dispatch
        let start = Instant::now();
        for _ in 0..100 {
            let encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            // 仅提交空 encoder 测量基础开销
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

    /// GPU 计算引擎
    pub struct GpuCompute {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pipeline: wgpu::ComputePipeline,
        bind_group_layout: wgpu::BindGroupLayout,
        // 双缓冲节点
        nodes_buf_a: wgpu::Buffer,
        nodes_buf_b: wgpu::Buffer,
        connections_buf: wgpu::Buffer,
        meta_buf: wgpu::Buffer,
        outputs_buf: wgpu::Buffer,
        staging_buf: wgpu::Buffer,
        // 当前读写方向
        ping: bool,
        // CPU 侧数据（用于上传）
        nodes_cpu: Vec<GpuNode>,
        connections_cpu: Vec<GpuConnection>,
        meta_cpu: Vec<GpuCreatureMeta>,
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
                    entries: &[
                        storage_entry(0, true),
                        storage_entry(1, false),
                        storage_entry(2, true),
                        storage_entry(3, true),
                        storage_entry(4, false),
                    ],
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
            let total_outputs = MAX_CREATURES * 6;

            let nodes_size = (total_nodes * std::mem::size_of::<GpuNode>()) as u64;
            let conns_size = (total_conns * std::mem::size_of::<GpuConnection>()) as u64;
            let meta_size = (MAX_CREATURES * std::mem::size_of::<GpuCreatureMeta>()) as u64;
            let output_size = (total_outputs * std::mem::size_of::<f32>()) as u64;

            let usage_rw = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC;

            let nodes_buf_a = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nodes_a"),
                size: nodes_size,
                usage: usage_rw,
                mapped_at_creation: false,
            });
            let nodes_buf_b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nodes_b"),
                size: nodes_size,
                usage: usage_rw,
                mapped_at_creation: false,
            });
            let connections_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("connections"),
                size: conns_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let meta_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("meta"),
                size: meta_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let outputs_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("outputs"),
                size: output_size,
                usage: usage_rw,
                mapped_at_creation: false,
            });
            let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging"),
                size: output_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            Some(Self {
                device,
                queue,
                pipeline,
                bind_group_layout,
                nodes_buf_a,
                nodes_buf_b,
                connections_buf,
                meta_buf,
                outputs_buf,
                staging_buf,
                ping: true,
                nodes_cpu: vec![GpuNode::default(); total_nodes],
                connections_cpu: vec![GpuConnection::default(); total_conns],
                meta_cpu: vec![GpuCreatureMeta::default(); MAX_CREATURES],
            })
        }

        /// 上传基因组到指定 slot
        pub fn upload_genome(&mut self, slot: usize, genome: &Genome) {
            if slot >= MAX_CREATURES {
                return;
            }

            let node_base = slot * MAX_NODES;
            let conn_base = slot * MAX_CONNS;

            // 清空 slot
            for i in 0..MAX_NODES {
                self.nodes_cpu[node_base + i] = GpuNode::default();
            }
            for i in 0..MAX_CONNS {
                self.connections_cpu[conn_base + i] = GpuConnection::default();
            }

            // 建立 genome node_id → 局部索引映射
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
                    NodeType::Hidden => {}
                }

                self.nodes_cpu[node_base + local_idx] = gpu_node;
            }

            // 上传连接
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

            // 上传到 GPU
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

        /// 清空 slot
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

        /// 上传感知输入（设置输入节点的 membrane 和 fired）
        pub fn upload_inputs(&mut self, slot: usize, perception: &[f64; 10]) {
            if slot >= MAX_CREATURES {
                return;
            }
            let node_base = slot * MAX_NODES;
            let input_count = self.meta_cpu[slot].input_count as usize;

            for i in 0..input_count.min(10) {
                self.nodes_cpu[node_base + i].membrane = perception[i] as f32;
                self.nodes_cpu[node_base + i].set_fired(true);
            }

            let offset = (node_base * std::mem::size_of::<GpuNode>()) as u64;
            let input_size = input_count.min(MAX_NODES);
            let data = bytemuck::cast_slice(&self.nodes_cpu[node_base..node_base + input_size]);
            let buf = if self.ping {
                &self.nodes_buf_a
            } else {
                &self.nodes_buf_b
            };
            self.queue.write_buffer(buf, offset, data);
        }

        /// 执行一个 GPU tick
        pub fn dispatch_tick(&mut self) {
            let (read_buf, write_buf) = if self.ping {
                (&self.nodes_buf_a, &self.nodes_buf_b)
            } else {
                (&self.nodes_buf_b, &self.nodes_buf_a)
            };

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("snn-bind-group"),
                layout: &self.bind_group_layout,
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
                        resource: self.connections_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.meta_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.outputs_buf.as_entire_binding(),
                    },
                ],
            });

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
                pass.set_bind_group(0, &bind_group, &[]);
                // 总线程数 = MAX_CREATURES * MAX_NODES，workgroup_size = 64
                let workgroups = (MAX_CREATURES * MAX_NODES + 63) / 64;
                pass.dispatch_workgroups(workgroups as u32, 1, 1);
            }

            self.queue.submit(Some(encoder.finish()));
            self.ping = !self.ping;
        }

        /// 读回输出（阻塞）
        pub fn readback_outputs(&self) -> Vec<f32> {
            let output_count = MAX_CREATURES * 6;
            let output_size = (output_count * std::mem::size_of::<f32>()) as u64;

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("readback"),
                });
            encoder.copy_buffer_to_buffer(&self.outputs_buf, 0, &self.staging_buf, 0, output_size);
            self.queue.submit(Some(encoder.finish()));

            let slice = self.staging_buf.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            self.device.poll(wgpu::Maintain::Wait);
            let _ = rx.recv();

            let data = slice.get_mapped_range();
            let result: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
            drop(data);
            self.staging_buf.unmap();
            result
        }
    }

    /// GPU 执行器（实现 TickExecutor trait）
    pub struct GpuExecutor {
        gpu: GpuCompute,
        slots: SlotAllocator,
        last_tick_ns: u64,
    }

    impl GpuExecutor {
        pub fn new() -> Option<Self> {
            let gpu = GpuCompute::new()?;
            Some(Self {
                gpu,
                slots: SlotAllocator::new(MAX_CREATURES),
                last_tick_ns: 0,
            })
        }
    }

    impl TickExecutor for GpuExecutor {
        fn register(&mut self, id: u64, genome: &Genome) {
            if let Some(slot) = self.slots.allocate(id) {
                self.gpu.upload_genome(slot, genome);
            }
        }

        fn unregister(&mut self, id: u64) {
            if let Some(slot) = self.slots.get_slot(id) {
                self.gpu.clear_slot(slot);
            }
            self.slots.free(id);
        }

        fn inject_inputs(&mut self, inputs: &[CreatureInput]) {
            for input in inputs {
                if let Some(slot) = self.slots.get_slot(input.creature_id) {
                    self.gpu.upload_inputs(slot, &input.perception);
                }
            }
        }

        fn tick(&mut self) {
            let t0 = Instant::now();
            self.gpu.dispatch_tick();
            self.last_tick_ns = t0.elapsed().as_nanos() as u64;
        }

        fn read_outputs(&self) -> Vec<CreatureOutput> {
            let raw = self.gpu.readback_outputs();
            let active_count = self.slots.active_entries().count().max(1) as u64;
            let per_creature_ns = self.last_tick_ns / active_count;
            let mut results = Vec::new();
            for (&creature_id, &slot) in self.slots.active_entries() {
                let base = slot * 6;
                if base + 6 <= raw.len() {
                    let mut outputs = [0.0f64; 6];
                    for i in 0..6 {
                        outputs[i] = raw[base + i] as f64;
                    }
                    results.push(CreatureOutput {
                        creature_id,
                        outputs,
                        compute_ns: per_creature_ns,
                    });
                }
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
