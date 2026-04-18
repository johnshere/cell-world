use rand::Rng;

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::neural::{PhysioGene, Genome, SpikingNetwork};

#[cfg(feature = "persistence")]
fn deserialize_perception_cache<'de, D>(deserializer: D) -> Result<[f64; 18], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Vec<f64> = Vec::deserialize(deserializer)?;
    let mut arr = [0.0f64; 18];
    for (i, &val) in v.iter().enumerate().take(18) {
        arr[i] = val;
    }
    Ok(arr)
}

/// 生理状态缓冲（世界注入，帧内累积，apply 后清零）
/// 框架设计：每新增一种生理通道只需加一个字段
#[derive(Clone, Default)]
pub struct PhysioState {
    /// 快乐（多巴胺类）：摄食、繁殖等正向事件
    pub pleasure: f64,
    // 后续扩展：
    // pub pain: f64,
    // pub comfort: f64,
}

impl PhysioState {
    /// 清零所有通道（帧开始时调用）
    pub fn clear(&mut self) {
        self.pleasure = 0.0;
    }

    /// 各通道乘以对应敏感度基因后求和，返回最终学习信号
    pub fn total_reward(&self, gene: &PhysioGene) -> f64 {
        let pleasure = self.pleasure * gene.pleasure_sensitivity;
        // 后续扩展：
        // let pain = -self.pain * gene.pain_sensitivity;
        // let comfort = self.comfort * gene.comfort_sensitivity;
        pleasure
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
    #[cfg_attr(feature = "persistence", serde(default))]
    pub follow_level: f64,

    // 感知结果缓存（18维：左眼8 + 右眼8 + 自身1 + 地形1）
    // 扫描眼逐帧增量更新，必须持久化；兼容旧存档 17 维（第 18 通道补 0）
    #[cfg_attr(feature = "persistence", serde(deserialize_with = "deserialize_perception_cache"))]
    pub perception_cache: [f64; 18],

    // 上一帧 SNN 输出缓存
    pub last_outputs: [f64; 7],

    // 器官冷却计时器（<=0 可触发）
    pub eye_cooldown_timer: f64, // 已废弃，保留兼容

    // 扫描眼偏移量（弧度，[0]=左眼，[1]=右眼，从0到total_fov循环）
    pub eye_scan_offset: [f64; 2],
    pub mouth_cooldown_timer: f64,

    // 痕迹生成计时器（<=0 可生成）
    pub trail_emit_timer: f64,

    // 繁殖冷却计时器（<=0 可繁殖）
    pub reproduce_cooldown_timer: f64,

    // 本帧 CPU 计算耗时（纳秒）
    pub frame_compute_ns: u64,

    // 本帧生理状态（世界注入，帧末 apply 后失效，不持久化）
    #[cfg_attr(feature = "persistence", serde(skip))]
    pub physio: PhysioState,
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
        let heading = heading.unwrap_or_else(|| rand::thread_rng().gen_range(0.0..std::f64::consts::TAU));

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
            clan_hash: genome_hash, // 默认用自身 hash，繁殖时由调用者覆盖
            current_speed: 0.0,
            follow_level: 0.0,
            perception_cache: [0.0; 18],
            last_outputs: [0.0; 7],
            eye_cooldown_timer: 0.0,
            eye_scan_offset: [0.0; 2],
            mouth_cooldown_timer: 0.0,
            trail_emit_timer: 0.0,
            reproduce_cooldown_timer: 0.0,
            frame_compute_ns: 0,
            physio: PhysioState::default(),
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
        let child_genome = self.genome.mutate(conf);
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
