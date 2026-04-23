use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use super::genome::Genome;

/// 生物感知输入
#[derive(Clone)]
pub struct CreatureInput {
    pub creature_id: u64,
    pub perception: [f64; 20],
}

/// 生物动作输出
#[derive(Clone)]
pub struct CreatureOutput {
    pub creature_id: u64,
    pub outputs: [f64; 8],
    pub compute_ns: u64,
}

/// 生物生命周期事件
pub enum CreatureEvent {
    Born { id: u64, genome: Genome },
    Died { id: u64 },
}

/// 世界线程 → 神经线程的一次 tick 批请求
pub struct TickRequest {
    pub events: Vec<CreatureEvent>,
    pub inputs: Vec<CreatureInput>,
    pub tick_count: usize,
}

/// 神经线程 → 世界线程的一次响应
pub struct TickResponse {
    pub outputs: Vec<CreatureOutput>,
}

/// 世界线程与神经线程之间的同步通信桥
///
/// 语义：世界每次 update 调用 `run_batch_sync` 提交一批请求并阻塞等待结果。
/// 神经线程完全由世界驱动，没有独立节奏。加速倍速仅影响世界调用频率，
/// 单次批的 tick_count 固定，确保"加速只是更快获得结果，不影响结果"。
pub struct NeuralBridge {
    req_tx: mpsc::Sender<TickRequest>,
    resp_rx: Mutex<mpsc::Receiver<TickResponse>>,
    /// 累积待发送的生命周期事件，run_batch_sync 时打包进请求
    pending_events: Mutex<Vec<CreatureEvent>>,
    running: Arc<AtomicBool>,
}

impl NeuralBridge {
    pub fn new() -> (Self, BridgeServerHandle) {
        let (req_tx, req_rx) = mpsc::channel();
        let (resp_tx, resp_rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let bridge = Self {
            req_tx,
            resp_rx: Mutex::new(resp_rx),
            pending_events: Mutex::new(Vec::new()),
            running: Arc::clone(&running),
        };
        let handle = BridgeServerHandle {
            req_rx,
            resp_tx,
            running,
        };
        (bridge, handle)
    }

    /// 向桥推送生命周期事件（即将在下次 run_batch_sync 打包给神经线程）
    pub fn send_event(&self, event: CreatureEvent) {
        if let Ok(mut buf) = self.pending_events.lock() {
            buf.push(event);
        }
    }

    /// 同步执行一批 tick：打包事件+感知+次数，阻塞等待神经线程返回决策输出
    pub fn run_batch_sync(
        &self,
        inputs: Vec<CreatureInput>,
        tick_count: usize,
    ) -> Vec<CreatureOutput> {
        let events = self
            .pending_events
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default();

        if self
            .req_tx
            .send(TickRequest {
                events,
                inputs,
                tick_count,
            })
            .is_err()
        {
            return Vec::new();
        }

        match self.resp_rx.lock() {
            Ok(rx) => rx.recv().map(|r| r.outputs).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// 停止神经线程
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// 神经线程持有的服务端句柄
pub struct BridgeServerHandle {
    pub req_rx: mpsc::Receiver<TickRequest>,
    pub resp_tx: mpsc::Sender<TickResponse>,
    pub running: Arc<AtomicBool>,
}

impl BridgeServerHandle {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
