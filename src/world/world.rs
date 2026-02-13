use rand::Rng;
use rustc_hash::FxHashMap;

use crate::config::Config;
use super::{Creature, EnergyParticle, SpatialGrid};

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
        }
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

        // 重建空间索引
        self.rebuild_spatial_index();

        // 更新生物
        self.update_creatures(dt, config);

        // 更新能量粒子
        self.update_energy_particles(dt);

        // 清理死亡实体
        self.cleanup();
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

        for i in 0..creature_count {
            if !self.creatures[i].alive {
                continue;
            }

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
            let perception = self.perceive(i, config);

            // 神经网络决策
            let outputs = self.creatures[i].brain.forward(&perception);

            // 执行动作
            self.execute_actions(i, &outputs, dt, config);
        }
    }

    /// 感知环境 (17维输入)
    /// 0-7: 8方向能量感知
    /// 8-15: 8方向邻居相似度 (0=无邻居, >0=有邻居且为相似度)
    /// 16: 自身能量
    fn perceive(&self, creature_idx: usize, config: &Config) -> Vec<f64> {
        let creature = &self.creatures[creature_idx];
        let mut input = vec![0.0; 17];

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
                        dir_similarity += creature.similarity(other);
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
    }

    /// 获取统计信息
    pub fn stats(&self) -> WorldStats {
        let alive_families = self.family_stats.len();
        let largest_family = self.family_stats.values().max().copied().unwrap_or(0);
        let max_generation = self.creatures.iter()
            .filter(|c| c.alive)
            .map(|c| c.generation)
            .max()
            .unwrap_or(0);

        WorldStats {
            time: self.time,
            creature_count: self.creatures.len(),
            energy_particle_count: self.energy_particles.len(),
            alive_families,
            extinct_families: self.extinct_families,
            largest_family,
            max_generation,
        }
    }
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
}
