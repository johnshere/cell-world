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
    /// 百分比代谢率（每秒消耗当前能量的百分比）
    pub percent_metabolism: f64,
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
    /// 初始世界缩放比例
    pub initial_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_speed: 1.0,  // 初始倍速，加速演化

            min_creatures: 40,  // 更大种群，增加有用变异概率
            max_creatures: 150, // 限制最大数量以保证性能（O(n²)聚类）

            initial_energy: 50.0,  // 初始能量适中，略高于繁殖阈值

            energy_spawn_interval: 0.8,  // 适中的生成频率，维持能量供给
            energy_spawn_count: 1,  // 每次只生成1个（稀缺但稳定）
            energy_particle_value: 50.0,  // 单个粒子价值适中
            energy_particle_lifetime: 40.0,  // 能量存在更久

            energy_wave_enabled: true,  // 启用能量波动
            energy_wave_amplitude: 0.5,  // 波动幅度50%（强度范围 0.5~1.5）
            // 使用质数周期（秒），产生长周期/弱周期效果
            // 总周期 = LCM(31, 47, 73, 113) ≈ 12,005,773 秒（超过138天）
            energy_wave_periods: [31.0, 47.0, 73.0, 113.0],

            base_metabolism: 0.15,  // 高基础消耗，强迫移动觅食
            percent_metabolism: 0.008,  // 高百分比代谢，能量多消耗快，鼓励繁殖
            move_cost: 0.001,  // 移动消耗极低，移动比待机划算
            reproduce_threshold: 38.0,  // 繁殖阈值适中，让成功觅食者能繁殖
            reproduce_energy_ratio: 0.35,  // 子代获得35%能量，确保子代能存活

            scan_free_radius: 50.0,      // 免费扫描半径
            scan_max_radius: 200.0,       // 最大扫描半径
            scan_max_angular_velocity: 180.0,  // 最大角速度（度/秒）
            scan_cost: 0.00001,           // 扫描单位成本
            contact_range: 8.0,

            mutation_rate: 0.15,  // 提高变异率，加速结构探索
            initial_connections_min: 6,  // 更多初始连接，增加有用组合概率
            initial_connections_max: 12,
            species_similarity_threshold: 0.9,  // 基因相似度 >= 视为同一种族
            initial_scale: 0.6,  // 初始世界缩放比例
        }
    }
}
