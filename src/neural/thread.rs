use rustc_hash::FxHashMap;
use std::thread;
use std::time::Instant;

use super::bridge::{
    BridgeServerHandle, CreatureEvent, CreatureInput, CreatureOutput, NeuralBridge, TickResponse,
};
use super::genome::Genome;
use super::spiking::SpikingNetwork;
use crate::config::Config;

/// tick 执行器接口（CPU 和 GPU 共用）
///
/// 同步批处理协议：一次 `run_batch` 对应一次世界 update 所需的全部 tick。
/// 执行器内部负责批内 tick 推进，批结束后通过 `read_outputs` 返回最终动作输出。
pub trait TickExecutor: Send {
    fn register(&mut self, id: u64, genome: &Genome);
    fn unregister(&mut self, id: u64);
    /// 一次性执行一批 tick：注入输入 + 推进 tick_count 个 tick
    fn run_batch(&mut self, inputs: &[CreatureInput], tick_count: usize);
    /// 读取上一批的最终输出（按 creature_id 返回）
    fn read_outputs(&self) -> Vec<CreatureOutput>;
}

/// CPU 执行器
pub struct CpuExecutor {
    networks: FxHashMap<u64, SpikingNetwork>,
    current_inputs: FxHashMap<u64, [f64; 18]>,
    first_outputs: FxHashMap<u64, [f64; 7]>,
    compute_times: FxHashMap<u64, u64>,
    spike_counts: FxHashMap<u64, [u32; 7]>,
    output_modes_cache: FxHashMap<u64, Vec<bool>>,
    /// 上一批实际执行的 tick 数（用于发放率计算）
    last_tick_count: u32,
}

impl CpuExecutor {
    pub fn new() -> Self {
        Self {
            networks: FxHashMap::default(),
            current_inputs: FxHashMap::default(),
            first_outputs: FxHashMap::default(),
            compute_times: FxHashMap::default(),
            spike_counts: FxHashMap::default(),
            output_modes_cache: FxHashMap::default(),
            last_tick_count: 0,
        }
    }
}

impl TickExecutor for CpuExecutor {
    fn register(&mut self, id: u64, genome: &Genome) {
        let network = SpikingNetwork::from_genome(genome);
        let modes = network.output_modes().to_vec();
        self.networks.insert(id, network);
        self.output_modes_cache.insert(id, modes);
        self.spike_counts.insert(id, [0; 7]);
    }

    fn unregister(&mut self, id: u64) {
        self.networks.remove(&id);
        self.current_inputs.remove(&id);
        self.first_outputs.remove(&id);
        self.output_modes_cache.remove(&id);
        self.spike_counts.remove(&id);
    }

    fn run_batch(&mut self, inputs: &[CreatureInput], tick_count: usize) {
        self.compute_times.clear();
        self.first_outputs.clear();
        for counts in self.spike_counts.values_mut() {
            *counts = [0; 7];
        }

        self.current_inputs.clear();
        for input in inputs {
            self.current_inputs
                .insert(input.creature_id, input.perception);
        }

        let n = tick_count.max(1);
        self.last_tick_count = n as u32;

        for tick_idx in 0..n {
            for (&id, network) in self.networks.iter_mut() {
                let t0 = Instant::now();
                let outputs = if tick_idx == 0 {
                    if let Some(input) = self.current_inputs.get(&id) {
                        network.tick(&input[..])
                    } else {
                        network.tick_free()
                    }
                } else {
                    network.tick_free()
                };
                let elapsed_ns = t0.elapsed().as_nanos() as u64;
                *self.compute_times.entry(id).or_insert(0) += elapsed_ns;

                // tick 0 捕获直读输出快照
                if tick_idx == 0 && outputs.len() >= 7 {
                    let mut arr = [0.0; 7];
                    arr.copy_from_slice(&outputs[..7]);
                    self.first_outputs.insert(id, arr);
                }

                // 脉冲累加
                if let Some(modes) = self.output_modes_cache.get(&id) {
                    if let Some(counts) = self.spike_counts.get_mut(&id) {
                        for (j, (&v, &direct_read)) in
                            outputs.iter().zip(modes.iter()).enumerate().take(7)
                        {
                            if !direct_read && v > 0.5 {
                                counts[j] += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    fn read_outputs(&self) -> Vec<CreatureOutput> {
        self.networks
            .keys()
            .map(|&id| {
                let first = self.first_outputs.get(&id).copied().unwrap_or([0.0; 7]);
                let mut final_outputs = first;
                if let Some(modes) = self.output_modes_cache.get(&id) {
                    if let Some(counts) = self.spike_counts.get(&id) {
                        if self.last_tick_count > 0 {
                            for (j, &direct_read) in modes.iter().enumerate().take(7) {
                                if !direct_read {
                                    let rate = counts[j] as f64 / self.last_tick_count as f64;
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
    let (bridge, handle) = NeuralBridge::new();
    let backend = config.neural_backend.clone();

    thread::Builder::new()
        .name("neural-snn".to_string())
        .spawn(move || {
            neural_thread_main(handle, &backend);
        })
        .expect("Failed to spawn neural thread");

    bridge
}

fn neural_thread_main(handle: BridgeServerHandle, backend: &str) {
    let mut executor: Box<dyn TickExecutor> = select_backend(backend);

    eprintln!("[neural-snn] 神经线程启动（同步驱动）");

    while handle.is_running() {
        // 阻塞等待世界请求
        let req = match handle.req_rx.recv() {
            Ok(r) => r,
            Err(_) => break, // 所有 sender drop → 世界退出
        };

        // 处理生命周期事件
        for event in req.events {
            match event {
                CreatureEvent::Born { id, genome } => {
                    executor.register(id, &genome);
                }
                CreatureEvent::Died { id } => {
                    executor.unregister(id);
                }
            }
        }

        // 执行一批 tick
        executor.run_batch(&req.inputs, req.tick_count);

        // 回传结果
        let outputs = executor.read_outputs();
        if handle
            .resp_tx
            .send(TickResponse { outputs })
            .is_err()
        {
            break;
        }
    }

    eprintln!("[neural-snn] 神经线程退出");
}
