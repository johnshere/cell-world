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
    pub perception: [f64; 14],
}

/// 生物动作输出
#[derive(Clone)]
pub struct CreatureOutput {
    pub creature_id: u64,
    pub outputs: [f64; 6],
    pub compute_ns: u64,
}

/// 生物生命周期事件
pub enum CreatureEvent {
    Born { id: u64, genome: Genome },
    Died { id: u64 },
}

/// 世界线程与神经线程之间的通信桥
pub struct NeuralBridge {
    /// 感知输入：世界写 front，神经读 back，帧开始时交换
    input_front: Arc<Mutex<Vec<CreatureInput>>>,
    input_back: Arc<Mutex<Vec<CreatureInput>>>,
    /// 动作输出：神经写 back，世界读 front，定期交换
    output_front: Arc<Mutex<Vec<CreatureOutput>>>,
    output_back: Arc<Mutex<Vec<CreatureOutput>>>,
    /// 生命周期事件
    event_tx: mpsc::Sender<CreatureEvent>,
    pub(crate) event_rx: Arc<Mutex<mpsc::Receiver<CreatureEvent>>>,
    /// 运行标志
    pub running: Arc<AtomicBool>,
    /// 输入是否已被消费（世界 swap 时重置，神经线程消费后置 true）
    input_consumed: Arc<AtomicBool>,
}

impl NeuralBridge {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            input_front: Arc::new(Mutex::new(Vec::new())),
            input_back: Arc::new(Mutex::new(Vec::new())),
            output_front: Arc::new(Mutex::new(Vec::new())),
            output_back: Arc::new(Mutex::new(Vec::new())),
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            running: Arc::new(AtomicBool::new(true)),
            input_consumed: Arc::new(AtomicBool::new(true)),
        }
    }

    // === 世界线程侧 API ===

    /// 写入感知数据（世界线程调用）
    pub fn write_inputs(&self, inputs: Vec<CreatureInput>) {
        if let Ok(mut front) = self.input_front.lock() {
            *front = inputs;
        }
    }

    /// 交换输入缓冲区（帧开始时调用）
    pub fn swap_inputs(&self) {
        if let (Ok(mut front), Ok(mut back)) = (self.input_front.lock(), self.input_back.lock()) {
            std::mem::swap(&mut *front, &mut *back);
        }
        self.input_consumed.store(false, Ordering::Release);
    }

    /// 读取输出（世界线程调用）
    pub fn read_outputs(&self) -> Vec<CreatureOutput> {
        if let Ok(front) = self.output_front.lock() {
            front.clone()
        } else {
            Vec::new()
        }
    }

    /// 交换输出缓冲区
    pub fn swap_outputs(&self) {
        if let (Ok(mut front), Ok(mut back)) = (self.output_front.lock(), self.output_back.lock()) {
            std::mem::swap(&mut *front, &mut *back);
        }
    }

    /// 发送生命周期事件
    pub fn send_event(&self, event: CreatureEvent) {
        let _ = self.event_tx.send(event);
    }

    /// 停止神经线程
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    // === 神经线程侧 API ===

    /// 读取输入（神经线程调用）
    pub(crate) fn read_inputs_back(&self) -> Vec<CreatureInput> {
        if let Ok(back) = self.input_back.lock() {
            back.clone()
        } else {
            Vec::new()
        }
    }

    /// 写入输出（神经线程调用）
    pub(crate) fn write_outputs_back(&self, outputs: Vec<CreatureOutput>) {
        if let Ok(mut back) = self.output_back.lock() {
            *back = outputs;
        }
    }

    /// 检查是否仍在运行
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// 创建线程侧句柄（克隆 Arc 引用）
    pub fn thread_handle(&self) -> NeuralBridgeHandle {
        NeuralBridgeHandle {
            input_back: Arc::clone(&self.input_back),
            output_back: Arc::clone(&self.output_back),
            event_rx: Arc::clone(&self.event_rx),
            running: Arc::clone(&self.running),
            input_consumed: Arc::clone(&self.input_consumed),
        }
    }
}

/// 神经线程持有的桥接句柄
pub struct NeuralBridgeHandle {
    input_back: Arc<Mutex<Vec<CreatureInput>>>,
    output_back: Arc<Mutex<Vec<CreatureOutput>>>,
    event_rx: Arc<Mutex<mpsc::Receiver<CreatureEvent>>>,
    running: Arc<AtomicBool>,
    input_consumed: Arc<AtomicBool>,
}

impl NeuralBridgeHandle {
    /// 尝试消费输入（仅在世界提供新数据后返回 Some，之后返回 None 直到下一帧）
    pub fn try_consume_inputs(&self) -> Option<Vec<CreatureInput>> {
        if self
            .input_consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if let Ok(back) = self.input_back.lock() {
                Some(back.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn write_outputs(&self, outputs: Vec<CreatureOutput>) {
        if let Ok(mut back) = self.output_back.lock() {
            *back = outputs;
        }
    }

    pub fn drain_events(&self) -> Vec<CreatureEvent> {
        let mut events = Vec::new();
        if let Ok(rx) = self.event_rx.lock() {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        events
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
