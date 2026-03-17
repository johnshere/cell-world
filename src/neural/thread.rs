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
    fn set_inputs(&mut self, inputs: &[CreatureInput]);
    fn tick(&mut self);
    fn read_outputs(&self) -> Vec<CreatureOutput>;
}

/// CPU 执行器
pub struct CpuExecutor {
    networks: FxHashMap<u64, SpikingNetwork>,
    current_inputs: FxHashMap<u64, [f64; 10]>,
    current_outputs: FxHashMap<u64, [f64; 6]>,
    compute_times: FxHashMap<u64, u64>,
}

impl CpuExecutor {
    pub fn new() -> Self {
        Self {
            networks: FxHashMap::default(),
            current_inputs: FxHashMap::default(),
            current_outputs: FxHashMap::default(),
            compute_times: FxHashMap::default(),
        }
    }
}

impl TickExecutor for CpuExecutor {
    fn register(&mut self, id: u64, genome: &Genome) {
        let network = SpikingNetwork::from_genome(genome);
        self.networks.insert(id, network);
        self.current_outputs.insert(id, [0.0; 6]);
    }

    fn unregister(&mut self, id: u64) {
        self.networks.remove(&id);
        self.current_inputs.remove(&id);
        self.current_outputs.remove(&id);
    }

    fn set_inputs(&mut self, inputs: &[CreatureInput]) {
        // 清除旧输入，仅保留本帧有新数据的
        self.current_inputs.clear();
        self.compute_times.clear();
        for input in inputs {
            self.current_inputs
                .insert(input.creature_id, input.perception);
        }
    }

    fn tick(&mut self) {
        for (&id, network) in self.networks.iter_mut() {
            let input = self
                .current_inputs
                .get(&id)
                .map(|p| &p[..])
                .unwrap_or(&[0.0; 10]);
            let t0 = Instant::now();
            let outputs = network.tick(input);
            let elapsed_ns = t0.elapsed().as_nanos() as u64;
            *self.compute_times.entry(id).or_insert(0) += elapsed_ns;
            if outputs.len() >= 6 {
                let mut arr = [0.0; 6];
                arr.copy_from_slice(&outputs[..6]);
                self.current_outputs.insert(id, arr);
            }
        }
        // tick 之后清除输入，后续 tick 用零输入
        self.current_inputs.clear();
    }

    fn read_outputs(&self) -> Vec<CreatureOutput> {
        self.current_outputs
            .iter()
            .map(|(&id, &outputs)| CreatureOutput {
                creature_id: id,
                outputs,
                compute_ns: self.compute_times.get(&id).copied().unwrap_or(0),
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

        // 读取输入
        let inputs = handle.read_inputs();
        executor.set_inputs(&inputs);

        // 执行一个 tick
        executor.tick();

        // 写回输出
        let outputs = executor.read_outputs();
        handle.write_outputs(outputs);
    }

    eprintln!("[neural-snn] 神经线程退出");
}
