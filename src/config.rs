/// 世界配置
pub struct Config {
    /// 初始模拟速度
    pub initial_speed: f64,

    /// 最小生物数量（低于此值自动补充）
    pub min_creatures: usize,
    /// 最大生物数量（超过此值停止繁殖）
    pub max_creatures: usize,

    /// 初始能量
    pub initial_energy: f64,

    /// 能量粒子生成间隔（秒）
    pub energy_spawn_interval: f64,
    /// 每次生成的能量粒子数量
    pub energy_spawn_count: usize,
    /// 每个能量粒子的能量值
    pub energy_particle_value: f64,
    /// 能量粒子存活时间（秒）
    pub energy_particle_lifetime: f64,

    /// 基础代谢率（每秒固定消耗）
    pub base_metabolism: f64,
    /// 百分比代谢率（每秒消耗当前能量的百分比）
    pub percent_metabolism: f64,
    /// 移动消耗（每单位距离）
    pub move_cost: f64,
    /// 繁殖所需最低能量
    pub reproduce_threshold: f64,
    /// 子代获得的能量比例
    pub reproduce_energy_ratio: f64,

    /// 感知半径
    pub sense_range: f64,
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
    /// 初始世界缩放比例
    pub initial_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_speed: 3.5,  // 初始倍速，加速演化

            min_creatures: 40,  // 更大种群，增加有用变异概率
            max_creatures: 150, // 限制最大数量以保证性能（O(n²)聚类）

            initial_energy: 70.0,  // 更多初始能量，延长生存时间

            energy_spawn_interval: 0.15,  // 更频繁生成
            energy_spawn_count: 2,  // 每次生成更多
            energy_particle_value: 50.0,  // 每个粒子更多能量
            energy_particle_lifetime: 40.0,  // 能量存在更久

            base_metabolism: 0.05,  // 降低基础代谢
            percent_metabolism: 0.002,  // 降低百分比代谢
            move_cost: 0.1,
            reproduce_threshold: 28.0,  // 降低繁殖阈值，让更多生物能繁殖
            reproduce_energy_ratio: 0.4,  // 子代获得40%能量

            sense_range: 50.0,
            contact_range: 8.0,

            mutation_rate: 0.15,  // 提高变异率，加速结构探索
            initial_connections_min: 6,  // 更多初始连接，增加有用组合概率
            initial_connections_max: 12,
            species_similarity_threshold: 0.6,  // 基因相似度 >= 60% 视为同一种族
            initial_scale: 0.6,  // 初始世界缩放比例
        }
    }
}
