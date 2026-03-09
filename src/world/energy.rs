/// 粒子来源
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ParticleSource {
    Volcano,
    Meteorite,
}

/// 能量粒子（阳光）
pub struct EnergyParticle {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub initial_energy: f64,
    pub lifetime: f64,
    pub age: f64,
    pub alive: bool,
    /// 来源（火山/陨石）
    pub source: ParticleSource,
}

impl EnergyParticle {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, lifetime: f64, source: ParticleSource) -> Self {
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
        }
    }

    /// 更新（返回是否仍然存活）
    pub fn update(&mut self, dt: f64, decay_rate: f64) -> bool {
        self.age += dt;
        // 能量衰减
        self.energy -= self.energy * decay_rate * dt;
        // 能量过低则死亡
        if self.energy <= 0.1 {
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
        self.energy
    }
}
