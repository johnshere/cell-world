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

    // 朝向（弧度，0 = 右，π/2 = 下）
    pub heading: f64,

    // 遗传
    pub genome: Genome,
    pub brain: Network,
    pub generation: usize,

    // 缓存
    pub genome_hash: u64,

    // 感知结果缓存（10维：3眼×3通道 + 自身能量）
    pub perception_cache: [f64; 10],
}

impl Creature {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, genome: Genome, generation: usize) -> Self {
        let brain = Network::from_genome(&genome);
        let genome_hash = genome.hash();

        Self {
            id,
            x,
            y,
            energy,
            age: 0.0,
            alive: true,
            heading: 0.0,
            genome,
            brain,
            generation,
            genome_hash,
            perception_cache: [0.0; 10],
        }
    }

    /// 创建随机生物（第0代）
    pub fn random(
        id: u64,
        x: f64,
        y: f64,
        energy: f64,
        min_connections: usize,
        max_connections: usize,
    ) -> Self {
        let genome = Genome::random_minimal(min_connections, max_connections);
        Self::new(id, x, y, energy, genome, 0)
    }

    /// 繁殖产生子代（代数+1）
    pub fn reproduce(&self, id: u64, x: f64, y: f64, energy: f64, mutation_rate: f64) -> Self {
        let child_genome = self.genome.mutate(mutation_rate);
        Self::new(id, x, y, energy, child_genome, self.generation + 1)
    }
}
