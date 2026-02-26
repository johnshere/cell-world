/// 世界配置
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
    /// 粒子能量衰减率（每秒 energy *= (1 - rate)）
    pub particle_decay_rate: f64,

    /// 基础代谢率（每秒固定消耗）
    pub base_metabolism: f64,
    /// 年龄代谢倍率（age × 此值 = 额外倍率，年龄越大消耗越高）
    pub age_metabolism_factor: f64,
    /// 移动消耗（每单位距离）
    pub move_cost: f64,
    /// 繁殖所需最低能量
    pub reproduce_threshold: f64,
    /// 子代获得的能量比例
    pub reproduce_energy_ratio: f64,

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
    /// 优势种检测：种群最老成员最低年龄
    pub dominant_min_age: f64,
    /// 初始世界缩放比例
    pub initial_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_speed: 1.0,  // 初始倍速，加速演化

            min_creatures: 15,  // 低于此值自动补充

            initial_energy: 50.0,  // 初始能量更高，确保能繁殖一次

            volcano_x: 0.0,
            volcano_y: 0.0,
            volcano_interval: 30.0,         // 喷发间隔（秒）
            volcano_radius: 450.0,          // 喷射半径（内密外疏）
            volcano_count: 60,              // 每次粒子数
            volcano_particle_energy: 35.0,  // 单粒子能量
            meteorite_interval: 12.0,       // 陨石间隔（秒）
            meteorite_count: 15,            // 每颗粒子数
            meteorite_length: 150.0,        // 散布线段长度
            meteorite_particle_energy: 35.0,
            particle_decay_rate: 0.005,     // 每秒 energy *= (1 - rate)

            base_metabolism: 0.07,  // 基础消耗
            age_metabolism_factor: 0.04,  // 年龄倍率：age=33s时消耗×2.0，age=100s时消耗×4.0
            move_cost: 0.001,  // 移动消耗极低，移动比待机划算
            reproduce_threshold: 50.0,  // 繁殖阈值
            reproduce_energy_ratio: 0.3,  // 子代获得能量

            vision_range: 150.0,  // 视觉半径
            contact_range: 15.0,  // 接触判定距离

            mutation_rate: 0.15,  // 提高变异率，加速结构探索
            initial_connections_min: 6,  // 更多初始连接，增加有用组合概率
            initial_connections_max: 12,
            species_similarity_threshold: 0.9,  // 基因相似度 >= 视为同一种族
            dominant_min_age: 500.0,  // 优势种检测：最老成员需达到年龄（秒）
            initial_scale: 0.6,  // 初始世界缩放比例
        }
    }
}
