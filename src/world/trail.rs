#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 痕迹点（生物移动时留下的能量痕迹）
#[derive(Clone)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct TrailPoint {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub initial_energy: f64,
    pub clan_hash: u64,
    pub creator_id: u64,    // 创建者生物ID，主人死亡时痕迹消失
    pub visual_radius: f64, // 创建者的体型半径，用于渲染
    pub age: f64,
    pub alive: bool,
}

impl TrailPoint {
    pub fn new(
        x: f64,
        y: f64,
        energy: f64,
        clan_hash: u64,
        creator_id: u64,
        visual_radius: f64,
    ) -> Self {
        Self {
            x,
            y,
            energy,
            initial_energy: energy,
            clan_hash,
            creator_id,
            visual_radius,
            age: 0.0,
            alive: true,
        }
    }

    /// 更新衰减
    pub fn update(&mut self, dt: f64, decay_rate: f64) {
        self.age += dt;
        self.energy *= 1.0 - decay_rate * dt;
        // 纯百分比衰减，与时间相关：比率低于1%时消失
        if self.energy / self.initial_energy < 0.01 {
            self.alive = false;
        }
    }

    /// 被吸收，返回能量值
    pub fn consume(&mut self) -> f64 {
        self.alive = false;
        self.energy
    }
}
