use rand::Rng;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::time::Instant;

use crate::config::Config;
use crate::neural::Genome;
use super::{Creature, EnergyParticle, ScanResult, SpatialGrid};

/// 性能统计（单帧）
#[derive(Default, Clone)]
pub struct PerfStats {
    pub perceive_ms: f64,
    pub forward_ms: f64,
    pub actions_ms: f64,
    pub spatial_ms: f64,
    pub total_ms: f64,
    pub creature_count: usize,
}

/// 世界
pub struct World {
    pub creatures: Vec<Creature>,
    pub energy_particles: Vec<EnergyParticle>,

    // 空间索引
    creature_grid: SpatialGrid,
    energy_grid: SpatialGrid,

    // 统计
    pub time: f64,
    pub family_stats: FxHashMap<usize, usize>,
    pub extinct_families: usize,

    // 内部状态
    next_family_id: usize,
    energy_spawn_timer: f64,

    // ID 计数器
    next_creature_id: u64,
    next_energy_id: u64,

    // 视窗范围（世界坐标）
    viewport_min_x: f64,
    viewport_min_y: f64,
    viewport_max_x: f64,
    viewport_max_y: f64,

    // 性能统计
    pub perf_stats: PerfStats,

    // 相似度缓存：(min_hash, max_hash) -> similarity（使用 RefCell 允许 &self 时修改）
    similarity_cache: RefCell<FxHashMap<(u64, u64), f64>>,

    // 缓存清理计时器
    cache_cleanup_timer: f64,

    // 行为触发次数统计（10个功能）
    pub action_counts: [usize; 10],

    // 死亡年龄统计
    death_ages: Vec<f64>,           // 所有死亡生物的年龄（已排序，用于中位数）
    death_age_sum: f64,             // 死亡年龄累计和（用于平均值）
    pub death_age_stats: DeathAgeStats, // 缓存的统计结果

    // 聚类缓存（stats 和 render 共享，使用 RefCell 允许 &self 时更新）
    species_cache: RefCell<Option<SpeciesCache>>,
    species_cache_time: RefCell<f64>,

    // 扫描缓冲区复用
    scan_buffer: Vec<ScanResult>,
    creature_query_buf: Vec<usize>,
    energy_query_buf: Vec<usize>,
}

/// 聚类缓存结果
#[derive(Clone)]
struct SpeciesCache {
    /// 并查集 parent 数组（已路径压缩）
    parent: Vec<usize>,
    /// 活着的生物在 creatures 数组中的原始索引
    alive_indices: Vec<usize>,
    /// 种群数量
    species_count: usize,
    /// 前三种群
    top_species: Vec<RankedEntry>,
    /// 生物局部索引 -> 种群根索引
    creature_species_map: FxHashMap<usize, usize>,
}

impl World {
    pub fn new(config: &Config) -> Self {
        let mut world = Self {
            creatures: Vec::new(),
            energy_particles: Vec::new(),
            creature_grid: SpatialGrid::new(config.scan_max_radius),
            energy_grid: SpatialGrid::new(config.scan_max_radius),
            time: 0.0,
            family_stats: FxHashMap::default(),
            extinct_families: 0,
            next_family_id: 0,
            energy_spawn_timer: 0.0,
            next_creature_id: 0,
            next_energy_id: 0,
            // 初始视窗（会在第一帧被实际视窗覆盖）
            viewport_min_x: 0.0,
            viewport_min_y: 0.0,
            viewport_max_x: 800.0,
            viewport_max_y: 600.0,
            perf_stats: PerfStats::default(),
            similarity_cache: RefCell::new(FxHashMap::default()),
            cache_cleanup_timer: 0.0,
            action_counts: [0; 10],
            death_ages: Vec::new(),
            death_age_sum: 0.0,
            death_age_stats: DeathAgeStats::default(),
            species_cache: RefCell::new(None),
            species_cache_time: RefCell::new(-1.0),
            scan_buffer: Vec::new(),
            creature_query_buf: Vec::new(),
            energy_query_buf: Vec::new(),
        };
        // 生成初始生物
        for _ in 0..config.min_creatures {
            world.spawn_creature(config);
        }
        world
    }

    /// 获取缓存的相似度（或计算并缓存）
    fn get_similarity(&self, creature_a: &Creature, creature_b: &Creature) -> f64 {
        let hash_a = creature_a.genome_hash;
        let hash_b = creature_b.genome_hash;
        // 规范化 key：小的在前
        let key = if hash_a <= hash_b { (hash_a, hash_b) } else { (hash_b, hash_a) };

        let mut cache = self.similarity_cache.borrow_mut();
        *cache.entry(key).or_insert_with(|| {
            creature_a.genome.similarity(&creature_b.genome)
        })
    }

    /// 设置视窗范围（世界坐标）
    pub fn set_viewport(&mut self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
        self.viewport_min_x = min_x;
        self.viewport_min_y = min_y;
        self.viewport_max_x = max_x;
        self.viewport_max_y = max_y;
    }

    /// 更新世界
    pub fn update(&mut self, dt: f64, config: &Config) {
        self.time += dt;

        // 生成能量粒子
        self.spawn_energy(dt, config);

        // 自动补充生物（低于阈值时补充）
        self.replenish_creatures(config);

        // 重建空间索引（计时）
        let spatial_start = Instant::now();
        self.rebuild_spatial_index();
        self.perf_stats.spatial_ms = spatial_start.elapsed().as_secs_f64() * 1000.0;

        // 更新生物
        self.update_creatures(dt, config);

        // 更新能量粒子
        self.update_energy_particles(dt);

        // 清理死亡实体
        self.cleanup();
    }

    /// 当生物数量低于最小值时自动补充
    fn replenish_creatures(&mut self, config: &Config) {
        loop {
            let alive_count = self.creatures.iter().filter(|c| c.alive).count();
            if alive_count >= config.min_creatures {
                break;
            }
            self.spawn_creature(config);
        }
    }

    /// 在视窗范围内生成一个新生物
    pub fn spawn_creature(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(self.viewport_min_x..self.viewport_max_x);
        let y = rng.gen_range(self.viewport_min_y..self.viewport_max_y);
        let energy = config.initial_energy * rng.gen_range(0.8..1.2);
        let family_id = self.next_family_id;
        self.next_family_id += 1;

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let creature = Creature::random(
            creature_id,
            x,
            y,
            energy,
            family_id,
            config.initial_connections_min,
            config.initial_connections_max,
        );
        self.creatures.push(creature);
        *self.family_stats.entry(family_id).or_insert(0) += 1;
    }

    /// 从模板生成生物
    pub fn spawn_from_template(&mut self, config: &Config, genome: &crate::neural::Genome, initial_energy: f64) {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(self.viewport_min_x..self.viewport_max_x);
        let y = rng.gen_range(self.viewport_min_y..self.viewport_max_y);
        let energy = initial_energy.max(config.initial_energy * 0.5);
        let family_id = self.next_family_id;
        self.next_family_id += 1;

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let creature = Creature::new(creature_id, x, y, energy, genome.clone(), family_id, 0);
        self.creatures.push(creature);
        *self.family_stats.entry(family_id).or_insert(0) += 1;
    }

    /// 杀死指定生物
    pub fn kill_creature(&mut self, id: u64) {
        if let Some(creature) = self.creatures.iter_mut().find(|c| c.id == id) {
            creature.alive = false;
        }
    }

    /// 计算当前能量强度（基于多层正弦波叠加）
    /// 返回值范围: [1-amplitude, 1+amplitude]，当 wave_enabled=false 时返回 1.0
    pub fn calculate_energy_intensity(&self, config: &Config) -> f64 {
        if !config.energy_wave_enabled {
            return 1.0;
        }

        let periods = &config.energy_wave_periods;
        let amplitude = config.energy_wave_amplitude;

        // 多层正弦波叠加，权重均匀分布使各周期贡献相近
        let weights = [0.3, 0.3, 0.25, 0.15];
        let mut wave_sum = 0.0;

        for (i, &period) in periods.iter().enumerate() {
            if period > 0.0 {
                let phase = self.time * 2.0 * std::f64::consts::PI / period;
                wave_sum += phase.sin() * weights[i];
            }
        }

        // wave_sum 范围约 [-1.0, 1.0]，乘以 amplitude 后加到基准值 1.0
        (1.0 + wave_sum * amplitude).max(0.1)
    }

    /// 生成能量粒子（只在视窗范围内生成，受波动影响）
    fn spawn_energy(&mut self, dt: f64, config: &Config) {
        self.energy_spawn_timer += dt;

        if self.energy_spawn_timer >= config.energy_spawn_interval {
            self.energy_spawn_timer = 0.0;

            // 计算当前能量强度
            let intensity = self.calculate_energy_intensity(config);

            // 根据强度调整生成数量（概率性）
            let base_count = config.energy_spawn_count as f64 * intensity;
            let spawn_count = base_count.floor() as usize;
            let fractional = base_count - base_count.floor();

            let mut rng = rand::thread_rng();

            // 额外的概率性生成（处理小数部分）
            let extra = if rng.gen::<f64>() < fractional { 1 } else { 0 };
            let total_count = spawn_count + extra;

            // 能量值也受强度影响（波动更明显）
            let energy_value = config.energy_particle_value * intensity;
            // 粒子存活时间也受强度影响
            let lifetime = config.energy_particle_lifetime * intensity;

            for _ in 0..total_count {
                // 在当前视窗范围内生成能量粒子
                let x = rng.gen_range(self.viewport_min_x..self.viewport_max_x);
                let y = rng.gen_range(self.viewport_min_y..self.viewport_max_y);
                let energy_id = self.next_energy_id;
                self.next_energy_id += 1;
                let particle = EnergyParticle::new(
                    energy_id,
                    x,
                    y,
                    energy_value,
                    lifetime,
                );
                self.energy_particles.push(particle);
            }
        }
    }

    /// 重建空间索引
    fn rebuild_spatial_index(&mut self) {
        self.creature_grid.clear();
        self.energy_grid.clear();

        for (i, c) in self.creatures.iter().enumerate() {
            if c.alive {
                self.creature_grid.insert(i, c.x, c.y);
            }
        }

        for (i, e) in self.energy_particles.iter().enumerate() {
            if e.alive {
                self.energy_grid.insert(i, e.x, e.y);
            }
        }
    }

    /// 更新生物
    fn update_creatures(&mut self, dt: f64, config: &Config) {
        let creature_count = self.creatures.len();
        let mut alive_count = 0;
        let mut perceive_time = 0.0;
        let mut forward_time = 0.0;
        let mut actions_time = 0.0;

        let total_start = Instant::now();

        for i in 0..creature_count {
            if !self.creatures[i].alive {
                continue;
            }
            alive_count += 1;

            // 基础代谢 × 年龄倍率（年龄越大消耗越高）
            let age_multiplier = 1.0 + self.creatures[i].age * config.age_metabolism_factor;
            let metabolism_cost = config.base_metabolism * age_multiplier * dt;
            self.creatures[i].energy -= metabolism_cost;
            self.creatures[i].age += dt;

            // 能量耗尽则死亡
            if self.creatures[i].energy <= 0.0 {
                self.creatures[i].alive = false;
                continue;
            }

            // 更新雷达扫描
            let t0 = Instant::now();
            self.update_scan(i, dt, config);
            perceive_time += t0.elapsed().as_secs_f64() * 1000.0;

            // 神经网络决策（使用缓存的感知结果）
            let t1 = Instant::now();
            let perception = self.creatures[i].perception_cache;
            let outputs = self.creatures[i].brain.forward(&perception);
            forward_time += t1.elapsed().as_secs_f64() * 1000.0;

            // 执行动作
            let t2 = Instant::now();
            self.execute_actions(i, &outputs, dt, config);
            actions_time += t2.elapsed().as_secs_f64() * 1000.0;
        }

        let total_time = total_start.elapsed().as_secs_f64() * 1000.0;

        // 更新性能统计
        self.perf_stats.perceive_ms = perceive_time;
        self.perf_stats.forward_ms = forward_time;
        self.perf_stats.actions_ms = actions_time;
        self.perf_stats.total_ms = total_time;
        self.perf_stats.creature_count = alive_count;
    }

    /// 更新雷达扫描（每帧一次空间查询，替代逐度查询）
    fn update_scan(&mut self, creature_idx: usize, dt: f64, config: &Config) {
        let old_angle = self.creatures[creature_idx].scan_angle;
        let angular_velocity = self.creatures[creature_idx].scan_angular_velocity;
        let scan_radius = self.creatures[creature_idx].scan_radius;
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;

        // 更新扫描角度
        let mut new_angle = old_angle + angular_velocity * dt;

        // 计算跨越的整度数
        let old_degree = old_angle.floor() as i32;
        let mut new_degree = new_angle.floor() as i32;

        // 处理角度回绕
        if new_angle >= 360.0 {
            new_angle %= 360.0;
            new_degree = 359; // 确保扫描到 359 度
        }

        self.creatures[creature_idx].scan_angle = new_angle;

        let degrees_crossed = new_degree - old_degree;
        if degrees_crossed <= 0 {
            return;
        }

        // 扫描弧范围（对应原 1 度扇形的并集）
        let arc_start = old_degree as f64 + 0.5;
        let arc_end = new_degree as f64 + 0.5;

        // 临时取出缓冲区避免借用冲突
        let mut creature_buf = std::mem::take(&mut self.creature_query_buf);
        let mut energy_buf = std::mem::take(&mut self.energy_query_buf);
        let mut scan_buf = std::mem::take(&mut self.scan_buffer);
        scan_buf.clear();

        // 一次空间查询获取所有邻近生物
        self.creature_grid.query_into(cx, cy, scan_radius, &mut creature_buf);
        for &idx in &creature_buf {
            if idx == creature_idx {
                continue;
            }
            let other = &self.creatures[idx];
            if !other.alive {
                continue;
            }

            let dx = other.x - cx;
            let dy = other.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist > 0.0 && dist <= scan_radius {
                let angle_deg = dy.atan2(dx).to_degrees();
                let angle_deg = if angle_deg < 0.0 { angle_deg + 360.0 } else { angle_deg };

                if angle_deg >= arc_start && angle_deg <= arc_end {
                    let similarity = self.get_similarity(
                        &self.creatures[creature_idx],
                        &self.creatures[idx],
                    );
                    scan_buf.push(ScanResult {
                        angle: angle_deg,
                        distance: dist,
                        similarity,
                        energy: other.energy,
                        is_energy: false,
                    });
                }
            }
        }

        // 一次空间查询获取所有邻近能量粒子
        self.energy_grid.query_into(cx, cy, scan_radius, &mut energy_buf);
        for &idx in &energy_buf {
            let particle = &self.energy_particles[idx];
            if !particle.alive {
                continue;
            }

            let dx = particle.x - cx;
            let dy = particle.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist > 0.0 && dist <= scan_radius {
                let angle_deg = dy.atan2(dx).to_degrees();
                let angle_deg = if angle_deg < 0.0 { angle_deg + 360.0 } else { angle_deg };

                if angle_deg >= arc_start && angle_deg <= arc_end {
                    scan_buf.push(ScanResult {
                        angle: angle_deg,
                        distance: dist,
                        similarity: 0.0,
                        energy: particle.energy,
                        is_energy: true,
                    });
                }
            }
        }

        // 批量推入扫描缓存
        self.creatures[creature_idx].scan_cache.append(&mut scan_buf);

        // 归还缓冲区
        self.creature_query_buf = creature_buf;
        self.energy_query_buf = energy_buf;
        self.scan_buffer = scan_buf;

        // 批量扫描耗能（替代逐度计算）
        let excess_radius = (scan_radius - config.scan_free_radius).max(0.0);
        let scan_cost_per_degree =
            excess_radius * excess_radius * std::f64::consts::PI / 360.0 * config.scan_cost;
        self.creatures[creature_idx].energy -= scan_cost_per_degree * degrees_crossed as f64;

        // 更新感知计数，每 90 度触发一次感知计算
        self.creatures[creature_idx].degrees_since_perception += degrees_crossed;
        if self.creatures[creature_idx].degrees_since_perception >= 90 {
            self.compute_perception(creature_idx, config);
            self.creatures[creature_idx].degrees_since_perception %= 90;
            self.creatures[creature_idx].scan_cache.clear();
        }
    }

    /// 从扫描缓存计算 25 维感知输入（sin/cos角度编码 + 能量粒子独立通道）
    /// [0-4]   最佳同类: sin(θ), cos(θ), 距离, 相似度, 能量
    /// [5-9]   最差同类: sin(θ), cos(θ), 距离, 相似度, 能量
    /// [10-14] 最佳异类: sin(θ), cos(θ), 距离, 相似度, 能量
    /// [15-19] 最差异类: sin(θ), cos(θ), 距离, 相似度, 能量
    /// [20-23] 最佳能量粒子: sin(θ), cos(θ), 距离, 能量
    /// [24]    自身能量
    fn compute_perception(&mut self, creature_idx: usize, config: &Config) {
        let creature = &self.creatures[creature_idx];
        let threshold = config.species_similarity_threshold;

        let mut input = [0.0; 25];

        // 分类：同类 vs 异类 vs 能量粒子（三类独立）
        let mut allies: Vec<&ScanResult> = Vec::new();
        let mut enemies: Vec<&ScanResult> = Vec::new();
        let mut energy_particles: Vec<&ScanResult> = Vec::new();

        for result in &creature.scan_cache {
            if result.is_energy {
                energy_particles.push(result);
            } else if result.similarity >= threshold {
                allies.push(result);
            } else {
                enemies.push(result);
            }
        }

        // 评分函数: 能量 / 距离
        let score = |r: &ScanResult| -> f64 {
            if r.distance > 0.0 { r.energy / r.distance } else { r.energy * 1000.0 }
        };

        // 填充生物感知槽（sin/cos角度编码，连续无跳变）
        let fill_slot = |input: &mut [f64; 25], offset: usize, result: &ScanResult, max_radius: f64| {
            let angle_rad = result.angle.to_radians();
            input[offset]     = angle_rad.sin();
            input[offset + 1] = angle_rad.cos();
            input[offset + 2] = (result.distance / max_radius).min(1.0);
            input[offset + 3] = result.similarity;
            input[offset + 4] = (result.energy / 200.0).min(1.0);
        };

        // 最佳同类（评分最高）
        if let Some(best) = allies.iter().max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            fill_slot(&mut input, 0, best, config.scan_max_radius);
        }
        // 最差同类（评分最低）
        if let Some(worst) = allies.iter().min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            fill_slot(&mut input, 5, worst, config.scan_max_radius);
        }
        // 最佳异类（评分最高）
        if let Some(best) = enemies.iter().max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            fill_slot(&mut input, 10, best, config.scan_max_radius);
        }
        // 最差异类（评分最低）
        if let Some(worst) = enemies.iter().min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            fill_slot(&mut input, 15, worst, config.scan_max_radius);
        }

        // 最佳能量粒子（独立通道，4维）
        if let Some(best) = energy_particles.iter().max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            let angle_rad = best.angle.to_radians();
            input[20] = angle_rad.sin();
            input[21] = angle_rad.cos();
            input[22] = (best.distance / config.scan_max_radius).min(1.0);
            input[23] = (best.energy / 200.0).min(1.0);
        }

        // 自身能量
        input[24] = (creature.energy / 200.0).min(1.0);

        self.creatures[creature_idx].perception_cache = input;
    }

    /// 执行动作（10个功能池）
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        let output_map = self.creatures[creature_idx].genome.output_map.clone();

        // 先收集移动参数（需要组合使用）
        let mut move_dir_sin: Option<f64> = None;
        let mut move_dir_cos: Option<f64> = None;
        let mut move_speed: Option<f64> = None;

        for (out_idx, &func_id) in output_map.iter().enumerate() {
            if out_idx >= outputs.len() {
                break;
            }
            let value = outputs[out_idx];

            match func_id {
                0 => move_dir_sin = Some(value),   // 移动方向 sin
                1 => move_dir_cos = Some(value),   // 移动方向 cos
                2 => move_speed = Some(value),     // 移动速度
                3 => {
                    if self.action_absorb(creature_idx, value, config) {
                        self.action_counts[3] += 1;
                    }
                }
                4 => {
                    if self.action_release(creature_idx, value, config) {
                        self.action_counts[4] += 1;
                    }
                }
                5 => {
                    if self.action_reproduce(creature_idx, value, config) {
                        self.action_counts[5] += 1;
                    }
                }
                6 => {
                    if self.action_predation(creature_idx, value, config) {
                        self.action_counts[6] += 1;
                    }
                }
                7 => {
                    self.action_set_scan_radius(creature_idx, value, config);
                }
                8 => {
                    self.action_set_scan_velocity(creature_idx, value, config);
                }
                9 => {
                    if self.action_nurture(creature_idx, value, config) {
                        self.action_counts[9] += 1;
                    }
                }
                _ => {}
            }
        }

        // 执行移动（需要sin/cos和速度，且速度>0.1才统计）
        if let (Some(sin_v), Some(cos_v), Some(spd)) = (move_dir_sin, move_dir_cos, move_speed) {
            if spd.abs() > 0.1 {
                self.action_move(creature_idx, sin_v, cos_v, spd, dt, config);
                self.action_counts[0] += 1;
            }
        }
    }

    // 功能 0+1+2: 移动（sin/cos方向 + 速度）
    fn action_move(&mut self, idx: usize, dir_sin: f64, dir_cos: f64, speed: f64, dt: f64, config: &Config) {
        // dir_sin, dir_cos: tanh输出(-1~1)，用atan2计算方向
        let angle_rad = dir_sin.atan2(dir_cos);
        let actual_speed = speed.abs() * 50.0;  // 最大速度 50 单位/秒

        let dx = angle_rad.cos() * actual_speed * dt;
        let dy = angle_rad.sin() * actual_speed * dt;

        self.creatures[idx].x += dx;
        self.creatures[idx].y += dy;

        // 移动消耗（与速度平方成正比，高速代价更大）
        let distance = (dx * dx + dy * dy).sqrt();
        self.creatures[idx].energy -= distance * config.move_cost * actual_speed;
    }

    // 功能 2: 吸收（自动触发，接触即吸收）
    // 返回是否成功吸收了能量
    fn action_absorb(&mut self, idx: usize, _value: f64, config: &Config) -> bool {
        let creature = &self.creatures[idx];
        let nearby = self.energy_grid.query(creature.x, creature.y, config.contact_range);

        for &particle_idx in &nearby {
            if self.energy_particles[particle_idx].alive {
                let px = self.energy_particles[particle_idx].x;
                let py = self.energy_particles[particle_idx].y;
                let dist = ((px - creature.x).powi(2) + (py - creature.y).powi(2)).sqrt();

                if dist < config.contact_range {
                    let energy = self.energy_particles[particle_idx].consume();
                    self.creatures[idx].energy += energy;
                    return true; // 吸收成功
                }
            }
        }
        false // 没有吸收到
    }

    // 功能 3: 释放
    // 返回是否成功释放了能量
    fn action_release(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        if value <= 0.0 {
            return false;
        }

        let release_amount = value * 10.0;
        if self.creatures[idx].energy > release_amount {
            self.creatures[idx].energy -= release_amount;
            let energy_id = self.next_energy_id;
            self.next_energy_id += 1;
            let particle = EnergyParticle::new(
                energy_id,
                self.creatures[idx].x,
                self.creatures[idx].y,
                release_amount,
                config.energy_particle_lifetime,
            );
            self.energy_particles.push(particle);
            return true;
        }
        false
    }

    // 功能 5: 繁殖（支持有性繁殖/交叉）
    // 如果附近有同种生物，进行交叉繁殖；否则无性繁殖
    fn action_reproduce(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        // 降低阈值使繁殖更容易触发
        if value <= 0.2 {
            return false;
        }

        if self.creatures[idx].energy < config.reproduce_threshold {
            return false;
        }

        let child_energy = self.creatures[idx].energy * config.reproduce_energy_ratio;
        self.creatures[idx].energy -= child_energy;

        let mut rng = rand::thread_rng();
        let offset_x = rng.gen_range(-10.0..10.0);
        let offset_y = rng.gen_range(-10.0..10.0);

        // 尝试找同种邻居进行交叉繁殖
        let mate_genome = self.find_mate(idx, config);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;

        let child = if let Some(mate_genome) = mate_genome {
            // 有性繁殖：交叉 + 变异
            let a_is_fitter = true; // 主动繁殖者视为更适应
            let crossover_genome = crate::neural::Genome::crossover(
                &self.creatures[idx].genome,
                &mate_genome,
                a_is_fitter,
            );
            let child_genome = crossover_genome.mutate(config.mutation_rate);
            Creature::new(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                child_genome,
                self.creatures[idx].family_id,
                self.creatures[idx].generation + 1,
            )
        } else {
            // 无性繁殖：变异
            self.creatures[idx].reproduce(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                config.mutation_rate,
            )
        };

        let family_id = child.family_id;
        self.creatures.push(child);
        *self.family_stats.entry(family_id).or_insert(0) += 1;
        true
    }

    /// 寻找附近的同种配偶（返回其基因组的克隆）
    fn find_mate(&self, idx: usize, config: &Config) -> Option<crate::neural::Genome> {
        let creature = &self.creatures[idx];
        let nearby = self.creature_grid.query(creature.x, creature.y, config.contact_range);

        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }
            let other = &self.creatures[other_idx];
            let dist = ((other.x - creature.x).powi(2) + (other.y - creature.y).powi(2)).sqrt();
            if dist < config.contact_range {
                let similarity = self.get_similarity(creature, other);
                if similarity >= config.species_similarity_threshold {
                    return Some(other.genome.clone());
                }
            }
        }
        None
    }

    // 功能 6: 捕食（掠夺邻居能量）
    // 返回是否成功捕食
    fn action_predation(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        if value <= 0.1 {
            return false;
        }

        let creature = &self.creatures[idx];
        let nearby = self.creature_grid.query(creature.x, creature.y, config.contact_range);

        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }

            let other = &self.creatures[other_idx];
            let dist = ((other.x - creature.x).powi(2) + (other.y - creature.y).powi(2)).sqrt();

            if dist < config.contact_range {
                // 掠夺：转移目标能量的 value×20%（最高20%），按转化率折损
                let transfer_ratio = value * 0.2;
                let target_energy = self.creatures[other_idx].energy;
                let transfer_amount = target_energy * transfer_ratio;
                self.creatures[other_idx].energy -= transfer_amount;
                self.creatures[idx].energy += transfer_amount * config.predation_efficiency;
                return true;
            }
        }
        false
    }

    // 功能 9: 哺育（给予邻居能量）
    // 返回是否成功哺育
    fn action_nurture(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        if value <= 0.1 {
            return false;
        }

        let creature = &self.creatures[idx];
        let nearby = self.creature_grid.query(creature.x, creature.y, config.contact_range);

        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }

            let other = &self.creatures[other_idx];
            let dist = ((other.x - creature.x).powi(2) + (other.y - creature.y).powi(2)).sqrt();

            if dist < config.contact_range {
                // 给予：转移自身能量的 value×20%
                let transfer_ratio = value * 0.2;
                let my_energy = self.creatures[idx].energy;
                let transfer_amount = my_energy * transfer_ratio;
                self.creatures[idx].energy -= transfer_amount;
                self.creatures[other_idx].energy += transfer_amount;
                return true;
            }
        }
        false
    }

    // 功能 7: 设置扫描半径
    fn action_set_scan_radius(&mut self, idx: usize, value: f64, config: &Config) {
        // value: -1~1 映射到 free_radius ~ max_radius
        let normalized = (value + 1.0) / 2.0;  // 0~1
        let radius = config.scan_free_radius + normalized * (config.scan_max_radius - config.scan_free_radius);
        self.creatures[idx].scan_radius = radius;
    }

    // 功能 8: 设置扫描角速度
    fn action_set_scan_velocity(&mut self, idx: usize, value: f64, config: &Config) {
        // value: -1~1 映射到 0 ~ max_angular_velocity
        let normalized = (value + 1.0) / 2.0;  // 0~1
        let velocity = normalized * config.scan_max_angular_velocity;
        self.creatures[idx].scan_angular_velocity = velocity;
    }

    /// 更新能量粒子
    fn update_energy_particles(&mut self, dt: f64) {
        for particle in &mut self.energy_particles {
            particle.update(dt);
        }
    }

    /// 清理死亡实体
    fn cleanup(&mut self) {
        let mut had_deaths = false;

        // 统计死亡的家族 + 收集死亡年龄
        for creature in &self.creatures {
            if !creature.alive {
                had_deaths = true;

                // 收集死亡年龄（二分插入维持排序）
                let age = creature.age;
                let pos = self.death_ages.partition_point(|&x| x < age);
                self.death_ages.insert(pos, age);
                self.death_age_sum += age;

                if let Some(count) = self.family_stats.get_mut(&creature.family_id) {
                    *count -= 1;
                    if *count == 0 {
                        self.extinct_families += 1;
                    }
                }
            }
        }

        // 有死亡时更新年龄统计
        if had_deaths {
            let n = self.death_ages.len();
            let median = if n % 2 == 0 {
                (self.death_ages[n / 2 - 1] + self.death_ages[n / 2]) / 2.0
            } else {
                self.death_ages[n / 2]
            };
            self.death_age_stats = DeathAgeStats {
                count: n,
                avg: self.death_age_sum / n as f64,
                median,
                min: self.death_ages[0],
                max: self.death_ages[n - 1],
            };
        }

        // 移除死亡实体
        self.creatures.retain(|c| c.alive);
        self.energy_particles.retain(|e| e.alive);

        // 清理空家族
        self.family_stats.retain(|_, &mut count| count > 0);

        // 每10秒清理一次相似度缓存（避免每帧都清理的开销）
        if self.time - self.cache_cleanup_timer >= 10.0 {
            self.cache_cleanup_timer = self.time;
            // 收集当前活着生物的 genome_hash
            let alive_hashes: rustc_hash::FxHashSet<u64> = self.creatures.iter()
                .filter(|c| c.alive)
                .map(|c| c.genome_hash)
                .collect();
            // 只保留两端都是活着生物的缓存条目
            let mut cache = self.similarity_cache.borrow_mut();
            cache.retain(|(h1, h2), _| {
                alive_hashes.contains(h1) && alive_hashes.contains(h2)
            });
        }
    }

    /// 获取统计信息
    pub fn stats(&self, species_threshold: f64, config: &Config) -> WorldStats {
        let alive_creatures: Vec<_> = self.creatures.iter().filter(|c| c.alive).collect();
        let alive_families = self.family_stats.len();
        let max_generation = alive_creatures.iter()
            .map(|c| c.generation)
            .max()
            .unwrap_or(0);

        // 计算能量统计
        let creature_energy: f64 = alive_creatures.iter().map(|c| c.energy).sum();
        let particle_energy: f64 = self.energy_particles.iter()
            .filter(|e| e.alive)
            .map(|e| e.energy)
            .sum();
        let total_energy = creature_energy + particle_energy;
        let avg_energy = if alive_creatures.is_empty() {
            0.0
        } else {
            creature_energy / alive_creatures.len() as f64
        };

        // 统计每个功能解锁的生物数
        let mut function_unlocks = [0usize; 10];
        for creature in &alive_creatures {
            for &func_id in &creature.genome.output_map {
                if func_id < 10 {
                    function_unlocks[func_id] += 1;
                }
            }
        }

        // 计算种群分组（使用缓存的聚类结果）
        self.ensure_species_cache(species_threshold);
        let cache = self.species_cache.borrow();
        let cache = cache.as_ref().unwrap();
        let species_count = cache.species_count;
        let top_species = cache.top_species.clone();
        let creature_species_map = cache.creature_species_map.clone();

        // 统计每个家族的种族分布
        // family_id -> (species_id -> count)
        let mut family_species_counts: FxHashMap<usize, FxHashMap<usize, usize>> = FxHashMap::default();
        for (i, creature) in alive_creatures.iter().enumerate() {
            if let Some(&species_id) = creature_species_map.get(&i) {
                *family_species_counts
                    .entry(creature.family_id)
                    .or_default()
                    .entry(species_id)
                    .or_insert(0) += 1;
            }
        }

        // 计算家族前三，找出每个家族的主导种族
        let mut family_vec: Vec<_> = self.family_stats.iter()
            .map(|(&family_id, &count)| {
                // 找出该家族中数量最多的种族
                let dominant_species = family_species_counts.get(&family_id)
                    .and_then(|species| species.iter().max_by_key(|(_, &c)| c))
                    .map(|(&sid, _)| sid)
                    .unwrap_or(0);
                RankedEntry {
                    id: family_id,
                    count,
                    family_id,
                    species_id: dominant_species,
                }
            })
            .collect();
        family_vec.sort_by(|a, b| b.count.cmp(&a.count));
        let top_families: Vec<_> = family_vec.into_iter().take(3).collect();

        // 构建生物ID -> 种族ID映射
        let mut id_species_map: FxHashMap<u64, usize> = FxHashMap::default();
        for (i, creature) in alive_creatures.iter().enumerate() {
            if let Some(&species_id) = creature_species_map.get(&i) {
                id_species_map.insert(creature.id, species_id);
            }
        }

        // 优势种检测
        let dominant_candidate = self.detect_dominant(
            &alive_creatures,
            &creature_species_map,
            avg_energy,
            max_generation,
            config,
        );

        WorldStats {
            time: self.time,
            creature_count: alive_creatures.len(),
            energy_particle_count: self.energy_particles.iter().filter(|e| e.alive).count(),
            total_energy,
            alive_families,
            extinct_families: self.extinct_families,
            max_generation,
            avg_energy,
            function_unlocks,
            action_counts: self.action_counts,
            death_age_stats: self.death_age_stats.clone(),
            species_count,
            top_families,
            top_species,
            creature_species_map: id_species_map,
            dominant_candidate,
        }
    }

    /// 获取渲染上下文数据（种族映射和前三家族ID）
    /// 返回 (creature_index -> 种族XOR基因哈希, 前三家族ID)
    pub fn get_render_data(&self, threshold: f64) -> (FxHashMap<usize, u64>, Vec<usize>) {
        // 复用聚类缓存
        self.ensure_species_cache(threshold);
        let cache_ref = self.species_cache.borrow();
        let cache = match cache_ref.as_ref() {
            Some(c) => c,
            None => return (FxHashMap::default(), Vec::new()),
        };

        if cache.alive_indices.is_empty() {
            return (FxHashMap::default(), Vec::new());
        }

        let n = cache.alive_indices.len();
        let mut parent = cache.parent.clone();

        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

        // 取每个种族中最老成员的 genome_hash 作为稳定颜色标识
        let mut species_elder: FxHashMap<usize, (u64, f64)> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, &idx) in cache.alive_indices.iter().enumerate() {
            let root = find(&mut parent, i);
            let creature = &self.creatures[idx];
            species_elder
                .entry(root)
                .and_modify(|(hash, age)| {
                    if creature.age > *age {
                        *hash = creature.genome_hash;
                        *age = creature.age;
                    }
                })
                .or_insert((creature.genome_hash, creature.age));
        }

        // 构建 creature_index -> 种族颜色哈希 映射
        let mut creature_species: FxHashMap<usize, u64> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, &idx) in cache.alive_indices.iter().enumerate() {
            let root = find(&mut parent, i);
            let (elder_hash, _) = species_elder[&root];
            creature_species.insert(idx, elder_hash);
        }

        // 获取前三家族ID
        let mut family_vec: Vec<_> = self.family_stats.iter()
            .map(|(&id, &count)| (id, count))
            .collect();
        family_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let top_family_ids: Vec<usize> = family_vec.into_iter().take(3).map(|(id, _)| id).collect();

        (creature_species, top_family_ids)
    }

    /// 计算种族分组（使用并查集优化，O(n²·α(n)) 代替 O(n³)）
    /// 优势种检测：遍历各种群，检测触发条件并计算评分
    fn detect_dominant(
        &self,
        alive_creatures: &[&Creature],
        creature_species_map: &FxHashMap<usize, usize>,
        global_avg_energy: f64,
        global_max_generation: usize,
        config: &Config,
    ) -> Option<DominantCandidate> {
        let total_count = alive_creatures.len();
        if total_count < 5 {
            return None;
        }

        let death_median = self.death_age_stats.median;
        let has_death_data = self.death_age_stats.count > 0;

        // 按种群聚合成员索引
        let mut species_members: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        for (i, _) in alive_creatures.iter().enumerate() {
            if let Some(&species_root) = creature_species_map.get(&i) {
                species_members.entry(species_root).or_default().push(i);
            }
        }

        let mut best: Option<DominantCandidate> = None;

        for (_species_root, members) in &species_members {
            let count = members.len();

            // 条件3: 种群数量 >= 5
            if count < 5 {
                continue;
            }

            // 条件2: 种群占比 >= 30%
            let ratio = count as f64 / total_count as f64;
            if ratio < 0.3 {
                continue;
            }

            // 聚合种群统计
            let mut max_age: f64 = 0.0;
            let mut sum_age: f64 = 0.0;
            let mut sum_energy: f64 = 0.0;
            let mut sp_max_gen: usize = 0;
            let mut best_energy_idx: usize = members[0];
            let mut best_energy_val: f64 = f64::NEG_INFINITY;

            for &idx in members {
                let c = alive_creatures[idx];
                if c.age > max_age {
                    max_age = c.age;
                }
                sum_age += c.age;
                sum_energy += c.energy;
                if c.generation > sp_max_gen {
                    sp_max_gen = c.generation;
                }
                if c.energy > best_energy_val {
                    best_energy_val = c.energy;
                    best_energy_idx = idx;
                }
            }

            let sp_avg_age = sum_age / count as f64;
            let sp_avg_energy = sum_energy / count as f64;

            // 条件1: 种群最老成员 age >= 配置阈值
            if max_age < config.dominant_min_age {
                continue;
            }

            // 条件4: 平均年龄 >= 死亡中位数（无死亡数据时跳过）
            if has_death_data && sp_avg_age < death_median {
                continue;
            }

            // 计算评分
            let ratio_score = ratio;  // 种群数/总数
            let energy_score = if global_avg_energy > 0.0 {
                (sp_avg_energy / global_avg_energy).min(2.0) / 2.0
            } else {
                0.0
            };
            let age_score = if has_death_data && death_median > 0.0 {
                (sp_avg_age / death_median).min(2.0) / 2.0
            } else {
                0.5  // 无死亡数据时给中间值
            };
            let gen_score = if global_max_generation > 0 {
                sp_max_gen as f64 / global_max_generation as f64
            } else {
                0.0
            };

            let score = 0.30 * ratio_score
                + 0.20 * energy_score
                + 0.30 * age_score
                + 0.20 * gen_score;

            if score < 0.6 {
                continue;
            }

            // 取种群中能量最高者的 genome 作为代表
            let representative = alive_creatures[best_energy_idx];

            let candidate = DominantCandidate {
                genome: representative.genome.clone(),
                genome_hash: representative.genome_hash,
                score,
                population_ratio: ratio,
                avg_energy: sp_avg_energy,
                avg_age: sp_avg_age,
                max_generation: sp_max_gen,
            };

            if best.as_ref().is_none_or(|b| score > b.score) {
                best = Some(candidate);
            }
        }

        best
    }

    /// 确保聚类缓存有效（同一时刻只计算一次）
    fn ensure_species_cache(&self, threshold: f64) {
        let current_time = self.time;
        if *self.species_cache_time.borrow() == current_time && self.species_cache.borrow().is_some() {
            return;
        }

        let alive_creatures: Vec<&Creature> = self.creatures.iter().filter(|c| c.alive).collect();
        let cache = self.calculate_species_cache(&alive_creatures, threshold);
        *self.species_cache.borrow_mut() = Some(cache);
        *self.species_cache_time.borrow_mut() = current_time;
    }

    /// 返回: (种族数, 最大种族数, 种族前三, 生物索引->种族根索引映射)
    fn calculate_species_cache(&self, alive_creatures: &[&Creature], threshold: f64) -> SpeciesCache {
        if alive_creatures.is_empty() {
            return SpeciesCache {
                parent: Vec::new(),
                alive_indices: Vec::new(),
                species_count: 0,
                top_species: Vec::new(),
                creature_species_map: FxHashMap::default(),
            };
        }

        let n = alive_creatures.len();

        // 并查集：parent[i] 指向父节点，rank[i] 用于优化合并
        let mut parent: Vec<usize> = (0..n).collect();
        let mut rank: Vec<usize> = vec![0; n];

        // 查找根节点（带路径压缩）
        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

        // 合并两个集合（按秩合并）
        fn union(parent: &mut [usize], rank: &mut [usize], x: usize, y: usize) {
            let root_x = find(parent, x);
            let root_y = find(parent, y);
            if root_x != root_y {
                if rank[root_x] < rank[root_y] {
                    parent[root_x] = root_y;
                } else if rank[root_x] > rank[root_y] {
                    parent[root_y] = root_x;
                } else {
                    parent[root_y] = root_x;
                    rank[root_x] += 1;
                }
            }
        }

        // 按结构哈希分桶，只在同桶内做相似度比较（拓扑不同的 similarity 必然很低）
        let mut buckets: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
        for i in 0..n {
            let sh = alive_creatures[i].genome.structural_hash();
            buckets.entry(sh).or_default().push(i);
        }

        for members in buckets.values() {
            for (a_idx, &i) in members.iter().enumerate() {
                for &j in &members[a_idx + 1..] {
                    if self.get_similarity(alive_creatures[i], alive_creatures[j]) >= threshold {
                        union(&mut parent, &mut rank, i, j);
                    }
                }
            }
        }

        // 统计各种族数量和家族分布
        let mut species_counts: FxHashMap<usize, usize> = FxHashMap::default();
        // species_root -> (family_id -> count)
        let mut species_family_counts: FxHashMap<usize, FxHashMap<usize, usize>> = FxHashMap::default();
        // creature_index -> species_root
        let mut creature_species_map: FxHashMap<usize, usize> = FxHashMap::default();

        for i in 0..n {
            let root = find(&mut parent, i);
            let family_id = alive_creatures[i].family_id;
            *species_counts.entry(root).or_insert(0) += 1;
            *species_family_counts.entry(root).or_default().entry(family_id).or_insert(0) += 1;
            creature_species_map.insert(i, root);
        }

        let species_count = species_counts.len();

        // 计算种群前三，找出每个种群的主导家族
        let mut species_vec: Vec<_> = species_counts.into_iter()
            .map(|(species_id, count)| {
                // 找出该种族中数量最多的家族
                let dominant_family = species_family_counts.get(&species_id)
                    .and_then(|families| families.iter().max_by_key(|(_, &c)| c))
                    .map(|(&fid, _)| fid)
                    .unwrap_or(0);
                RankedEntry {
                    id: species_id,
                    count,
                    family_id: dominant_family,
                    species_id,
                }
            })
            .collect();
        species_vec.sort_by(|a, b| b.count.cmp(&a.count));
        let top_species: Vec<_> = species_vec.into_iter().take(3).collect();

        // 收集 alive_indices（creatures 数组中的原始索引）
        let alive_indices: Vec<usize> = (0..self.creatures.len())
            .filter(|&i| self.creatures[i].alive)
            .collect();

        SpeciesCache {
            parent,
            alive_indices,
            species_count,
            top_species,
            creature_species_map,
        }
    }
}

/// 死亡年龄统计
#[derive(Default, Clone)]
pub struct DeathAgeStats {
    pub count: usize,
    pub avg: f64,
    pub median: f64,
    pub max: f64,
    pub min: f64,
}

/// 排名数据
#[derive(Clone, Default)]
pub struct RankedEntry {
    pub id: usize,
    pub count: usize,
    pub family_id: usize,   // 关联的家族ID（种族排行用）
    pub species_id: usize,  // 关联的种族ID（家族排行用）
}

/// 世界统计
pub struct WorldStats {
    pub time: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub total_energy: f64,         // 总能量（生物+粒子）
    pub alive_families: usize,
    pub extinct_families: usize,
    pub max_generation: usize,
    pub avg_energy: f64,
    // 功能解锁统计（每个功能解锁的生物数）
    pub function_unlocks: [usize; 10],
    // 行为触发统计（累计触发次数）
    pub action_counts: [usize; 10],
    // 死亡年龄统计
    pub death_age_stats: DeathAgeStats,
    // 种群统计
    pub species_count: usize,      // 种群数量
    pub top_families: Vec<RankedEntry>,   // 前三家族
    pub top_species: Vec<RankedEntry>,    // 前三种群
    pub creature_species_map: FxHashMap<u64, usize>,  // 生物ID -> 种群ID
    // 优势种候选
    pub dominant_candidate: Option<DominantCandidate>,
}

/// 优势种候选
#[derive(Clone)]
pub struct DominantCandidate {
    pub genome: Genome,
    pub genome_hash: u64,
    pub score: f64,
    pub population_ratio: f64,
    pub avg_energy: f64,
    pub avg_age: f64,
    pub max_generation: usize,
}
