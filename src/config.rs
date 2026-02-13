/// 世界配置
pub struct Config {
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_energy: 60.0,

            energy_spawn_interval: 0.3,
            energy_spawn_count: 2,
            energy_particle_value: 35.0,
            energy_particle_lifetime: 25.0,

            base_metabolism: 0.1,
            percent_metabolism: 0.005,  // 每秒消耗0.5%的能量
            move_cost: 0.2,
            reproduce_threshold: 35.0,
            reproduce_energy_ratio: 0.45,

            sense_range: 50.0,
            contact_range: 8.0,

            mutation_rate: 0.05,
            initial_connections_min: 3,
            initial_connections_max: 6,
        }
    }
}
