use serde::{Deserialize, Serialize};

const CONFIG_PATH: &str = "config.toml";

/// 世界配置
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// 初始模拟速度
    pub initial_speed: f64,

    /// 最小生物数量（低于此值自动补充）
    pub min_creatures: usize,

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
    /// 每次喷发粒子数
    pub volcano_count: usize,
    /// 火山粒子能量
    pub volcano_particle_energy: f64,
    /// 陨石降落间隔（秒）
    pub meteorite_interval: f64,
    /// 每颗陨石粒子数
    pub meteorite_count: usize,
    /// 陨石散布线段长度
    pub meteorite_length: f64,
    /// 陨石粒子能量
    pub meteorite_particle_energy: f64,
    /// 火山粒子衰减率（每秒）
    pub volcano_decay_rate: f64,
    /// 陨石粒子衰减率（每秒）
    pub meteorite_decay_rate: f64,

    /// 基础代谢率（每秒固定消耗）
    pub base_metabolism: f64,
    /// 年龄代谢倍率（age × 此值 = 额外倍率，年龄越大消耗越高）
    pub age_metabolism_factor: f64,
    /// 移动消耗（每单位距离）
    pub move_cost: f64,

    /// 视觉半径（眼睛能看到的最大距离）
    pub vision_range: f64,
    /// 接触判定距离
    pub contact_range: f64,

    /// 变异率（所有变异逻辑共用）
    pub mutation_rate: f64,
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
    /// 陨石间隔正弦周期（秒）
    pub meteorite_interval_cycle: f64,
    /// 陨石间隔振幅比（0~0.9）
    pub meteorite_interval_amplitude: f64,
    /// 陨石能量正弦周期（秒）
    pub meteorite_energy_cycle: f64,
    /// 陨石能量振幅比（0~0.9）
    pub meteorite_energy_amplitude: f64,

    // === 落地杀伤 ===
    /// 火山落地杀伤半径
    pub volcano_kill_radius: f64,
    /// 陨石落地杀伤半径
    pub meteorite_kill_radius: f64,
    /// 落地最低伤害比例（保证至少削掉此比例能量）
    #[serde(default = "default_min_landing_damage_ratio")]
    pub min_landing_damage_ratio: f64,

    // === 痕迹点 ===
    /// 痕迹点能量衰减率（/秒）
    pub trail_decay_rate: f64,
    /// 痕迹抑制半径（px，范围内有其他生物痕迹则不产生）
    pub trail_suppress_radius: f64,
    /// 痕迹生成间隔（秒，每个生物独立计时）
    pub trail_emit_interval: f64,

    // === 繁殖 ===
    /// 繁殖冷却时间（秒）
    pub reproduce_cooldown: f64,

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
}

fn default_min_landing_damage_ratio() -> f64 {
    0.15
}
fn default_eye_scan_speed() -> f64 {
    280.0
}

fn default_snn_ticks_per_frame() -> usize {
    10
}
fn default_neural_backend() -> String {
    "auto".to_string()
}
fn default_neural_tick_rate() -> f64 {
    600.0
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

    /// 当前陨石降落间隔（正弦调制后）
    pub fn current_meteorite_interval(&self, time: f64) -> f64 {
        self.sinusoidal(
            self.meteorite_interval,
            self.meteorite_interval_amplitude,
            self.meteorite_interval_cycle,
            time,
        )
    }

    /// 当前陨石粒子能量（正弦调制后）
    pub fn current_meteorite_energy(&self, time: f64) -> f64 {
        self.sinusoidal(
            self.meteorite_particle_energy,
            self.meteorite_energy_amplitude,
            self.meteorite_energy_cycle,
            time,
        )
    }

    /// 战力计算公式（energy + speed + ally）
    pub fn combat_power(
        &self,
        energy: f64,
        speed_norm: f64,
        ally_total_energy: f64,
    ) -> f64 {
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
