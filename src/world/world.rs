use rand::Rng;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::time::Instant;

use crate::config::Config;
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

    // 行为触发次数统计（8个功能）
    pub action_counts: [usize; 8],
}

impl World {
    pub fn new(config: &Config) -> Self {
        Self {
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
            action_counts: [0; 8],
        }
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

        // 自动补充生物（已禁用，让演化自然进行）
        // self.replenish_creatures(config);

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
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();
        while alive_count < config.min_creatures {
            self.spawn_creature(config);
            if self.creatures.iter().filter(|c| c.alive).count() >= config.min_creatures {
                break;
            }
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

            // 基础代谢 = 固定消耗 + 百分比消耗
            let base_cost = config.base_metabolism * dt;
            let percent_cost = self.creatures[i].energy * config.percent_metabolism * dt;
            self.creatures[i].energy -= base_cost + percent_cost;
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

    /// 更新雷达扫描
    fn update_scan(&mut self, creature_idx: usize, dt: f64, config: &Config) {
        let creature = &self.creatures[creature_idx];
        let old_angle = creature.scan_angle;
        let angular_velocity = creature.scan_angular_velocity;
        let scan_radius = creature.scan_radius;
        let last_perception_time = creature.last_perception_time;

        // 新生物初始化：做一次完整360度扫描（免费）
        if last_perception_time < 0.0 {
            for degree in 0..360 {
                let degree_rad = (degree as f64).to_radians();
                self.scan_degree(creature_idx, degree_rad, scan_radius, config);
            }
            self.compute_perception(creature_idx, config);
            self.creatures[creature_idx].last_perception_time = self.time;
            self.creatures[creature_idx].scan_cache.clear();
            return;
        }

        // 更新扫描角度
        let mut new_angle = old_angle + angular_velocity * dt;

        // 计算跨越的整度数
        let old_degree = old_angle.floor() as i32;
        let mut new_degree = new_angle.floor() as i32;

        // 处理角度回绕
        if new_angle >= 360.0 {
            new_angle %= 360.0;
            new_degree = 359;  // 确保扫描到 359 度
        }

        self.creatures[creature_idx].scan_angle = new_angle;

        // 对每个跨越的度数进行扫描和耗能
        for degree in (old_degree + 1)..=new_degree {
            let degree_rad = (degree as f64).to_radians();

            // 扫描该角度扇形内的实体
            self.scan_degree(creature_idx, degree_rad, scan_radius, config);

            // 计算扫描耗能: max(0, r - 50)² × (π/360) × cost
            let excess_radius = (scan_radius - config.scan_free_radius).max(0.0);
            let scan_cost = excess_radius * excess_radius * std::f64::consts::PI / 360.0 * config.scan_cost;
            self.creatures[creature_idx].energy -= scan_cost;

            // 检查是否需要更新感知（超过1秒且跨过一度时）
            let time_since_last = self.time - last_perception_time;
            if time_since_last >= 1.0 {
                self.compute_perception(creature_idx, config);
                self.creatures[creature_idx].last_perception_time = self.time;
                // 清空缓存，开始新一轮收集
                self.creatures[creature_idx].scan_cache.clear();
            }
        }
    }

    /// 扫描指定角度的扇形区域
    fn scan_degree(&mut self, creature_idx: usize, angle_rad: f64, radius: f64, _config: &Config) {
        let creature = &self.creatures[creature_idx];
        let cx = creature.x;
        let cy = creature.y;

        // 扫描方向向量
        let scan_dx = angle_rad.cos();
        let scan_dy = angle_rad.sin();

        // 扇形半角（约 0.5 度 = 0.00873 弧度）
        let half_angle = 0.5_f64.to_radians();
        let cos_half = half_angle.cos();

        // 先收集数据，避免借用冲突
        let mut new_results: Vec<ScanResult> = Vec::new();

        // 查询附近的生物
        let nearby_creatures = self.creature_grid.query(cx, cy, radius);
        for &idx in &nearby_creatures {
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

            if dist > 0.0 && dist <= radius {
                // 检查是否在扫描扇形内
                let dot = (dx * scan_dx + dy * scan_dy) / dist;
                if dot >= cos_half {
                    // 计算角度
                    let angle = dy.atan2(dx).to_degrees();
                    let angle = if angle < 0.0 { angle + 360.0 } else { angle };

                    // 计算相似度
                    let similarity = self.get_similarity(&self.creatures[creature_idx], other);

                    new_results.push(ScanResult {
                        angle,
                        distance: dist,
                        similarity,
                        energy: other.energy,
                    });
                }
            }
        }

        // 查询附近的能量粒子
        let nearby_energy = self.energy_grid.query(cx, cy, radius);
        for &idx in &nearby_energy {
            let particle = &self.energy_particles[idx];
            if !particle.alive {
                continue;
            }

            let dx = particle.x - cx;
            let dy = particle.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist > 0.0 && dist <= radius {
                let dot = (dx * scan_dx + dy * scan_dy) / dist;
                if dot >= cos_half {
                    let angle = dy.atan2(dx).to_degrees();
                    let angle = if angle < 0.0 { angle + 360.0 } else { angle };

                    new_results.push(ScanResult {
                        angle,
                        distance: dist,
                        similarity: 0.0,  // 能量粒子无相似度
                        energy: particle.energy,
                    });
                }
            }
        }

        // 将收集的结果推入缓存
        self.creatures[creature_idx].scan_cache.extend(new_results);
    }

    /// 从扫描缓存计算 17 维感知输入
    /// [0-3]   最近最大同类: 角度, 距离, 相似度, 能量
    /// [4-7]   最近最小同类: 角度, 距离, 相似度, 能量
    /// [8-11]  最近最大异类: 角度, 距离, 相似度, 能量
    /// [12-15] 最近最小异类: 角度, 距离, 相似度, 能量
    /// [16]    自身能量
    fn compute_perception(&mut self, creature_idx: usize, config: &Config) {
        let creature = &self.creatures[creature_idx];
        let threshold = config.species_similarity_threshold;

        let mut input = [0.0; 17];

        // 分类：同类 vs 异类（能量粒子视为异类，相似度=0）
        let mut allies: Vec<&ScanResult> = Vec::new();
        let mut enemies: Vec<&ScanResult> = Vec::new();

        for result in &creature.scan_cache {
            if result.similarity >= threshold {
                allies.push(result);
            } else {
                enemies.push(result);
            }
        }

        // 评分函数: 能量 / 距离
        let score = |r: &ScanResult| -> f64 {
            if r.distance > 0.0 { r.energy / r.distance } else { r.energy * 1000.0 }
        };

        // 角度编码: -1~1（与输出方向编码一致）
        // 0°=-1, 180°=0, 360°=1
        // 这样追逐只需权重≈+1，躲避只需权重≈-1

        // 最近最大同类（评分最高）
        if let Some(best_ally) = allies.iter().max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            input[0] = best_ally.angle / 180.0 - 1.0;
            input[1] = (best_ally.distance / config.scan_max_radius).min(1.0);
            input[2] = best_ally.similarity;
            input[3] = (best_ally.energy / 200.0).min(1.0);
        }

        // 最近最小同类（评分最低）
        if let Some(worst_ally) = allies.iter().min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            input[4] = worst_ally.angle / 180.0 - 1.0;
            input[5] = (worst_ally.distance / config.scan_max_radius).min(1.0);
            input[6] = worst_ally.similarity;
            input[7] = (worst_ally.energy / 200.0).min(1.0);
        }

        // 最近最大异类（评分最高）
        if let Some(best_enemy) = enemies.iter().max_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            input[8] = best_enemy.angle / 180.0 - 1.0;
            input[9] = (best_enemy.distance / config.scan_max_radius).min(1.0);
            input[10] = best_enemy.similarity;
            input[11] = (best_enemy.energy / 200.0).min(1.0);
        }

        // 最近最小异类（评分最低）
        if let Some(worst_enemy) = enemies.iter().min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap()) {
            input[12] = worst_enemy.angle / 180.0 - 1.0;
            input[13] = (worst_enemy.distance / config.scan_max_radius).min(1.0);
            input[14] = worst_enemy.similarity;
            input[15] = (worst_enemy.energy / 200.0).min(1.0);
        }

        // 自身能量
        input[16] = (creature.energy / 200.0).min(1.0);

        self.creatures[creature_idx].perception_cache = input;
    }

    /// 执行动作
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        let output_map = self.creatures[creature_idx].genome.output_map.clone();

        // 先收集移动方向和速度（需要组合使用）
        let mut move_direction: Option<f64> = None;
        let mut move_speed: Option<f64> = None;

        for (out_idx, &func_id) in output_map.iter().enumerate() {
            if out_idx >= outputs.len() {
                break;
            }
            let value = outputs[out_idx];

            match func_id {
                0 => move_direction = Some(value),  // 移动方向
                1 => move_speed = Some(value),      // 移动速度
                2 => {
                    if self.action_absorb(creature_idx, value, config) {
                        self.action_counts[2] += 1;  // 只统计实际吸收成功
                    }
                }
                3 => {
                    if self.action_release(creature_idx, value, config) {
                        self.action_counts[3] += 1;  // 只统计实际释放成功
                    }
                }
                4 => {
                    if self.action_reproduce(creature_idx, value, config) {
                        self.action_counts[4] += 1;  // 只统计实际繁殖成功
                    }
                }
                5 => {
                    if self.action_transfer(creature_idx, value, config) {
                        self.action_counts[5] += 1;  // 只统计实际转移成功
                    }
                }
                6 => {
                    // 扫描半径调整
                    self.action_set_scan_radius(creature_idx, value, config);
                }
                7 => {
                    // 扫描角速度调整
                    self.action_set_scan_velocity(creature_idx, value, config);
                }
                _ => {}
            }
        }

        // 执行移动（需要方向和速度都有值，且速度>0.1才统计）
        if let (Some(dir), Some(spd)) = (move_direction, move_speed) {
            if spd.abs() > 0.1 {
                self.action_move(creature_idx, dir, spd, dt, config);
                self.action_counts[0] += 1;
            }
        }
    }

    // 功能 0+1: 移动（方向 + 速度）
    fn action_move(&mut self, idx: usize, direction: f64, speed: f64, dt: f64, config: &Config) {
        // direction: -1~1 映射到 0~360 度
        // speed: -1~1，绝对值为速度
        let angle_deg = (direction + 1.0) / 2.0 * 360.0;  // 0~360
        let angle_rad = angle_deg.to_radians();
        let actual_speed = speed.abs() * 50.0;  // 最大速度 50 单位/秒

        let dx = angle_rad.cos() * actual_speed * dt;
        let dy = angle_rad.sin() * actual_speed * dt;

        self.creatures[idx].x += dx;
        self.creatures[idx].y += dy;

        // 移动消耗
        let distance = (dx * dx + dy * dy).sqrt();
        self.creatures[idx].energy -= distance * config.move_cost;
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

    // 功能 4: 繁殖
    // 返回是否成功繁殖
    fn action_reproduce(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        // 降低阈值使繁殖更容易触发
        if value <= 0.2 {
            return false;
        }

        // 检查是否超过最大生物数量
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();
        if alive_count >= config.max_creatures {
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

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let child = self.creatures[idx].reproduce(
            creature_id,
            self.creatures[idx].x + offset_x,
            self.creatures[idx].y + offset_y,
            child_energy,
            config.mutation_rate,
        );

        let family_id = child.family_id;
        self.creatures.push(child);
        *self.family_stats.entry(family_id).or_insert(0) += 1;
        true
    }

    // 功能 5: 能量转移
    // 返回是否成功转移了能量
    fn action_transfer(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        if value.abs() < 0.1 {
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
                // 正值 = 掠夺（按目标能量百分比），负值 = 给予
                let target_energy = self.creatures[other_idx].energy;

                if value > 0.0 {
                    // 掠夺：转移目标能量的 value*20%（最高20%）
                    let transfer_ratio = value * 0.2;
                    let transfer_amount = target_energy * transfer_ratio;
                    self.creatures[other_idx].energy -= transfer_amount;
                    self.creatures[idx].energy += transfer_amount;
                } else {
                    // 给予：转移自身能量的 |value|*20%
                    let transfer_ratio = (-value) * 0.2;
                    let my_energy = self.creatures[idx].energy;
                    let transfer_amount = my_energy * transfer_ratio;
                    self.creatures[idx].energy -= transfer_amount;
                    self.creatures[other_idx].energy += transfer_amount;
                }
                return true; // 转移成功
            }
        }
        false
    }

    // 功能 6: 设置扫描半径
    fn action_set_scan_radius(&mut self, idx: usize, value: f64, config: &Config) {
        // value: -1~1 映射到 free_radius ~ max_radius
        let normalized = (value + 1.0) / 2.0;  // 0~1
        let radius = config.scan_free_radius + normalized * (config.scan_max_radius - config.scan_free_radius);
        self.creatures[idx].scan_radius = radius;
    }

    // 功能 7: 设置扫描角速度
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
        // 统计死亡的家族
        for creature in &self.creatures {
            if !creature.alive {
                if let Some(count) = self.family_stats.get_mut(&creature.family_id) {
                    *count -= 1;
                    if *count == 0 {
                        self.extinct_families += 1;
                    }
                }
            }
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
    pub fn stats(&self, species_threshold: f64) -> WorldStats {
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
        let mut function_unlocks = [0usize; 8];
        for creature in &alive_creatures {
            for &func_id in &creature.genome.output_map {
                if func_id < 8 {
                    function_unlocks[func_id] += 1;
                }
            }
        }

        // 计算种群分组（使用并查集思想）
        let (species_count, top_species, creature_species_map) =
            self.calculate_species(&alive_creatures, species_threshold);

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
            species_count,
            top_families,
            top_species,
            creature_species_map: id_species_map,
        }
    }

    /// 获取渲染上下文数据（种族映射和前三家族ID）
    /// 返回 (creature_index -> 种族XOR基因哈希, 前三家族ID)
    pub fn get_render_data(&self, threshold: f64) -> (FxHashMap<usize, u64>, Vec<usize>) {
        // 预分配 Vec 容量以减少重新分配
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();
        let mut alive_indices: Vec<usize> = Vec::with_capacity(alive_count);
        for (i, c) in self.creatures.iter().enumerate() {
            if c.alive {
                alive_indices.push(i);
            }
        }

        if alive_indices.is_empty() {
            return (FxHashMap::default(), Vec::new());
        }

        let n = alive_indices.len();

        // 并查集优化聚类（O(n²·α(n)) 代替 O(n³)）
        let mut parent: Vec<usize> = (0..n).collect();
        let mut rank: Vec<usize> = vec![0; n];

        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }

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

        for i in 0..n {
            for j in (i + 1)..n {
                let ci = &self.creatures[alive_indices[i]];
                let cj = &self.creatures[alive_indices[j]];
                if self.get_similarity(ci, cj) >= threshold {
                    union(&mut parent, &mut rank, i, j);
                }
            }
        }

        // 计算每个种族的基因哈希 XOR（用于稳定且分散的颜色标识）
        // XOR 比 min 更能产生分散的值，同时保持相对稳定
        let mut species_xor_hash: FxHashMap<usize, u64> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, &idx) in alive_indices.iter().enumerate() {
            let root = find(&mut parent, i);
            let hash = self.creatures[idx].genome_hash;
            species_xor_hash
                .entry(root)
                .and_modify(|xor| *xor ^= hash)
                .or_insert(hash);
        }

        // 构建 creature_index -> 种族 XOR 基因哈希 映射
        let mut creature_species: FxHashMap<usize, u64> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, &idx) in alive_indices.iter().enumerate() {
            let root = find(&mut parent, i);
            let xor_hash = species_xor_hash[&root];
            creature_species.insert(idx, xor_hash);
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
    /// 返回: (种族数, 最大种族数, 种族前三, 生物索引->种族根索引映射)
    fn calculate_species(&self, alive_creatures: &[&Creature], threshold: f64) -> (usize, Vec<RankedEntry>, FxHashMap<usize, usize>) {
        if alive_creatures.is_empty() {
            return (0, Vec::new(), FxHashMap::default());
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

        // 聚类：将相似的生物归为同一种族（使用缓存）
        for i in 0..n {
            for j in (i + 1)..n {
                if self.get_similarity(alive_creatures[i], alive_creatures[j]) >= threshold {
                    union(&mut parent, &mut rank, i, j);
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

        (species_count, top_species, creature_species_map)
    }
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
    pub function_unlocks: [usize; 8],
    // 行为触发统计（累计触发次数）
    pub action_counts: [usize; 8],
    // 种群统计
    pub species_count: usize,      // 种群数量
    pub top_families: Vec<RankedEntry>,   // 前三家族
    pub top_species: Vec<RankedEntry>,    // 前三种群
    pub creature_species_map: FxHashMap<u64, usize>,  // 生物ID -> 种群ID
}
