use crate::neural::Genome;
use crate::neural::Network;

/// 生物
pub struct Creature {
    // 唯一标识
    pub id: u64,

    // 位置
    pub x: f64,
    pub y: f64,

    // 状态
    pub energy: f64,
    pub age: f64,
    pub alive: bool,

    // 遗传
    pub genome: Genome,
    pub brain: Network,
    pub family_id: usize,

    // 缓存
    pub genome_hash: u64,
}

impl Creature {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, genome: Genome, family_id: usize) -> Self {
        let brain = Network::from_genome(&genome);
        let genome_hash = genome.hash();

        Self {
            id,
            x,
            y,
            energy,
            age: 0.0,
            alive: true,
            genome,
            brain,
            family_id,
            genome_hash,
        }
    }

    /// 创建随机生物
    pub fn random(id: u64, x: f64, y: f64, energy: f64, family_id: usize) -> Self {
        let genome = Genome::random_minimal();
        Self::new(id, x, y, energy, genome, family_id)
    }

    /// 繁殖产生子代
    pub fn reproduce(&self, id: u64, x: f64, y: f64, energy: f64, mutation_rate: f64) -> Self {
        let child_genome = self.genome.mutate(mutation_rate);
        Self::new(id, x, y, energy, child_genome, self.family_id)
    }

    /// 计算与另一个生物的基因相似度
    pub fn similarity(&self, other: &Creature) -> f64 {
        self.genome.similarity(&other.genome)
    }

    /// 获取颜色 (HSL)
    pub fn color(&self) -> (f32, f32, f32) {
        let hue = (self.genome_hash % 360) as f32;
        let saturation = 0.3 + (self.age / 10000.0).min(1.0) as f32 * 0.7;
        let lightness = 0.2 + (self.energy / 200.0).min(1.0) as f32 * 0.5;
        (hue, saturation, lightness)
    }
}
