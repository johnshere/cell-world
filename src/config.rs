/// 世界配置
pub struct Config {
    /// 初始模拟速度
    pub initial_speed: f64,

    /// 最小生物数量（低于此值自动补充）
    pub min_creatures: usize,

    /// 初始能量
    pub initial_energy: f64,

    /// 能量粒子生成间隔（秒）
    pub energy_spawn_interval: f64,
    /// 每次生成的能量粒子数量（基准值，会被波动影响）
    pub energy_spawn_count: usize,
    /// 每个能量粒子的能量值（基准值，会被波动影响）
    pub energy_particle_value: f64,
    /// 能量粒子存活时间（秒）
    pub energy_particle_lifetime: f64,

    /// 能量波动开关
    pub energy_wave_enabled: bool,
    /// 能量波动幅度（0.0~1.0，建议0.3~0.6）
    /// 实际强度范围: [1-amplitude, 1+amplitude]
    pub energy_wave_amplitude: f64,
    /// 能量波动周期（秒数组，使用互质数产生弱周期效果）
    /// 多层正弦波叠加，产生看似无规律的波动
    pub energy_wave_periods: [f64; 4],

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

    /// 免费感知半径（此范围内扫描不耗能）
    pub scan_free_radius: f64,
    /// 最大感知半径
    pub scan_max_radius: f64,
    /// 最大扫描角速度（度/秒）
    pub scan_max_angular_velocity: f64,
    /// 扫描单位成本（每度耗能 = max(0, r-free)² × π/360 × cost）
    pub scan_cost: f64,
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
    /// 捕猎能量转化率（掠夺的能量 × 此值 = 实际获得）
    pub predation_efficiency: f64,
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

            energy_spawn_interval: 0.6,  // 略快生成，增加能量供给
            energy_spawn_count: 2,  // 每次生成3个，增加能量可用性
            energy_particle_value: 35.0,  // 提高粒子价值，让觅食更有效
            energy_particle_lifetime: 35.0,  // 能量存在更久

            energy_wave_enabled: true,  // 启用能量波动
            energy_wave_amplitude: 0.75,  // 波动幅度75%（强度范围 0.25~1.75）
            // 使用质数周期（秒），产生长周期/弱周期效果
            energy_wave_periods: [93.0, 141.0, 219.0, 339.0],

            base_metabolism: 0.07,  // 基础消耗
            age_metabolism_factor: 0.04,  // 年龄倍率：age=33s时消耗×2.0，age=100s时消耗×4.0
            move_cost: 0.001,  // 移动消耗极低，移动比待机划算
            reproduce_threshold: 50.0,  // 繁殖阈值
            reproduce_energy_ratio: 0.3,  // 子代获得能量

            scan_free_radius: 40.0,      // 免费扫描半径
            scan_max_radius: 200.0,       // 最大扫描半径
            scan_max_angular_velocity: 180.0,  // 最大角速度（度/秒）
            scan_cost: 0.0001,           // 扫描单位成本
            contact_range: 15.0,  // 增大接触范围，让吸收更容易

            mutation_rate: 0.15,  // 提高变异率，加速结构探索
            initial_connections_min: 6,  // 更多初始连接，增加有用组合概率
            initial_connections_max: 12,
            species_similarity_threshold: 0.9,  // 基因相似度 >= 视为同一种族
            predation_efficiency: 0.8,  // 捕猎转化率
            dominant_min_age: 500.0,  // 优势种检测：最老成员需达到年龄（秒）
            initial_scale: 0.6,  // 初始世界缩放比例
        }
    }
}
