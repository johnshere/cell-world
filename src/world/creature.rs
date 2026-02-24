use crate::neural::Genome;
use crate::neural::Network;

/// 扫描到的个体信息
#[derive(Clone, Default)]
pub struct ScanResult {
    pub angle: f64,      // 角度（0~360）
    pub distance: f64,   // 距离
    pub similarity: f64, // 相似度
    pub energy: f64,     // 能量值
}

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
    pub generation: usize,

    // 缓存
    pub genome_hash: u64,

    // 雷达扫描状态
    pub scan_angle: f64,              // 当前扫描角度（0~360）
    pub scan_radius: f64,             // 当前扫描半径
    pub scan_angular_velocity: f64,   // 当前角速度（度/秒）
    pub scan_cache: Vec<ScanResult>,  // 扫描到的个体缓存
    pub last_scan_degree: i32,        // 上次结算的整度数
    pub perception_cache: [f64; 17],  // 感知结果缓存
    pub last_perception_time: f64,    // 上次计算感知的时间
}

impl Creature {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, genome: Genome, family_id: usize, generation: usize) -> Self {
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
            generation,
            genome_hash,
            // 扫描状态初始化
            scan_angle: 0.0,
            scan_radius: 50.0,            // 初始使用免费半径
            scan_angular_velocity: 90.0,   // 初始角速度
            scan_cache: Vec::new(),
            last_scan_degree: 0,
            perception_cache: [0.0; 17],
            last_perception_time: 0.0,
        }
    }

    /// 创建随机生物（第0代）
    pub fn random(
        id: u64,
        x: f64,
        y: f64,
        energy: f64,
        family_id: usize,
        min_connections: usize,
        max_connections: usize,
    ) -> Self {
        let genome = Genome::random_minimal(min_connections, max_connections);
        Self::new(id, x, y, energy, genome, family_id, 0)
    }

    /// 繁殖产生子代（代数+1）
    pub fn reproduce(&self, id: u64, x: f64, y: f64, energy: f64, mutation_rate: f64) -> Self {
        let child_genome = self.genome.mutate(mutation_rate);
        Self::new(id, x, y, energy, child_genome, self.family_id, self.generation + 1)
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
