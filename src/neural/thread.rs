use rustc_hash::FxHashMap;
use std::thread;
use std::time::{Duration, Instant};

use super::bridge::{
    CreatureEvent, CreatureInput, CreatureOutput, NeuralBridge, NeuralBridgeHandle,
};
use super::genome::Genome;
use super::spiking::SpikingNetwork;
use crate::config::Config;

/// tick 执行器接口（CPU 和 GPU 共用）
pub trait TickExecutor: Send {
    fn register(&mut self, id: u64, genome: &Genome);
    fn unregister(&mut self, id: u64);
    /// 注入新输入（每帧调用一次）
    fn inject_inputs(&mut self, inputs: &[CreatureInput]);
    /// 执行一个 tick（第一次有输入注入，后续 tick_free）
    fn tick(&mut self);
    fn read_outputs(&self) -> Vec<CreatureOutput>;
}

/// CPU 执行器
pub struct CpuExecutor {
    networks: FxHashMap<u64, SpikingNetwork>,
    /// 待注入的输入（注入后清空）
    pending_inputs: FxHashMap<u64, [f64; 14]>,
    current_outputs: FxHashMap<u64, [f64; 6]>,
    /// 首次 tick（注入输入时）的直读输出值
    first_outputs: FxHashMap<u64, [f64; 6]>,
    compute_times: FxHashMap<u64, u64>,
    /// 脉冲发放计数（每帧重置）
    spike_counts: FxHashMap<u64, [u32; 6]>,
    /// 帧内 tick 计数
    tick_count: u32,
    /// 输出模式缓存：creature_id -> output_modes
    output_modes_cache: FxHashMap<u64, Vec<bool>>,
}

impl CpuExecutor {
    pub fn new() -> Self {
        Self {
            networks: FxHashMap::default(),
            pending_inputs: FxHashMap::default(),
            current_outputs: FxHashMap::default(),
            first_outputs: FxHashMap::default(),
            compute_times: FxHashMap::default(),
            spike_counts: FxHashMap::default(),
            tick_count: 0,
            output_modes_cache: FxHashMap::default(),
        }
    }
}

impl TickExecutor for CpuExecutor {
    fn register(&mut self, id: u64, genome: &Genome) {
        let network = SpikingNetwork::from_genome(genome);
        let modes = network.output_modes().to_vec();
        self.networks.insert(id, network);
        self.current_outputs.insert(id, [0.0; 6]);
        self.output_modes_cache.insert(id, modes);
        self.spike_counts.insert(id, [0; 6]);
    }

    fn unregister(&mut self, id: u64) {
        self.networks.remove(&id);
        self.pending_inputs.remove(&id);
        self.current_outputs.remove(&id);
        self.first_outputs.remove(&id);
        self.output_modes_cache.remove(&id);
        self.spike_counts.remove(&id);
    }

    fn inject_inputs(&mut self, inputs: &[CreatureInput]) {
        self.pending_inputs.clear();
        self.compute_times.clear();
        self.tick_count = 0;
        for counts in self.spike_counts.values_mut() {
            *counts = [0; 6];
        }
        for input in inputs {
            self.pending_inputs
                .insert(input.creature_id, input.perception);
        }
    }

    fn tick(&mut self) {
        self.tick_count += 1;
        let inject = !self.pending_inputs.is_empty();

        for (&id, network) in self.networks.iter_mut() {
            let t0 = Instant::now();
            let outputs = if inject {
                if let Some(input) = self.pending_inputs.get(&id) {
                    network.tick(&input[..])
                } else {
                    network.tick_free()
                }
            } else {
                network.tick_free()
            };
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            *self.compute_times.entry(id).or_insert(0) += elapsed_ns;

            // 追踪脉冲发放
            if let Some(modes) = self.output_modes_cache.get(&id) {
                if let Some(counts) = self.spike_counts.get_mut(&id) {
                    for (j, (&v, &direct_read)) in
                        outputs.iter().zip(modes.iter()).enumerate().take(6)
                    {
                        if !direct_read && v > 0.5 {
                            counts[j] += 1;
                        }
                    }
                }
            }

            if outputs.len() >= 6 {
                let mut arr = [0.0; 6];
                arr.copy_from_slice(&outputs[..6]);
                // 注入帧的首次 tick：保存直读输出值
                if inject {
                    self.first_outputs.insert(id, arr);
                }
                self.current_outputs.insert(id, arr);
            }
        }
        // 输入注入后清空，后续 tick 用 tick_free（保持输入信号）
        if inject {
            self.pending_inputs.clear();
        }
    }

    fn read_outputs(&self) -> Vec<CreatureOutput> {
        self.current_outputs
            .iter()
            .map(|(&id, &_outputs)| {
                // 直读输出取首次注入时的值，脉冲输出取发放率
                let first = self.first_outputs.get(&id).copied().unwrap_or([0.0; 6]);
                let mut final_outputs = first;
                if let Some(modes) = self.output_modes_cache.get(&id) {
                    if let Some(counts) = self.spike_counts.get(&id) {
                        if self.tick_count > 0 {
                            for (j, &direct_read) in modes.iter().enumerate().take(6) {
                                if !direct_read {
                                    let rate =
                                        counts[j] as f64 / self.tick_count as f64;
                                    final_outputs[j] = rate * 2.0 - 1.0;
                                }
                            }
                        }
                    }
                }
                CreatureOutput {
                    creature_id: id,
                    outputs: final_outputs,
                    compute_ns: self.compute_times.get(&id).copied().unwrap_or(0),
                }
            })
            .collect()
    }
}

/// 根据配置选择后端
fn select_backend(backend: &str) -> Box<dyn TickExecutor> {
    match backend {
        "cpu" => {
            eprintln!("[neural] 使用 CPU 后端");
            Box::new(CpuExecutor::new())
        }
        #[cfg(feature = "gpu")]
        "gpu" => match super::gpu::GpuExecutor::new() {
            Some(gpu) => {
                eprintln!("[neural] 使用 GPU 后端（强制）");
                Box::new(gpu)
            }
            None => {
                eprintln!("[neural] GPU 后端初始化失败，回退 CPU");
                Box::new(CpuExecutor::new())
            }
        },
        "auto" | _ => {
            // auto：探测 GPU，适合则用 GPU，否则 CPU
            #[cfg(feature = "gpu")]
            {
                match super::gpu::probe_gpu() {
                    super::gpu::GpuProbeResult::Suitable {
                        adapter_name,
                        benchmark_us,
                    } => {
                        eprintln!(
                            "[neural] GPU 探测通过: {} ({}μs/tick)，尝试 GPU 后端",
                            adapter_name, benchmark_us
                        );
                        match super::gpu::GpuExecutor::new() {
                            Some(gpu) => {
                                eprintln!("[neural] GPU 后端就绪");
                                return Box::new(gpu);
                            }
                            None => {
                                eprintln!("[neural] GPU 后端初始化失败，回退 CPU");
                            }
                        }
                    }
                    super::gpu::GpuProbeResult::Unsuitable { reason } => {
                        eprintln!("[neural] GPU 不适合: {}，使用 CPU 后端", reason);
                    }
                }
            }
            #[cfg(not(feature = "gpu"))]
            {
                eprintln!("[neural] GPU feature 未启用，使用 CPU 后端");
            }
            Box::new(CpuExecutor::new())
        }
    }
}

/// 启动神经线程，返回 NeuralBridge
pub fn spawn_neural_thread(config: &Config) -> NeuralBridge {
    let bridge = NeuralBridge::new();
    let handle = bridge.thread_handle();
    let tick_rate = config.neural_tick_rate;
    let backend = config.neural_backend.clone();

    thread::Builder::new()
        .name("neural-snn".to_string())
        .spawn(move || {
            neural_thread_main(handle, tick_rate, &backend);
        })
        .expect("Failed to spawn neural thread");

    bridge
}

fn neural_thread_main(handle: NeuralBridgeHandle, tick_rate: f64, backend: &str) {
    let mut executor: Box<dyn TickExecutor> = select_backend(backend);

    let tick_interval = Duration::from_secs_f64(1.0 / tick_rate);
    let mut last_tick = Instant::now();

    eprintln!("[neural-snn] 神经线程启动，tick_rate={:.0} Hz", tick_rate);

    while handle.is_running() {
        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);

        if elapsed < tick_interval {
            // 精确等待
            let remaining = tick_interval - elapsed;
            if remaining > Duration::from_micros(100) {
                thread::sleep(remaining - Duration::from_micros(50));
            }
            // 自旋等待精确到位
            while Instant::now().duration_since(last_tick) < tick_interval {
                std::hint::spin_loop();
            }
        }

        last_tick = Instant::now();

        // 处理生命周期事件
        for event in handle.drain_events() {
            match event {
                CreatureEvent::Born { id, genome } => {
                    executor.register(id, &genome);
                }
                CreatureEvent::Died { id } => {
                    executor.unregister(id);
                }
            }
        }

        // 仅在世界提供新输入时注入（每帧一次）
        if let Some(inputs) = handle.try_consume_inputs() {
            executor.inject_inputs(&inputs);
        }

        // 执行一个 tick
        executor.tick();

        // 写回输出
        let outputs = executor.read_outputs();
        handle.write_outputs(outputs);
    }

    eprintln!("[neural-snn] 神经线程退出");
}
