use serde::{Deserialize, Serialize};

const CONFIG_PATH: &str = "config.toml";

/// 世界配置
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// 初始模拟速度
    pub initial_speed: f64,

    /// 最小生物数量（低于此值自动补充）
    pub min_creatures: usize,

    /// 低于最小生物数量时是否停止演化（不补充，暂停模拟）
    #[serde(default)]
    pub stop_on_extinction: bool,

    /// 自动投放随机生物间隔（秒），0=禁用
    #[serde(default)]
    pub auto_spawn_interval: f64,

    /// 最大生物数量（超过时禁止繁殖，0=不限制）
    #[serde(default = "default_max_creatures")]
    pub max_creatures: usize,

    /// 初始能量
    pub initial_energy: f64,

    /// 火山坐标 X
    pub volcano_x: f64,
    /// 火山坐标 Y
    pub volcano_y: f64,
    /// 火山喷发间隔（秒）
    pub volcano_interval: f64,
    /// 火山喷射半径
    pub volcano_radius: f64,
    /// 火山粒子半径分布倾角（负值让外围更密，默认-0.3）
    #[serde(default)]
    pub volcano_spread_bias: f64,
    /// 每次喷发粒子数
    pub volcano_count: usize,
    /// 火山粒子能量
    pub volcano_particle_energy: f64,
    /// 陨石降落间隔（秒）— 已废弃，保留兼容旧config
    #[serde(default)]
    pub meteorite_interval: f64,
    /// 每颗陨石粒子数 — 已废弃
    #[serde(default)]
    pub meteorite_count: usize,
    /// 陨石散布线段长度 — 已废弃
    #[serde(default)]
    pub meteorite_length: f64,
    /// 陨石粒子能量 — 已废弃
    #[serde(default)]
    pub meteorite_particle_energy: f64,
    /// 火山粒子衰减率（每秒）
    pub volcano_decay_rate: f64,
    /// 陨石粒子衰减率 — 已废弃
    #[serde(default)]
    pub meteorite_decay_rate: f64,

    /// 基础代谢率（已废弃，保留兼容旧config）
    #[serde(default = "default_base_metabolism")]
    pub base_metabolism: f64,
    /// 年龄代谢倍率（age × 此值 = 额外倍率，年龄越大消耗越高）
    pub age_metabolism_factor: f64,
    /// 代谢指数（代谢 ∝ energy^此值，越大则大体型惩罚越重，1=线性）
    #[serde(default = "default_metabolism_exponent")]
    pub metabolism_exponent: f64,
    /// 最大速度（px/s）
    #[serde(default = "default_max_speed")]
    pub max_speed: f64,
    /// 移动消耗系数（面板值 ×0.00001 × 半径³ = 实际消耗 每单位距离*速度）
    pub move_cost: f64,
    /// 跟随省力折扣（前方有同向生物时移动消耗最大减少比例）
    #[serde(default = "default_follow_cost_discount")]
    pub follow_cost_discount: f64,

    /// 跟随最省力方位角（弧度，左右对称双峰，默认 30°≈0.524 rad）
    #[serde(default = "default_follow_optimal_angle")]
    pub follow_optimal_angle: f64,

    /// 跟随方位角容忍宽度σ（弧度，默认 25°≈0.436 rad）
    #[serde(default = "default_follow_angle_width")]
    pub follow_angle_width: f64,

    /// 跟随度归一化上限缩放
    #[serde(default = "default_follow_max_level")]
    pub follow_max_level: f64,

    /// 跟随度稀疏计算间隔（秒）
    #[serde(default = "default_follow_update_interval")]
    pub follow_update_interval: f64,

    /// 视觉半径（眼睛能看到的最大距离）
    pub vision_range: f64,
    /// 接触判定距离
    pub contact_range: f64,

    /// 初始连接数最小值
    pub initial_connections_min: usize,
    /// 初始连接数最大值
    pub initial_connections_max: usize,
    /// 种族相似度阈值（高于此值视为同一种族）
    pub species_similarity_threshold: f64,
    /// 体温逸散系数
    pub heat_dissipation_coefficient: f64,
    /// 喂食效率（已废弃，保留兼容旧config）
    #[serde(default)]
    pub feed_efficiency: f64,
    /// 战力公式：速度权重（越快越强）
    pub combat_speed_weight: f64,
    /// 战力公式：同族援助权重（附近同族越多越强）
    pub combat_ally_weight: f64,

    /// 优势种检测：种群最老成员最低年龄
    pub dominant_min_age: f64,
    /// 初始世界缩放比例
    pub initial_scale: f32,

    // === 器官冷却 ===
    /// 眼睛冷却时间（秒）— 已废弃，保留兼容旧config
    pub eye_cooldown: f64,
    /// 眼睛扫描速度（度/秒）
    #[serde(default = "default_eye_scan_speed")]
    pub eye_scan_speed: f64,
    /// 嘴巴冷却时间（秒）
    pub mouth_cooldown: f64,
    /// 咬合能量转移率
    pub bite_transfer_rate: f64,

    // === 散热 ===
    /// 能量分母（nearby_energy 归一化）
    pub energy_denominator: f64,
    /// 散热下限（指数衰减底板，0~1）
    pub heat_floor: f64,

    // === 正弦周期 ===
    /// 火山间隔正弦周期（秒）
    pub volcano_interval_cycle: f64,
    /// 火山间隔振幅比（0~0.9）
    pub volcano_interval_amplitude: f64,
    /// 火山能量正弦周期（秒）
    pub volcano_energy_cycle: f64,
    /// 火山能量振幅比（0~0.9）
    pub volcano_energy_amplitude: f64,
    /// 陨石间隔正弦周期 — 已废弃
    #[serde(default)]
    pub meteorite_interval_cycle: f64,
    /// 陨石间隔振幅比 — 已废弃
    #[serde(default)]
    pub meteorite_interval_amplitude: f64,
    /// 陨石能量正弦周期 — 已废弃
    #[serde(default)]
    pub meteorite_energy_cycle: f64,
    /// 陨石能量振幅比 — 已废弃
    #[serde(default)]
    pub meteorite_energy_amplitude: f64,

    // === 落地杀伤 ===
    /// 火山落地杀伤半径
    pub volcano_kill_radius: f64,
    /// 陨石落地杀伤半径 — 已废弃
    #[serde(default)]
    pub meteorite_kill_radius: f64,

    // === 熔岩流 ===
    /// 每次火山喷发附带的熔岩流粒子数
    #[serde(default = "default_lava_count")]
    pub lava_count: usize,
    /// 熔岩粒子死亡时分裂第二个子粒子的概率
    #[serde(default = "default_lava_spread_probability")]
    pub lava_spread_probability: f64,
    /// 最大链式代数
    #[serde(default = "default_lava_max_chain_depth")]
    pub lava_max_chain_depth: u8,
    /// 每个粒子贡献的等效液面高度
    #[serde(default = "default_lava_level_per_particle")]
    pub lava_level_per_particle: f64,
    /// 熔岩粒子周期性杀伤基础间隔（秒，火山口处）
    #[serde(default = "default_lava_kill_base_interval")]
    pub lava_kill_base_interval: f64,
    /// 距离对杀伤周期的放大系数
    #[serde(default = "default_lava_kill_distance_scale")]
    pub lava_kill_distance_scale: f64,
    /// 熔岩粒子独立衰减率（/秒）
    #[serde(default = "default_lava_decay_rate")]
    pub lava_decay_rate: f64,
    /// 溢流最大区块跳跃数（超过则丢弃粒子）
    #[serde(default = "default_lava_max_overflow_depth")]
    pub lava_max_overflow_depth: usize,
    /// 落地最低伤害比例（已废弃，保留兼容旧config）
    #[serde(default)]
    pub min_landing_damage_ratio: f64,
    /// 落地杀伤系数（新公式：damage = c × (1 - exp(-p × multiplier / c))）
    #[serde(default = "default_landing_damage_multiplier")]
    pub landing_damage_multiplier: f64,
    /// 粒子消失能量阈值（低于此值粒子死亡）
    #[serde(default = "default_particle_min_energy")]
    pub particle_min_energy: f64,

    // === 痕迹点 ===
    /// 痕迹点能量衰减率（/秒）
    pub trail_decay_rate: f64,
    /// 痕迹抑制半径（px，范围内有痕迹则不产生）
    pub trail_suppress_radius: f64,
    /// 痕迹生成间隔（秒，每个生物独立计时）
    pub trail_emit_interval: f64,

    // === 繁殖 ===
    /// 繁殖冷却时间（已废弃，完全由神经网络控制，保留兼容旧config）
    #[serde(default)]
    pub reproduce_cooldown: f64,

    /// 繁殖能量年龄代价系数（对称指数曲线 k）
    /// 父辈消耗 = energy × ratio × exp(+k·p)，后辈得到 = energy × ratio × exp(-k·p)
    /// 其中 p = age / maturation_time（发育度）
    /// p=0 时双方=1（无损耗），p>0 时损耗 = 2·sinh(k·p)·ratio·energy 指数上升
    /// 与 structure_factor 形成姊妹曲线（同一时间坐标 p）
    /// 设为 0.0 退化为当前 100% 转移行为
    #[serde(default = "default_reproduction_age_cost_rate")]
    pub reproduction_age_cost_rate: f64,

    // === 算力能量 ===
    /// 算力转能量系数（0.1s → 能量，内部除以1亿，0 = 禁用）
    #[serde(default)]
    pub compute_energy_factor: f64,

    // === SNN ===
    /// 每帧 SNN tick 数（同步模式）
    #[serde(default = "default_snn_ticks_per_frame")]
    pub snn_ticks_per_frame: usize,
    /// 神经后端: auto | cpu | gpu | legacy
    #[serde(default = "default_neural_backend")]
    pub neural_backend: String,
    /// 异步模式 tick 频率 (ticks/s)
    #[serde(default = "default_neural_tick_rate")]
    pub neural_tick_rate: f64,

    /// 变异率（全局常量，不再是基因组内可演化基因）
    /// 同时作为 base/block 两类变异的触发概率
    #[serde(default = "default_mutation_rate")]
    pub mutation_rate: f64,

    /// 无性繁殖权重变异率缩放系数（仅作用于权重微调那一类；其余 7 类创新变异在无性繁殖中完全禁用）
    /// 默认 0.4 → 无性后代权重变异概率 = mutation_rate × 0.4，约等于近似克隆
    /// 设为 1.0 退化为无差异化，设为 0.0 则完全克隆
    #[serde(default = "default_asexual_mutation_scale")]
    pub asexual_mutation_scale: f64,

    // === 地形 ===
    /// 地形坡度移动消耗系数（上坡额外开销倍率，0=禁用）
    #[serde(default = "default_terrain_slope_cost")]
    pub terrain_slope_cost: f64,

    // === 奖励通道开关 ===
    /// 能量吸收奖励（核心生存信号，默认开启）
    #[serde(default = "default_true")]
    pub reward_energy_enabled: bool,
    /// 痕迹吸收奖励
    #[serde(default)]
    pub reward_trail_enabled: bool,
    /// 集体行为奖励（跟随+群居散热）
    #[serde(default)]
    pub reward_group_enabled: bool,

    // === 反孤立代价（统一系数）===
    /// 独立行为代价倍率：同时控制
    /// - 无性繁殖年龄代价 = reproduce_age_cost × solitude_penalty（vs 有性 ×1）
    /// - 孤独生命的年龄加速倍率（vision_range 内无邻居 → age 增速 ×solitude_penalty）
    /// 默认 3.0，设为 1.0 则两个机制都退化为无惩罚
    #[serde(default = "default_solitude_penalty")]
    pub solitude_penalty: f64,

    // === 力导图 ===
    /// 力导图水平锚定强度（左右半球横向拉扯力）
    #[serde(default = "default_force_graph_h_anchor")]
    pub force_graph_h_anchor: f64,
    /// 力导图垂直锚定强度（感官/运动纵向拉扯力）
    #[serde(default = "default_force_graph_v_anchor")]
    pub force_graph_v_anchor: f64,
    /// 力导图单 iter 速度上限（cap）：太小会让强锚定力被截断，导致 v_anchor 增大后仍打不过边吸引
    #[serde(default = "default_force_graph_max_vel")]
    pub force_graph_max_vel: f64,
    /// 力导图整体密度缩放（1.0=默认，<1 更紧凑，>1 更松散）
    #[serde(default = "default_force_graph_density")]
    pub force_graph_density: f64,
    /// 力导图 block 内质心凝聚力（0=关闭，越大同 block 节点越聚拢）
    #[serde(default = "default_force_graph_cohesion_k")]
    pub force_graph_cohesion_k: f64,
    /// 力导图 Input/Output 间距（世界坐标）。同时作用于两个维度：
    ///   - 横向：同一行内相邻 IO 节点的等距间隔
    ///   - 纵向：IO 行与最近 Block 节点的安全距离（每帧动态贴近）
    #[serde(default = "default_force_graph_io_spacing")]
    pub force_graph_io_spacing: f64,
}

fn default_true() -> bool {
    true
}

fn default_terrain_slope_cost() -> f64 {
    2.0
}

fn default_solitude_penalty() -> f64 {
    3.0
}

fn default_force_graph_h_anchor() -> f64 {
    0.08
}

fn default_force_graph_v_anchor() -> f64 {
    0.08
}

fn default_force_graph_max_vel() -> f64 {
    240.0
}

fn default_force_graph_io_spacing() -> f64 {
    150.0
}

fn default_force_graph_density() -> f64 {
    1.0
}

fn default_force_graph_cohesion_k() -> f64 {
    0.003
}

fn default_max_creatures() -> usize {
    700
}
fn default_max_speed() -> f64 {
    20.0
}
fn default_landing_damage_multiplier() -> f64 {
    1.0
}
fn default_base_metabolism() -> f64 {
    0.025
}
fn default_metabolism_exponent() -> f64 {
    1.5
}
fn default_eye_scan_speed() -> f64 {
    280.0
}
fn default_follow_cost_discount() -> f64 {
    0.3
}
fn default_follow_optimal_angle() -> f64 {
    30.0_f64.to_radians() // ±30°
}
fn default_follow_angle_width() -> f64 {
    25.0_f64.to_radians() // σ=25°
}
fn default_follow_max_level() -> f64 {
    1.0
}
fn default_follow_update_interval() -> f64 {
    0.25
}

fn default_lava_count() -> usize {
    3
}
fn default_lava_spread_probability() -> f64 {
    0.5
}
fn default_lava_max_chain_depth() -> u8 {
    5
}
fn default_lava_level_per_particle() -> f64 {
    0.5
}
fn default_lava_kill_base_interval() -> f64 {
    1.0
}
fn default_lava_kill_distance_scale() -> f64 {
    29.0
}
fn default_lava_decay_rate() -> f64 {
    0.005
}
fn default_lava_max_overflow_depth() -> usize {
    10
}

fn default_snn_ticks_per_frame() -> usize {
    10
}
fn default_neural_backend() -> String {
    "auto".to_string()
}
fn default_neural_tick_rate() -> f64 {
    300.0
}
fn default_mutation_rate() -> f64 {
    0.15
}
fn default_asexual_mutation_scale() -> f64 {
    0.4
}
fn default_reproduction_age_cost_rate() -> f64 {
    0.4
}
fn default_particle_min_energy() -> f64 {
    100.0
}

impl Config {
    /// 从 config.toml 加载，失败则用默认值
    pub fn load() -> Self {
        match std::fs::read_to_string(CONFIG_PATH) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("配置解析失败，使用默认值: {}", e);
                    let config = Self::default();
                    config.save();
                    config
                }
            },
            Err(_) => {
                let config = Self::default();
                config.save();
                config
            }
        }
    }

    /// 保存到 config.toml（保留注释）
    pub fn save(&self) {
        // 截断 f32 精度，避免序列化出十几位小数
        let mut config = self.clone();
        config.initial_scale = (config.initial_scale * 1000.0).round() / 1000.0;

        let existing = std::fs::read_to_string(CONFIG_PATH).unwrap_or_default();
        let mut doc = existing
            .parse::<toml_edit::DocumentMut>()
            .unwrap_or_else(|_| toml_edit::DocumentMut::new());

        // 序列化当前值，逐字段更新到已有文档（保留注释和排版）
        if let Ok(new_str) = toml::to_string(&config) {
            if let Ok(new_doc) = new_str.parse::<toml_edit::DocumentMut>() {
                for (key, item) in new_doc.iter() {
                    doc[key] = item.clone();
                }
            }
        }

        let _ = std::fs::write(CONFIG_PATH, doc.to_string());
    }

    /// 正弦周期调制值：value = average × (1 + amplitude × sin(2π × time / period))
    fn sinusoidal(&self, average: f64, amplitude: f64, cycle: f64, time: f64) -> f64 {
        if cycle <= 0.0 || amplitude <= 0.0 {
            return average;
        }
        average * (1.0 + amplitude * (std::f64::consts::TAU * time / cycle).sin())
    }

    /// 当前火山喷发间隔（正弦调制后）
    pub fn current_volcano_interval(&self, time: f64) -> f64 {
        self.sinusoidal(
            self.volcano_interval,
            self.volcano_interval_amplitude,
            self.volcano_interval_cycle,
            time,
        )
    }

    /// 当前火山粒子能量（正弦调制后）
    pub fn current_volcano_energy(&self, time: f64) -> f64 {
        self.sinusoidal(
            self.volcano_particle_energy,
            self.volcano_energy_amplitude,
            self.volcano_energy_cycle,
            time,
        )
    }

    /// 战力计算公式（energy + speed + ally）
    pub fn combat_power(&self, energy: f64, speed_norm: f64, ally_total_energy: f64) -> f64 {
        let energy_factor = energy / 100.0;
        let speed_factor = 1.0 + self.combat_speed_weight * speed_norm;
        let ally_norm = (ally_total_energy / 300.0).min(1.0);
        let ally_factor = 1.0 + self.combat_ally_weight * ally_norm;
        energy_factor * speed_factor * ally_factor
    }
}

impl Default for Config {
    fn default() -> Self {
        // 唯一配置源：config.toml（编译时嵌入）
        toml::from_str(include_str!("../config.toml")).expect("config.toml 格式错误")
    }
}
