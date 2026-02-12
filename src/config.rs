/// 世界配置
pub struct Config {
    // 世界尺寸
    pub world_width: f64,
    pub world_height: f64,

    // 初始化
    pub initial_creatures: usize,
    pub initial_energy: f64,

    // 能量粒子
    pub energy_spawn_interval: f64,    // 秒
    pub energy_spawn_count: usize,
    pub energy_particle_value: f64,
    pub energy_particle_lifetime: f64, // 秒

    // 生物能量
    pub base_metabolism: f64,          // 每秒消耗
    pub move_cost: f64,                // 每单位距离消耗
    pub reproduce_threshold: f64,      // 繁殖所需最低能量
    pub reproduce_energy_ratio: f64,   // 子代获得的能量比例

    // 感知
    pub sense_range: f64,              // 感知半径
    pub contact_range: f64,            // 接触判定距离

    // 进化
    pub mutation_rate: f64,
    pub weight_mutation_range: f64,
    pub structure_mutation_rate: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            world_width: 800.0,
            world_height: 600.0,

            initial_creatures: 50,
            initial_energy: 100.0,

            energy_spawn_interval: 0.1,
            energy_spawn_count: 30,
            energy_particle_value: 20.0,
            energy_particle_lifetime: 25.0,

            base_metabolism: 0.1,
            move_cost: 0.5,
            reproduce_threshold: 80.0,
            reproduce_energy_ratio: 0.5,

            sense_range: 50.0,
            contact_range: 5.0,

            mutation_rate: 0.1,
            weight_mutation_range: 0.2,
            structure_mutation_rate: 0.05,
        }
    }
}
