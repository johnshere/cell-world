use rand::Rng;

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::neural::{Genome, PhysioGene, SpikingNetwork};

/// 生理状态缓冲（世界注入，帧内累积，apply 后清零）
#[derive(Clone, Default)]
pub struct PhysioState {
    /// 能量吸收快乐
    pub pleasure_energy: f64,
    /// 痕迹吸收快乐
    pub pleasure_trail: f64,
    /// 集体行为快乐（跟随+群居散热）
    pub pleasure_group: f64,
}

impl PhysioState {
    /// 清零所有通道（帧开始时调用）
    pub fn clear(&mut self) {
        self.pleasure_energy = 0.0;
        self.pleasure_trail = 0.0;
        self.pleasure_group = 0.0;
    }

    /// 各通道乘以对应敏感度基因后求和，返回最终学习信号
    pub fn total_reward(&self, gene: &PhysioGene) -> f64 {
        self.pleasure_energy * gene.pleasure_energy_sensitivity
            + self.pleasure_trail * gene.pleasure_trail_sensitivity
            + self.pleasure_group * gene.pleasure_group_sensitivity
    }
}

/// 生物
#[derive(Clone)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
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
    #[cfg_attr(feature = "persistence", serde(skip))]
    pub brain: SpikingNetwork,
    pub generation: usize,

    // 血缘（None = 自然生成，无祖先）
    pub parent_id: Option<u64>,

    // 缓存
    pub genome_hash: u64,

    // 种族颜色标识（出生时确定，终身不变）
    pub clan_hash: u64,

    // 当前速度（每帧更新，用于战力计算）
    pub current_speed: f64,

    // 跟随度（指数平滑后的值，0~1）
    pub follow_level: f64,

    // 感知结果缓存（20维：左眼8 + 右眼8 + 自身1 + 地形1 + 发光感知2）
    // 扫描眼逐帧增量更新，必须持久化
    pub perception_cache: [f64; 20],

    // 上一帧 SNN 输出缓存
    pub last_outputs: [f64; 8],

    // 扫描眼偏移量（弧度，[0]=左眼，[1]=右眼，从0到total_fov循环）
    pub eye_scan_offset: [f64; 2],
    pub mouth_cooldown_timer: f64,

    // 痕迹生成计时器（<=0 可生成）
    pub trail_emit_timer: f64,

    // 本帧 CPU 计算耗时（纳秒）
    pub frame_compute_ns: u64,

    // 本帧生理状态（世界注入，帧末 apply 后失效，不持久化）
    #[cfg_attr(feature = "persistence", serde(skip))]
    pub physio: PhysioState,

    // 发光器官强度（0~1，量化到一位小数）
    pub light_intensity: f64,

    // 发光感知扫描偏移（弧度，0~2π 循环）
    pub light_scan_offset: f64,
}

impl Creature {
    pub fn new(
        id: u64,
        x: f64,
        y: f64,
        energy: f64,
        genome: Genome,
        generation: usize,
        parent_id: Option<u64>,
        heading: Option<f64>,
    ) -> Self {
        let brain = SpikingNetwork::from_genome(&genome);
        let genome_hash = genome.hash();
        let heading =
            heading.unwrap_or_else(|| rand::thread_rng().gen_range(0.0..std::f64::consts::TAU));

        Self {
            id,
            x,
            y,
            energy,
            age: 0.0,
            alive: true,
            heading,
            genome,
            brain,
            generation,
            parent_id,
            genome_hash,
            clan_hash: genome_hash,
            current_speed: 0.0,
            follow_level: 0.0,
            perception_cache: [0.0; 20],
            last_outputs: [0.0; 8],
            eye_scan_offset: [0.0; 2],
            mouth_cooldown_timer: 0.0,
            trail_emit_timer: 0.0,
            frame_compute_ns: 0,
            physio: PhysioState::default(),
            light_intensity: 0.0,
            light_scan_offset: 0.0,
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
        Self::new(id, x, y, energy, genome, 0, None, None)
    }

    /// 繁殖产生子代（代数+1，parent_id = 自己的 id）
    pub fn reproduce(&self, id: u64, x: f64, y: f64, energy: f64, conf: &Config) -> Self {
        let mut child_genome = self.genome.mutate(conf, self.age);
        // 子代发育时间 = (父代年龄 + 父代发育时间) / 2
        child_genome.maturation_time = (self.age + self.genome.maturation_time) / 2.0;
        Self::new(
            id,
            x,
            y,
            energy,
            child_genome,
            self.generation + 1,
            Some(self.id),
            Some(self.heading),
        )
    }
}
