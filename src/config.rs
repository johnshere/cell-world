use serde::{Serialize, Deserialize};

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
    /// 体温逸散系数（系数 × 冷却时长 × 周长 × 环境因子；小体积生物受影响更大，消耗/能量 ∝ energy^(-2/3)）
    pub heat_dissipation_coefficient: f64,
    /// 喂食效率（固定比例，无体型限制）
    pub feed_efficiency: f64,
    /// 战力公式：体温权重（越暖越强）
    pub combat_temp_weight: f64,
    /// 战力公式：速度权重（越快越强）
    pub combat_speed_weight: f64,
    /// 战力公式：同族援助权重（附近同族越多越强）
    pub combat_ally_weight: f64,
    /// 战力公式：同族援助范围
    pub combat_ally_range: f64,

    /// 优势种检测：种群最老成员最低年龄
    pub dominant_min_age: f64,
    /// 初始世界缩放比例
    pub initial_scale: f32,

    // === 器官冷却 ===
    /// 鼻子冷却时间（秒）
    pub nose_cooldown: f64,
    /// 眼睛冷却时间（秒）
    pub eye_cooldown: f64,
    /// 嘴巴冷却时间（秒）
    pub mouth_cooldown: f64,
    /// 鼻子单次扫描成本（每实体 × 此值，与环境密度正相关）
    pub nose_scan_cost: f64,
    /// 眼睛单次扫描成本（每实体 × 此值，与环境密度正相关）
    pub eye_scan_cost: f64,
    /// 咬合能量成本（|mouth| × mouth_power × 此值）
    pub bite_cost: f64,
    /// 咬合能量转移率
    pub bite_transfer_rate: f64,

    // === 环境温度 ===
    /// 火山热辐射范围
    pub volcano_heat_range: f64,
    /// 远离火山温度流失加成系数（+100%）
    pub cold_loss_factor: f64,

    /// 热容量系数（冷却上限 = 体型半径 × 此值，体型大热惯性高更抗寒）
    pub thermal_mass_factor: f64,

    // === 落地杀伤 ===
    /// 火山落地杀伤半径
    pub volcano_kill_radius: f64,
    /// 陨石落地杀伤半径
    pub meteorite_kill_radius: f64,
    /// 火山粒子下落时长（秒，分批落下总跨度）
    pub volcano_fall_duration: f64,
    /// 陨石粒子下落时长（秒）
    pub meteorite_fall_duration: f64,

    // === 痕迹点 ===
    /// 痕迹点能量衰减率（/秒）
    pub trail_decay_rate: f64,
    /// 痕迹抑制半径（px，范围内有其他生物痕迹则不产生）
    pub trail_suppress_radius: f64,
    /// 痕迹生成间隔（秒，每个生物独立计时）
    pub trail_emit_interval: f64,

    // === 鼻子 ===
    /// 鼻子半角（弧度）
    pub nose_half_angle: f64,
}

impl Config {
    /// 从 config.toml 加载，失败则用默认值
    pub fn load() -> Self {
        match std::fs::read_to_string(CONFIG_PATH) {
            Ok(content) => {
                match toml::from_str(&content) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("配置解析失败，使用默认值: {}", e);
                        let config = Self::default();
                        config.save();
                        config
                    }
                }
            }
            Err(_) => {
                let config = Self::default();
                config.save();
                config
            }
        }
    }

    /// 保存到 config.toml（保留注释）
    pub fn save(&self) {
        let existing = std::fs::read_to_string(CONFIG_PATH).unwrap_or_default();
        let mut doc = existing.parse::<toml_edit::DocumentMut>().unwrap_or_else(|_| {
            toml_edit::DocumentMut::new()
        });

        // 序列化当前值，逐字段更新到已有文档（保留注释和排版）
        if let Ok(new_str) = toml::to_string(self) {
            if let Ok(new_doc) = new_str.parse::<toml_edit::DocumentMut>() {
                for (key, item) in new_doc.iter() {
                    doc[key] = item.clone();
                }
            }
        }

        let _ = std::fs::write(CONFIG_PATH, doc.to_string());
    }

    /// 环境温度：距火山越近越高 (0~1)，三次方衰减使火山口附近更热、远处急剧下降
    pub fn ambient_temperature(&self, x: f64, y: f64) -> f64 {
        let dist = ((x - self.volcano_x).powi(2) + (y - self.volcano_y).powi(2)).sqrt();
        let linear = (1.0 - dist / self.volcano_heat_range).clamp(0.0, 1.0);
        linear * linear * linear
    }

    /// 战力计算公式
    pub fn combat_power(&self, energy: f64, warmth: f64, speed_norm: f64, ally_total_energy: f64) -> f64 {
        let energy_factor = energy / 100.0;
        let temp_factor = 1.0 + self.combat_temp_weight * warmth;
        let speed_factor = 1.0 + self.combat_speed_weight * speed_norm;
        let ally_norm = (ally_total_energy / 500.0).min(1.0);
        let ally_factor = 1.0 + self.combat_ally_weight * ally_norm;
        energy_factor * temp_factor * speed_factor * ally_factor
    }
}

impl Default for Config {
    fn default() -> Self {
        // 唯一配置源：config.toml（编译时嵌入）
        toml::from_str(include_str!("../config.toml"))
            .expect("config.toml 格式错误")
    }
}
