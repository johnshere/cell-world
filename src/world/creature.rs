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

    // 血缘（None = 自然生成，无祖先）
    pub parent_id: Option<u64>,

    // 缓存
    pub genome_hash: u64,

    // 体温：上次感温时间（世界时间）
    pub last_warm_time: f64,

    // 当前速度（每帧更新，用于战力计算）
    pub current_speed: f64,

    // 感知结果缓存（14维：鼻子5 + 左眼3 + 右眼3 + 自身3）
    pub perception_cache: [f64; 14],

    // 器官冷却计时器（<=0 可触发）
    pub nose_cooldown_timer: f64,
    pub eye_cooldown_timer: f64,
    pub mouth_cooldown_timer: f64,

    // 痕迹生成计时器（<=0 可生成）
    pub trail_emit_timer: f64,
}

impl Creature {
    pub fn new(id: u64, x: f64, y: f64, energy: f64, genome: Genome, generation: usize, parent_id: Option<u64>) -> Self {
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
            parent_id,
            genome_hash,
            last_warm_time: 0.0,
            current_speed: 0.0,
            perception_cache: [0.0; 14],
            nose_cooldown_timer: 0.0,
            eye_cooldown_timer: 0.0,
            mouth_cooldown_timer: 0.0,
            trail_emit_timer: 0.0,
        }
    }

    /// 创建随机生物（第0代，无祖先）
    pub fn random(
        id: u64,
        x: f64,
        y: f64,
        energy: f64,
        min_connections: usize,
        max_connections: usize,
    ) -> Self {
        let genome = Genome::random_minimal(min_connections, max_connections);
        Self::new(id, x, y, energy, genome, 0, None)
    }

    /// 繁殖产生子代（代数+1，parent_id = 自己的 id）
    pub fn reproduce(&self, id: u64, x: f64, y: f64, energy: f64, mutation_rate: f64) -> Self {
        let child_genome = self.genome.mutate(mutation_rate);
        Self::new(id, x, y, energy, child_genome, self.generation + 1, Some(self.id))
    }
}
