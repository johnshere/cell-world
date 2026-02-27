/// 痕迹点（生物移动时留下的能量痕迹）
pub struct TrailPoint {
    pub x: f64,
    pub y: f64,
    pub energy: f64,
    pub initial_energy: f64,
    pub genome_hash: u64,
    pub age: f64,
    pub alive: bool,
}

impl TrailPoint {
    pub fn new(x: f64, y: f64, energy: f64, genome_hash: u64) -> Self {
        Self {
            x,
            y,
            energy,
            initial_energy: energy,
            genome_hash,
            age: 0.0,
            alive: true,
        }
    }

    /// 更新衰减
    pub fn update(&mut self, dt: f64, decay_rate: f64) {
        self.age += dt;
        self.energy *= 1.0 - decay_rate * dt;
        if self.energy < 0.05 {
            self.alive = false;
        }
    }

    /// 被吸收，返回能量值
    pub fn consume(&mut self) -> f64 {
        self.alive = false;
        self.energy
    }
}
