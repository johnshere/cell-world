use rand::Rng;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::time::Instant;

use crate::config::Config;
use crate::neural::Genome;
use super::{Creature, EnergyParticle, SpatialGrid};

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

    // 内部状态
    volcano_timer: f64,
    meteorite_timer: f64,

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

    // 相似度缓存
    similarity_cache: RefCell<FxHashMap<(u64, u64), f64>>,
    cache_cleanup_timer: f64,

    // 行为触发次数统计（5事件：移动/吸收/咬/喂/繁殖）
    pub action_counts: [usize; 5],

    // 死亡年龄统计
    death_ages: Vec<f64>,
    death_age_sum: f64,
    pub death_age_stats: DeathAgeStats,

    // 种族缓存（祖先追溯模型）
    clan_cache: RefCell<Option<ClanCache>>,
    clan_cache_time: RefCell<f64>,

    // 空间查询缓冲区复用
    creature_query_buf: Vec<usize>,
    energy_query_buf: Vec<usize>,
}

/// 种族缓存（祖先追溯模型）
#[derive(Clone)]
struct ClanCache {
    /// 活生物在 creatures 数组中的原始索引
    alive_indices: Vec<usize>,
    /// 种族数量
    species_count: usize,
    /// 前三种族
    top_species: Vec<RankedEntry>,
    /// 活生物局部索引 -> 族长 creature id
    creature_clan_map: FxHashMap<usize, u64>,
    /// 族长 id -> 族长 genome_hash（用于颜色）
    clan_color: FxHashMap<u64, u64>,
}

impl World {
    pub fn new(config: &Config) -> Self {
        let mut world = Self {
            creatures: Vec::new(),
            energy_particles: Vec::new(),
            creature_grid: SpatialGrid::new(config.vision_range),
            energy_grid: SpatialGrid::new(config.vision_range),
            time: 0.0,
            volcano_timer: 0.0,
            meteorite_timer: 0.0,
            next_creature_id: 0,
            next_energy_id: 0,
            // 初始视窗居中于原点
            viewport_min_x: -400.0,
            viewport_min_y: -300.0,
            viewport_max_x: 400.0,
            viewport_max_y: 300.0,
            perf_stats: PerfStats::default(),
            similarity_cache: RefCell::new(FxHashMap::default()),
            cache_cleanup_timer: 0.0,
            action_counts: [0; 5],
            death_ages: Vec::new(),
            death_age_sum: 0.0,
            death_age_stats: DeathAgeStats::default(),
            clan_cache: RefCell::new(None),
            clan_cache_time: RefCell::new(-999.0),
            creature_query_buf: Vec::new(),
            energy_query_buf: Vec::new(),
        };
        // 初始火山喷发一次，提供起始能量
        world.volcano_erupt(config);

        for _ in 0..config.min_creatures {
            world.spawn_creature(config);
        }
        world
    }

    /// 获取缓存的相似度
    fn get_similarity(&self, creature_a: &Creature, creature_b: &Creature) -> f64 {
        let hash_a = creature_a.genome_hash;
        let hash_b = creature_b.genome_hash;
        let key = if hash_a <= hash_b { (hash_a, hash_b) } else { (hash_b, hash_a) };
        let mut cache = self.similarity_cache.borrow_mut();
        *cache.entry(key).or_insert_with(|| {
            creature_a.genome.similarity(&creature_b.genome)
        })
    }

    /// 设置视窗范围
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

        // 自动补充生物
        self.replenish_creatures(config);

        // 重建空间索引
        let spatial_start = Instant::now();
        self.rebuild_spatial_index();
        self.perf_stats.spatial_ms = spatial_start.elapsed().as_secs_f64() * 1000.0;

        // 更新生物
        self.update_creatures(dt, config);

        // 更新能量粒子
        self.update_energy_particles(dt, config);

        // 清理
        self.cleanup();
    }

    /// 火山喷发倒计时
    pub fn volcano_countdown(&self, config: &Config) -> f64 {
        (config.volcano_interval - self.volcano_timer).max(0.0)
    }

    // ========== 生成 ==========

    fn replenish_creatures(&mut self, config: &Config) {
        loop {
            let alive_count = self.creatures.iter().filter(|c| c.alive).count();
            if alive_count >= config.min_creatures {
                break;
            }
            self.spawn_creature(config);
        }
    }

    /// 在火山附近生成新生物
    pub fn spawn_creature(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let r = rng.gen_range(0.0_f64..1.0).sqrt() * config.volcano_radius * 0.8;
        let x = config.volcano_x + r * angle.cos();
        let y = config.volcano_y + r * angle.sin();
        let energy = config.initial_energy * rng.gen_range(0.8..1.2);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let creature = Creature::random(
            creature_id,
            x,
            y,
            energy,
            config.initial_connections_min,
            config.initial_connections_max,
        );
        self.creatures.push(creature);
    }

    /// 从模板生成生物
    pub fn spawn_from_template(&mut self, config: &Config, genome: &Genome, initial_energy: f64) {
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let r = rng.gen_range(0.0_f64..1.0).sqrt() * config.volcano_radius * 0.8;
        let x = config.volcano_x + r * angle.cos();
        let y = config.volcano_y + r * angle.sin();
        let energy = initial_energy.max(config.initial_energy * 0.5);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let creature = Creature::new(creature_id, x, y, energy, genome.clone(), 0, None);
        self.creatures.push(creature);
    }

    /// 杀死指定生物
    pub fn kill_creature(&mut self, id: u64) {
        if let Some(creature) = self.creatures.iter_mut().find(|c| c.id == id) {
            creature.alive = false;
        }
    }

    /// 生成能量粒子（火山喷发 + 随机陨石）
    fn spawn_energy(&mut self, dt: f64, config: &Config) {
        self.volcano_timer += dt;
        if self.volcano_timer >= config.volcano_interval {
            self.volcano_timer = 0.0;
            self.volcano_erupt(config);
        }

        self.meteorite_timer += dt;
        if self.meteorite_timer >= config.meteorite_interval {
            self.meteorite_timer = 0.0;
            self.meteorite_fall(config);
        }
    }

    fn volcano_erupt(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        for _ in 0..config.volcano_count {
            let angle = rng.gen_range(0.0..std::f64::consts::TAU);
            // 内密外疏：立方分布，中心密度远高于边缘
            let r = rng.gen_range(0.0_f64..1.0).powi(3) * config.volcano_radius;
            let x = config.volcano_x + r * angle.cos();
            let y = config.volcano_y + r * angle.sin();
            let energy_id = self.next_energy_id;
            self.next_energy_id += 1;
            self.energy_particles.push(EnergyParticle::new(
                energy_id, x, y, config.volcano_particle_energy, f64::MAX,
            ));
        }
    }

    fn meteorite_fall(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let cx = rng.gen_range(self.viewport_min_x..self.viewport_max_x);
        let cy = rng.gen_range(self.viewport_min_y..self.viewport_max_y);
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let dx = angle.cos();
        let dy = angle.sin();
        let half_len = config.meteorite_length / 2.0;

        for i in 0..config.meteorite_count {
            let t = if config.meteorite_count > 1 {
                (i as f64 / (config.meteorite_count - 1) as f64) * 2.0 - 1.0
            } else {
                0.0
            };
            let perp_offset = rng.gen_range(-5.0..5.0);
            let x = cx + dx * t * half_len + (-dy) * perp_offset;
            let y = cy + dy * t * half_len + dx * perp_offset;

            let energy_id = self.next_energy_id;
            self.next_energy_id += 1;
            self.energy_particles.push(EnergyParticle::new(
                energy_id, x, y, config.meteorite_particle_energy, f64::MAX,
            ));
        }
    }

    // ========== 空间索引 ==========

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

    // ========== 更新生物 ==========

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

            // 基础代谢
            let age_multiplier = 1.0 + self.creatures[i].age * config.age_metabolism_factor;
            let metabolism_cost = config.base_metabolism * age_multiplier * dt;
            self.creatures[i].energy -= metabolism_cost;
            self.creatures[i].age += dt;

            if self.creatures[i].energy <= 0.0 {
                self.creatures[i].alive = false;
                continue;
            }

            // 感知（3眼模型）
            let t0 = Instant::now();
            self.compute_eye_perception(i, config);
            perceive_time += t0.elapsed().as_secs_f64() * 1000.0;

            // 神经网络决策
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
        self.perf_stats.perceive_ms = perceive_time;
        self.perf_stats.forward_ms = forward_time;
        self.perf_stats.actions_ms = actions_time;
        self.perf_stats.total_ms = total_time;
        self.perf_stats.creature_count = alive_count;
    }

    // ========== 3眼感知系统 ==========

    /// 计算 10 维感知输入
    /// 3只眼（左-45°, 中0°, 右+45°），每只眼3通道:
    ///   食物接近度, 同族接近度, 异族接近度
    /// + 自身能量
    fn compute_eye_perception(&mut self, creature_idx: usize, config: &Config) {
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;
        let heading = self.creatures[creature_idx].heading;
        let vision = config.vision_range;
        let threshold = config.species_similarity_threshold;

        // 3只眼的方向（弧度）
        let eye_dirs = [
            heading - std::f64::consts::FRAC_PI_4,  // 左眼 -45°
            heading,                                  // 中眼
            heading + std::f64::consts::FRAC_PI_4,  // 右眼 +45°
        ];
        // 每只眼的半角（30°）
        let half_fov = std::f64::consts::PI / 6.0;

        let mut input = [0.0_f64; 10];

        // 每只眼跟踪最近的食物/同族/异族
        let mut eye_food = [f64::MAX; 3];
        let mut eye_ally = [f64::MAX; 3];
        let mut eye_enemy = [f64::MAX; 3];

        // 临时取出缓冲区
        let mut creature_buf = std::mem::take(&mut self.creature_query_buf);
        let mut energy_buf = std::mem::take(&mut self.energy_query_buf);

        // 查询邻近能量粒子
        self.energy_grid.query_into(cx, cy, vision, &mut energy_buf);
        for &idx in &energy_buf {
            let particle = &self.energy_particles[idx];
            if !particle.alive { continue; }

            let dx = particle.x - cx;
            let dy = particle.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 0.0 || dist > vision { continue; }

            let angle = dy.atan2(dx);

            for (eye_i, &eye_dir) in eye_dirs.iter().enumerate() {
                let mut diff = angle - eye_dir;
                // 归一化到 [-π, π]
                while diff > std::f64::consts::PI { diff -= std::f64::consts::TAU; }
                while diff < -std::f64::consts::PI { diff += std::f64::consts::TAU; }

                if diff.abs() <= half_fov && dist < eye_food[eye_i] {
                    eye_food[eye_i] = dist;
                }
            }
        }

        // 查询邻近生物
        self.creature_grid.query_into(cx, cy, vision, &mut creature_buf);
        for &idx in &creature_buf {
            if idx == creature_idx { continue; }
            let other = &self.creatures[idx];
            if !other.alive { continue; }

            let dx = other.x - cx;
            let dy = other.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 0.0 || dist > vision { continue; }

            let angle = dy.atan2(dx);
            let similarity = self.get_similarity(
                &self.creatures[creature_idx],
                &self.creatures[idx],
            );
            let is_ally = similarity >= threshold;

            for (eye_i, &eye_dir) in eye_dirs.iter().enumerate() {
                let mut diff = angle - eye_dir;
                while diff > std::f64::consts::PI { diff -= std::f64::consts::TAU; }
                while diff < -std::f64::consts::PI { diff += std::f64::consts::TAU; }

                if diff.abs() <= half_fov {
                    if is_ally {
                        if dist < eye_ally[eye_i] {
                            eye_ally[eye_i] = dist;
                        }
                    } else if dist < eye_enemy[eye_i] {
                        eye_enemy[eye_i] = dist;
                    }
                }
            }
        }

        // 归还缓冲区
        self.creature_query_buf = creature_buf;
        self.energy_query_buf = energy_buf;

        // 转换为接近度 (0~1, 越近越高)
        for eye_i in 0..3 {
            let base = eye_i * 3;
            input[base]     = if eye_food[eye_i] < f64::MAX  { 1.0 - eye_food[eye_i] / vision  } else { 0.0 };
            input[base + 1] = if eye_ally[eye_i] < f64::MAX  { 1.0 - eye_ally[eye_i] / vision  } else { 0.0 };
            input[base + 2] = if eye_enemy[eye_i] < f64::MAX { 1.0 - eye_enemy[eye_i] / vision } else { 0.0 };
        }

        // 自身能量
        input[9] = (self.creatures[creature_idx].energy / 200.0).min(1.0);

        self.creatures[creature_idx].perception_cache = input;
    }

    // ========== 动作系统（4输出） ==========

    /// 执行动作：转向(0), 速度(1), 嘴(2), 繁殖(3)
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        // 输出0: 转向
        let turn = outputs.get(0).copied().unwrap_or(0.0);
        // 输出1: 速度
        let speed = outputs.get(1).copied().unwrap_or(0.0);
        // 输出2: 嘴
        let mouth = outputs.get(2).copied().unwrap_or(0.0);
        // 输出3: 繁殖
        let reproduce = outputs.get(3).copied().unwrap_or(0.0);

        // 转向 + 移动
        let turn_rate = std::f64::consts::PI * 2.0; // 最大每秒一圈
        self.creatures[creature_idx].heading += turn * turn_rate * dt;

        let actual_speed = speed.abs() * 50.0;
        if actual_speed > 0.1 {
            let heading = self.creatures[creature_idx].heading;
            let dx = heading.cos() * actual_speed * dt;
            let dy = heading.sin() * actual_speed * dt;
            self.creatures[creature_idx].x += dx;
            self.creatures[creature_idx].y += dy;

            let distance = (dx * dx + dy * dy).sqrt();
            self.creatures[creature_idx].energy -= distance * config.move_cost * actual_speed;
            self.action_counts[0] += 1; // 移动
        }

        // 嘴：接触食物自动吸收 + 对生物咬/喂
        self.action_mouth(creature_idx, mouth, config);

        // 繁殖
        if reproduce > 0.2 {
            if self.action_reproduce(creature_idx, reproduce, config) {
                self.action_counts[4] += 1; // 繁殖
            }
        }
    }

    /// 嘴动作：接触食物自动吸收，对生物根据mouth值咬或喂
    fn action_mouth(&mut self, idx: usize, mouth: f64, config: &Config) {
        let cx = self.creatures[idx].x;
        let cy = self.creatures[idx].y;

        // 接触食物自动吸收
        let nearby_energy = self.energy_grid.query(cx, cy, config.contact_range);
        for &particle_idx in &nearby_energy {
            if self.energy_particles[particle_idx].alive {
                let px = self.energy_particles[particle_idx].x;
                let py = self.energy_particles[particle_idx].y;
                let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                if dist < config.contact_range {
                    let energy = self.energy_particles[particle_idx].consume();
                    self.creatures[idx].energy += energy;
                    self.action_counts[1] += 1; // 吸收
                    break; // 每帧吸收一个
                }
            }
        }

        // 对生物：mouth < -0.1 咬, mouth > 0.1 喂
        if mouth.abs() <= 0.1 {
            return;
        }

        let nearby_creatures = self.creature_grid.query(cx, cy, config.contact_range);
        for &other_idx in &nearby_creatures {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }
            let ox = self.creatures[other_idx].x;
            let oy = self.creatures[other_idx].y;
            let dist = ((ox - cx).powi(2) + (oy - cy).powi(2)).sqrt();
            if dist >= config.contact_range {
                continue;
            }

            if mouth < -0.1 {
                // 咬（捕食）— 相似度越高收益越低（生化兼容性）
                let similarity = self.get_similarity(&self.creatures[idx], &self.creatures[other_idx]);
                let bite_strength = (-mouth).min(1.0);
                let transfer_ratio = bite_strength * 0.2;
                let target_energy = self.creatures[other_idx].energy;
                let transfer_amount = target_energy * transfer_ratio;
                let efficiency = 1.0 - similarity;
                self.creatures[other_idx].energy -= transfer_amount;
                self.creatures[idx].energy += transfer_amount * efficiency;
                self.action_counts[2] += 1; // 咬
            } else {
                // 喂（哺育）
                let feed_strength = mouth.min(1.0);
                let transfer_ratio = feed_strength * 0.2;
                let my_energy = self.creatures[idx].energy;
                let transfer_amount = my_energy * transfer_ratio;
                self.creatures[idx].energy -= transfer_amount;
                self.creatures[other_idx].energy += transfer_amount;
                self.action_counts[3] += 1; // 喂
            }
            break; // 每帧只对一个目标
        }
    }

    /// 繁殖（支持有性/无性）
    fn action_reproduce(&mut self, idx: usize, value: f64, config: &Config) -> bool {
        if value <= 0.2 { return false; }
        if self.creatures[idx].energy < config.reproduce_threshold { return false; }

        let child_energy = self.creatures[idx].energy * config.reproduce_energy_ratio;
        self.creatures[idx].energy -= child_energy;

        let mut rng = rand::thread_rng();
        let offset_x = rng.gen_range(-10.0..10.0);
        let offset_y = rng.gen_range(-10.0..10.0);

        // 尝试找同种配偶
        let mate_genome = self.find_mate(idx, config);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;

        let child = if let Some(mate_genome) = mate_genome {
            let crossover_genome = Genome::crossover(
                &self.creatures[idx].genome,
                &mate_genome,
                true,
            );
            let child_genome = crossover_genome.mutate(config.mutation_rate);
            Creature::new(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                child_genome,
                self.creatures[idx].generation + 1,
                Some(self.creatures[idx].id),
            )
        } else {
            self.creatures[idx].reproduce(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                config.mutation_rate,
            )
        };

        self.creatures.push(child);
        true
    }

    fn find_mate(&self, idx: usize, config: &Config) -> Option<Genome> {
        let creature = &self.creatures[idx];
        let nearby = self.creature_grid.query(creature.x, creature.y, config.contact_range);

        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive { continue; }
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

    // ========== 更新/清理 ==========

    fn update_energy_particles(&mut self, dt: f64, config: &Config) {
        for particle in &mut self.energy_particles {
            particle.update(dt, config.particle_decay_rate);
        }
    }

    fn cleanup(&mut self) {
        let mut had_deaths = false;

        for creature in &self.creatures {
            if !creature.alive {
                had_deaths = true;
                let age = creature.age;
                let pos = self.death_ages.partition_point(|&x| x < age);
                self.death_ages.insert(pos, age);
                self.death_age_sum += age;
            }
        }

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

        self.creatures.retain(|c| c.alive);
        self.energy_particles.retain(|e| e.alive);

        // 每10秒清理一次相似度缓存
        if self.time - self.cache_cleanup_timer >= 10.0 {
            self.cache_cleanup_timer = self.time;
            let alive_hashes: rustc_hash::FxHashSet<u64> = self.creatures.iter()
                .filter(|c| c.alive)
                .map(|c| c.genome_hash)
                .collect();
            let mut cache = self.similarity_cache.borrow_mut();
            cache.retain(|(h1, h2), _| {
                alive_hashes.contains(h1) && alive_hashes.contains(h2)
            });
        }
    }

    // ========== 统计 ==========

    pub fn stats(&self, species_threshold: f64, config: &Config) -> WorldStats {
        let alive_creatures: Vec<_> = self.creatures.iter().filter(|c| c.alive).collect();
        let max_generation = alive_creatures.iter().map(|c| c.generation).max().unwrap_or(0);

        let creature_energy: f64 = alive_creatures.iter().map(|c| c.energy).sum();
        let particle_energy: f64 = self.energy_particles.iter()
            .filter(|e| e.alive).map(|e| e.energy).sum();
        let total_energy = creature_energy + particle_energy;
        let avg_energy = if alive_creatures.is_empty() { 0.0 }
            else { creature_energy / alive_creatures.len() as f64 };

        // 种族聚类（祖先追溯模型）
        self.ensure_clan_cache(species_threshold);
        let cache = self.clan_cache.borrow();
        let cache = cache.as_ref().unwrap();
        let species_count = cache.species_count;
        let top_species = cache.top_species.clone();
        let creature_clan_map = cache.creature_clan_map.clone();

        // 构建生物ID -> 族长ID映射
        let mut id_species_map: FxHashMap<u64, u64> = FxHashMap::default();
        for (i, creature) in alive_creatures.iter().enumerate() {
            if let Some(&leader_id) = creature_clan_map.get(&i) {
                id_species_map.insert(creature.id, leader_id);
            }
        }

        // 优势种检测
        let dominant_candidate = self.detect_dominant(
            &alive_creatures,
            &creature_clan_map,
            avg_energy,
            max_generation,
            config,
        );

        WorldStats {
            time: self.time,
            creature_count: alive_creatures.len(),
            energy_particle_count: self.energy_particles.iter().filter(|e| e.alive).count(),
            total_energy,
            max_generation,
            avg_energy,
            action_counts: self.action_counts,
            death_age_stats: self.death_age_stats.clone(),
            species_count,
            top_species,
            creature_species_map: id_species_map,
            dominant_candidate,
        }
    }

    /// 获取渲染上下文数据（族长 genome_hash 作为颜色标识）
    pub fn get_render_data(&self, threshold: f64) -> FxHashMap<usize, u64> {
        self.ensure_clan_cache(threshold);
        let cache_ref = self.clan_cache.borrow();
        let cache = match cache_ref.as_ref() {
            Some(c) => c,
            None => return FxHashMap::default(),
        };

        let mut creature_species: FxHashMap<usize, u64> = FxHashMap::with_capacity_and_hasher(
            cache.alive_indices.len(), Default::default(),
        );
        for (i, &original_idx) in cache.alive_indices.iter().enumerate() {
            if let Some(&leader_id) = cache.creature_clan_map.get(&i) {
                if let Some(&color_hash) = cache.clan_color.get(&leader_id) {
                    creature_species.insert(original_idx, color_hash);
                }
            }
        }

        creature_species
    }

    /// 优势种检测
    fn detect_dominant(
        &self,
        alive_creatures: &[&Creature],
        creature_clan_map: &FxHashMap<usize, u64>,
        global_avg_energy: f64,
        global_max_generation: usize,
        config: &Config,
    ) -> Option<DominantCandidate> {
        let total_count = alive_creatures.len();
        if total_count < 5 { return None; }

        let death_median = self.death_age_stats.median;
        let has_death_data = self.death_age_stats.count > 0;

        let mut species_members: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
        for (i, _) in alive_creatures.iter().enumerate() {
            if let Some(&leader_id) = creature_clan_map.get(&i) {
                species_members.entry(leader_id).or_default().push(i);
            }
        }

        let mut best: Option<DominantCandidate> = None;

        for (_species_root, members) in &species_members {
            let count = members.len();
            if count < 5 { continue; }

            let ratio = count as f64 / total_count as f64;
            if ratio < 0.3 { continue; }

            let mut max_age: f64 = 0.0;
            let mut sum_age: f64 = 0.0;
            let mut sum_energy: f64 = 0.0;
            let mut sp_max_gen: usize = 0;
            let mut best_energy_idx: usize = members[0];
            let mut best_energy_val: f64 = f64::NEG_INFINITY;

            for &idx in members {
                let c = alive_creatures[idx];
                if c.age > max_age { max_age = c.age; }
                sum_age += c.age;
                sum_energy += c.energy;
                if c.generation > sp_max_gen { sp_max_gen = c.generation; }
                if c.energy > best_energy_val {
                    best_energy_val = c.energy;
                    best_energy_idx = idx;
                }
            }

            let sp_avg_age = sum_age / count as f64;
            let sp_avg_energy = sum_energy / count as f64;

            if max_age < config.dominant_min_age { continue; }
            if has_death_data && sp_avg_age < death_median { continue; }

            let ratio_score = ratio;
            let energy_score = if global_avg_energy > 0.0 {
                (sp_avg_energy / global_avg_energy).min(2.0) / 2.0
            } else { 0.0 };
            let age_score = if has_death_data && death_median > 0.0 {
                (sp_avg_age / death_median).min(2.0) / 2.0
            } else { 0.5 };
            let gen_score = if global_max_generation > 0 {
                sp_max_gen as f64 / global_max_generation as f64
            } else { 0.0 };

            let score = 0.30 * ratio_score + 0.20 * energy_score + 0.30 * age_score + 0.20 * gen_score;
            if score < 0.6 { continue; }

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

    // ========== 聚类缓存 ==========

    fn ensure_clan_cache(&self, threshold: f64) {
        let current_time = self.time;
        if *self.clan_cache_time.borrow() == current_time && self.clan_cache.borrow().is_some() {
            return;
        }
        let alive_creatures: Vec<&Creature> = self.creatures.iter().filter(|c| c.alive).collect();
        let cache = self.calculate_clan_cache(&alive_creatures, threshold);
        *self.clan_cache.borrow_mut() = Some(cache);
        *self.clan_cache_time.borrow_mut() = current_time;
    }

    /// 祖先追溯种族计算：
    /// 对每个活生物，沿 parent_id 向上追溯到最老的活祖先（族长）,
    /// 然后检查与族长的基因相似度。相似度 >= 阈值则属同族，否则自立门户。
    fn calculate_clan_cache(&self, alive_creatures: &[&Creature], threshold: f64) -> ClanCache {
        let n = alive_creatures.len();
        if n == 0 {
            return ClanCache {
                alive_indices: Vec::new(),
                species_count: 0,
                top_species: Vec::new(),
                creature_clan_map: FxHashMap::default(),
                clan_color: FxHashMap::default(),
            };
        }

        // 建立 creature ID -> 局部索引映射
        let mut id_to_local: FxHashMap<u64, usize> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, c) in alive_creatures.iter().enumerate() {
            id_to_local.insert(c.id, i);
        }

        // 为每个生物找到族长
        let mut creature_clan_map: FxHashMap<usize, u64> = FxHashMap::with_capacity_and_hasher(n, Default::default());
        for i in 0..n {
            let leader_id = self.find_clan_leader(i, alive_creatures, &id_to_local, threshold);
            creature_clan_map.insert(i, leader_id);
        }

        // 构建族长颜色映射和统计
        let mut clan_color: FxHashMap<u64, u64> = FxHashMap::default();
        let mut clan_counts: FxHashMap<u64, usize> = FxHashMap::default();

        for i in 0..n {
            let leader_id = creature_clan_map[&i];
            *clan_counts.entry(leader_id).or_insert(0) += 1;
            clan_color.entry(leader_id).or_insert_with(|| {
                id_to_local.get(&leader_id)
                    .map(|&local| alive_creatures[local].genome_hash)
                    .unwrap_or(0)
            });
        }

        let species_count = clan_counts.len();

        let mut species_vec: Vec<_> = clan_counts.into_iter()
            .map(|(leader_id, count)| RankedEntry { id: leader_id as usize, count })
            .collect();
        species_vec.sort_by(|a, b| b.count.cmp(&a.count));
        let top_species: Vec<_> = species_vec.into_iter().take(3).collect();

        let alive_indices: Vec<usize> = (0..self.creatures.len())
            .filter(|&i| self.creatures[i].alive)
            .collect();

        ClanCache {
            alive_indices,
            species_count,
            top_species,
            creature_clan_map,
            clan_color,
        }
    }

    /// 沿 parent_id 向上追溯，找到最老的活祖先，
    /// 检查相似度决定是否归属该祖先的族群
    fn find_clan_leader(
        &self,
        local_idx: usize,
        alive_creatures: &[&Creature],
        id_to_local: &FxHashMap<u64, usize>,
        threshold: f64,
    ) -> u64 {
        let creature = alive_creatures[local_idx];

        // 向上走，找最老的活祖先
        let mut ancestor_local = local_idx;
        let mut depth = 0;
        loop {
            if depth > 1000 { break; } // 安全上限
            match alive_creatures[ancestor_local].parent_id {
                Some(parent_id) => {
                    if let Some(&parent_local) = id_to_local.get(&parent_id) {
                        ancestor_local = parent_local;
                        depth += 1;
                    } else {
                        break; // 父代已死
                    }
                }
                None => break, // 无父代（自然生成）
            }
        }

        if ancestor_local == local_idx {
            // 自己就是最老活祖先 → 自立门户
            creature.id
        } else {
            // 检查与最老活祖先的相似度
            let similarity = self.get_similarity(creature, alive_creatures[ancestor_local]);
            if similarity >= threshold {
                alive_creatures[ancestor_local].id
            } else {
                creature.id // 变异过大 → 自立门户
            }
        }
    }
}

// ========== 数据结构 ==========

#[derive(Default, Clone)]
pub struct DeathAgeStats {
    pub count: usize,
    pub avg: f64,
    pub median: f64,
    pub max: f64,
    pub min: f64,
}

#[derive(Clone, Default)]
pub struct RankedEntry {
    pub id: usize,
    pub count: usize,
}

pub struct WorldStats {
    pub time: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub total_energy: f64,
    pub max_generation: usize,
    pub avg_energy: f64,
    pub action_counts: [usize; 5],
    pub death_age_stats: DeathAgeStats,
    pub species_count: usize,
    pub top_species: Vec<RankedEntry>,
    pub creature_species_map: FxHashMap<u64, u64>,
    pub dominant_candidate: Option<DominantCandidate>,
}

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
