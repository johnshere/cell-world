/// 能量粒子（阳光）
pub struct EnergyParticle {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub lifetime: f64,
    pub age: f64,
    pub alive: bool,
}

impl EnergyParticle {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, lifetime: f64) -> Self {
        Self {
            id,
            x,
            y,
            energy,
            lifetime,
            age: 0.0,
            alive: true,
        }
    }

    /// 更新（返回是否仍然存活）
    pub fn update(&mut self, dt: f64) -> bool {
        self.age += dt;
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
