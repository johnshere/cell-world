use rand::Rng;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::time::Instant;

use crate::config::Config;
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
}

impl World {
    pub fn new(config: &Config) -> Self {
        Self {
            creatures: Vec::new(),
            energy_particles: Vec::new(),
            creature_grid: SpatialGrid::new(config.sense_range),
            energy_grid: SpatialGrid::new(config.sense_range),
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

        // 自动补充生物
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

    /// 生成能量粒子（只在视窗范围内生成）
    fn spawn_energy(&mut self, dt: f64, config: &Config) {
        self.energy_spawn_timer += dt;

        if self.energy_spawn_timer >= config.energy_spawn_interval {
            self.energy_spawn_timer = 0.0;

            let mut rng = rand::thread_rng();
            for _ in 0..config.energy_spawn_count {
                // 在当前视窗范围内生成能量粒子
                let x = rng.gen_range(self.viewport_min_x..self.viewport_max_x);
                let y = rng.gen_range(self.viewport_min_y..self.viewport_max_y);
                let energy_id = self.next_energy_id;
                self.next_energy_id += 1;
                let particle = EnergyParticle::new(
                    energy_id,
                    x,
                    y,
                    config.energy_particle_value,
                    config.energy_particle_lifetime,
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

            // 收集感知数据
            let t0 = Instant::now();
            let perception = self.perceive(i, config);
            perceive_time += t0.elapsed().as_secs_f64() * 1000.0;

            // 神经网络决策
            let t1 = Instant::now();
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

    /// 感知环境 (17维输入)
    /// 0-7: 8方向能量感知
    /// 8-15: 8方向邻居相似度 (0=无邻居, >0=有邻居且为相似度)
    /// 16: 自身能量
    fn perceive(&self, creature_idx: usize, config: &Config) -> [f64; 17] {
        let creature = &self.creatures[creature_idx];
        let mut input = [0.0; 17];  // 栈分配，避免堆分配开销

        // 8方向
        let directions: [(f64, f64); 8] = [
            (0.0, -1.0),     // N
            (0.707, -0.707), // NE
            (1.0, 0.0),      // E
            (0.707, 0.707),  // SE
            (0.0, 1.0),      // S
            (-0.707, 0.707), // SW
            (-1.0, 0.0),     // W
            (-0.707, -0.707), // NW
        ];

        // 查询附近实体
        let nearby_creatures = self.creature_grid.query(creature.x, creature.y, config.sense_range);
        let nearby_energy = self.energy_grid.query(creature.x, creature.y, config.sense_range);

        for (dir_idx, (dx, dy)) in directions.iter().enumerate() {
            let mut dir_energy = 0.0;
            let mut dir_similarity = 0.0;
            let mut neighbor_count = 0;

            // 检查邻居
            for &idx in &nearby_creatures {
                if idx == creature_idx {
                    continue;
                }
                let other = &self.creatures[idx];
                if !other.alive {
                    continue;
                }

                let ox = other.x - creature.x;
                let oy = other.y - creature.y;
                let dist = (ox * ox + oy * oy).sqrt();

                if dist > 0.0 && dist < config.sense_range {
                    // 检查是否在这个方向
                    let dot = (ox * dx + oy * dy) / dist;
                    if dot > 0.5 {
                        dir_similarity += self.get_similarity(creature, other);
                        neighbor_count += 1;
                    }
                }
            }

            // 检查能量粒子
            for &idx in &nearby_energy {
                let particle = &self.energy_particles[idx];
                if !particle.alive {
                    continue;
                }

                let px = particle.x - creature.x;
                let py = particle.y - creature.y;
                let dist = (px * px + py * py).sqrt();

                if dist > 0.0 && dist < config.sense_range {
                    let dot = (px * dx + py * dy) / dist;
                    if dot > 0.5 {
                        dir_energy += particle.energy;
                    }
                }
            }

            // 归一化
            // 0-7: 能量感知
            input[dir_idx] = (dir_energy / 200.0).min(1.0);
            // 8-15: 邻居相似度 (0=无邻居, >0=平均相似度)
            input[8 + dir_idx] = if neighbor_count > 0 {
                dir_similarity / neighbor_count as f64
            } else {
                0.0
            };
        }

        // 16: 自身能量
        input[16] = (creature.energy / 200.0).min(1.0);

        input
    }

    /// 执行动作
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        let output_map = self.creatures[creature_idx].genome.output_map.clone();

        for (out_idx, &func_id) in output_map.iter().enumerate() {
            if out_idx >= outputs.len() {
                break;
            }
            let value = outputs[out_idx];

            match func_id {
                0 => self.action_move_x(creature_idx, value, dt, config),
                1 => self.action_move_y(creature_idx, value, dt, config),
                2 => self.action_absorb(creature_idx, value, config),
                3 => self.action_release(creature_idx, value, config),
                4 => self.action_reproduce(creature_idx, value, config),
                5 => self.action_transfer(creature_idx, value, config),
                _ => {}
            }
        }
    }

    // 功能 0: 移动 X（无边界限制）
    fn action_move_x(&mut self, idx: usize, value: f64, dt: f64, config: &Config) {
        let dx = value * dt * 50.0;
        self.creatures[idx].x += dx;
        // 世界无限大，不限制移动范围
        self.creatures[idx].energy -= dx.abs() * config.move_cost;
    }

    // 功能 1: 移动 Y（无边界限制）
    fn action_move_y(&mut self, idx: usize, value: f64, dt: f64, config: &Config) {
        let dy = value * dt * 50.0;
        self.creatures[idx].y += dy;
        // 世界无限大，不限制移动范围
        self.creatures[idx].energy -= dy.abs() * config.move_cost;
    }

    // 功能 2: 吸收（自动触发，接触即吸收）
    fn action_absorb(&mut self, idx: usize, _value: f64, config: &Config) {
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
                    break; // 一次只吸收一个
                }
            }
        }
    }

    // 功能 3: 释放
    fn action_release(&mut self, idx: usize, value: f64, config: &Config) {
        if value <= 0.0 {
            return;
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
        }
    }

    // 功能 4: 繁殖
    fn action_reproduce(&mut self, idx: usize, value: f64, config: &Config) {
        // 降低阈值使繁殖更容易触发
        if value <= 0.2 {
            return;
        }

        // 检查是否超过最大生物数量
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();
        if alive_count >= config.max_creatures {
            return;
        }

        if self.creatures[idx].energy < config.reproduce_threshold {
            return;
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
    }

    // 功能 5: 能量转移
    fn action_transfer(&mut self, idx: usize, value: f64, config: &Config) {
        if value.abs() < 0.1 {
            return;
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
                // 正值 = 掠夺，负值 = 给予
                let transfer_amount = value * 5.0;

                if transfer_amount > 0.0 {
                    // 掠夺
                    let actual = transfer_amount.min(self.creatures[other_idx].energy);
                    self.creatures[other_idx].energy -= actual;
                    self.creatures[idx].energy += actual;
                } else {
                    // 给予
                    let actual = (-transfer_amount).min(self.creatures[idx].energy);
                    self.creatures[idx].energy -= actual;
                    self.creatures[other_idx].energy += actual;
                }
                break; // 一次只与一个交互
            }
        }
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
        let largest_family = self.family_stats.values().max().copied().unwrap_or(0);
        let max_generation = alive_creatures.iter()
            .map(|c| c.generation)
            .max()
            .unwrap_or(0);
        let avg_energy = if alive_creatures.is_empty() {
            0.0
        } else {
            alive_creatures.iter().map(|c| c.energy).sum::<f64>() / alive_creatures.len() as f64
        };

        // 统计解锁高级功能的生物数
        let transfer_unlocked = alive_creatures.iter()
            .filter(|c| c.genome.output_map.contains(&5))
            .count();
        let release_unlocked = alive_creatures.iter()
            .filter(|c| c.genome.output_map.contains(&3))
            .count();

        // 计算种族分组（使用并查集思想）
        let (species_count, largest_species, top_species, creature_species_map) =
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

        WorldStats {
            time: self.time,
            creature_count: alive_creatures.len(),
            energy_particle_count: self.energy_particles.len(),
            alive_families,
            extinct_families: self.extinct_families,
            largest_family,
            max_generation,
            avg_energy,
            transfer_unlocked,
            release_unlocked,
            species_count,
            largest_species,
            top_families,
            top_species,
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
    fn calculate_species(&self, alive_creatures: &[&Creature], threshold: f64) -> (usize, usize, Vec<RankedEntry>, FxHashMap<usize, usize>) {
        if alive_creatures.is_empty() {
            return (0, 0, Vec::new(), FxHashMap::default());
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
        let largest_species = species_counts.values().max().copied().unwrap_or(0);

        // 计算种族前三，找出每个种族的主导家族
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

        (species_count, largest_species, top_species, creature_species_map)
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
    pub alive_families: usize,
    pub extinct_families: usize,
    pub largest_family: usize,
    pub max_generation: usize,
    pub avg_energy: f64,
    // 行为统计
    pub transfer_unlocked: usize,  // 解锁转移功能的生物数
    pub release_unlocked: usize,   // 解锁释放功能的生物数
    // 种族统计
    pub species_count: usize,      // 种族数量
    pub largest_species: usize,    // 最大种族数量
    pub top_families: Vec<RankedEntry>,   // 前三家族
    pub top_species: Vec<RankedEntry>,    // 前三种族
}
