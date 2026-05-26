#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 粒子来源
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub enum ParticleSource {
    Volcano,
}

/// 能量粒子（阳光）
#[derive(Clone)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct EnergyParticle {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub initial_energy: f64,
    pub lifetime: f64,
    pub age: f64,
    pub alive: bool,
    /// 来源
    pub source: ParticleSource,
    /// 是否熔岩流粒子（死亡时链式扩散）
    #[serde(default)]
    pub lava: bool,
    /// 链式扩散代数（0=火山口直接喷出）
    #[serde(default)]
    pub chain_depth: u8,
    /// 被生物吃掉标记（吃掉不扩散）
    #[serde(default)]
    pub consumed: bool,
    /// 熔岩粒子周期性杀伤计时器
    #[serde(default)]
    pub lava_kill_timer: f64,
}

impl EnergyParticle {
    pub fn new(
        id: u64,
        x: f64,
        y: f64,
        energy: f64,
        lifetime: f64,
        source: ParticleSource,
    ) -> Self {
        Self {
            id,
            x,
            y,
            energy,
            initial_energy: energy,
            lifetime,
            age: 0.0,
            alive: true,
            source,
            lava: false,
            chain_depth: 0,
            consumed: false,
            lava_kill_timer: 0.0,
        }
    }

    /// 创建熔岩流粒子
    pub fn new_lava(id: u64, x: f64, y: f64, energy: f64, chain_depth: u8) -> Self {
        Self {
            id,
            x,
            y,
            energy,
            initial_energy: energy,
            lifetime: f64::MAX,
            age: 0.0,
            alive: true,
            source: ParticleSource::Volcano,
            lava: true,
            chain_depth,
            consumed: false,
            lava_kill_timer: 0.0,
        }
    }

    /// 更新（返回是否仍然存活）
    pub fn update(&mut self, dt: f64, decay_rate: f64, min_energy: f64) -> bool {
        self.age += dt;
        // 能量衰减
        self.energy -= self.energy * decay_rate * dt;
        // 能量过低则死亡
        if self.energy <= min_energy {
            self.alive = false;
        }
        // 保留 lifetime 硬上限（释放粒子兼容）
        if self.age >= self.lifetime {
            self.alive = false;
        }
        self.alive
    }

    /// 被消耗，返回能量值
    pub fn consume(&mut self) -> f64 {
        self.alive = false;
        self.consumed = true;
        self.energy
    }
}
