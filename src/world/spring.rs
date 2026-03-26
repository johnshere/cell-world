#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 温泉（固定位置的能量源，有限寿命）
#[derive(Clone)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct HotSpring {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    /// 当前年龄（秒）
    pub age: f64,
    /// 总寿命（秒）
    pub lifetime: f64,
    /// 粒子喷出计时器
    pub emit_timer: f64,
    pub alive: bool,
}

impl HotSpring {
    pub fn new(id: u64, x: f64, y: f64, lifetime: f64) -> Self {
        Self {
            id,
            x,
            y,
            age: 0.0,
            lifetime,
            emit_timer: 0.0,
            alive: true,
        }
    }

    /// 生命周期产出系数（0.0~1.0）
    /// 成长期(0~20%): 线性上升
    /// 鼎盛期(20%~70%): 满产出
    /// 衰退期(70%~100%): 线性下降
    pub fn output_factor(&self) -> f64 {
        let ratio = self.age / self.lifetime;
        if ratio < 0.2 {
            // 成长期
            ratio / 0.2
        } else if ratio < 0.7 {
            // 鼎盛期
            1.0
        } else if ratio < 1.0 {
            // 衰退期
            1.0 - (ratio - 0.7) / 0.3
        } else {
            0.0
        }
    }

    /// 更新年龄，返回是否仍存活
    pub fn update(&mut self, dt: f64) -> bool {
        self.age += dt;
        if self.age >= self.lifetime {
            self.alive = false;
        }
        self.alive
    }
}
