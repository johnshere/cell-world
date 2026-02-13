/// 世界配置
pub struct Config {
    /// 世界宽度
    pub world_width: f64,
    /// 世界高度
    pub world_height: f64,

    /// 初始生物数量
    pub initial_creatures: usize,
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

    /// 基础代谢率（每秒消耗）
    pub base_metabolism: f64,
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

    /// 变异率
    pub mutation_rate: f64,
    /// 权重变异范围
    pub weight_mutation_range: f64,
    /// 结构变异率
    pub structure_mutation_rate: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            world_width: 800.0,
            world_height: 600.0,

            initial_creatures: 100,
            initial_energy: 100.0,

            energy_spawn_interval: 0.2,
            energy_spawn_count: 5,
            energy_particle_value: 20.0,
            energy_particle_lifetime: 25.0,

            base_metabolism: 0.1,
            move_cost: 0.2,
            reproduce_threshold: 35.0,
            reproduce_energy_ratio: 0.45,

            sense_range: 50.0,
            contact_range: 8.0,

            mutation_rate: 0.1,
            weight_mutation_range: 0.2,
            structure_mutation_rate: 0.05,
        }
    }
}
