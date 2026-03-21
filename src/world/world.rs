use rand::Rng;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::time::Instant;

use super::{Creature, EnergyParticle, ParticleSource, SpatialGrid, TrailPoint};
use crate::config::Config;
use crate::neural::bridge::{CreatureEvent, CreatureInput, NeuralBridge};
use crate::neural::Genome;

/// 性能统计（单帧）
#[derive(Default, Clone)]
pub struct PerfStats {
    pub perceive_ms: f64,
    pub snn_ms: f64,
    pub actions_ms: f64,
    pub spatial_ms: f64,
    pub total_ms: f64,
    pub creature_count: usize,
    pub avg_compute_ns: f64,
}

/// 世界
pub struct World {
    pub creatures: Vec<Creature>,
    pub energy_particles: Vec<EnergyParticle>,
    pub trail_points: Vec<TrailPoint>,

    // 空间索引
    creature_grid: SpatialGrid,
    energy_grid: SpatialGrid,
    trail_grid: SpatialGrid,

    // 统计
    pub time: f64,

    // 内部状态
    volcano_timer: f64,
    meteorite_timer: f64,

    // ID 计数器
    next_creature_id: u64,
    next_energy_id: u64,

    // 视窗范围（世界坐标，动态跟随）
    viewport_min_x: f64,
    viewport_min_y: f64,
    viewport_max_x: f64,
    viewport_max_y: f64,

    // 火山范围检查定时器
    volcano_range_check_timer: f64,

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
    trail_query_buf: Vec<usize>,

    /// 痕迹系统完全禁用（手动开关，屏蔽所有痕迹逻辑）
    pub trail_disabled: bool,
    /// 痕迹生成暂停（FPS<30时自动开启，仅停止生成新痕迹）
    pub trail_spawn_paused: bool,

    /// 异步神经桥（None = 同步模式）
    neural_bridge: Option<NeuralBridge>,
    /// 异步模式下的输出缓存
    neural_output_cache: FxHashMap<u64, [f64; 6]>,
    /// 异步 SNN 耗时缓存
    neural_compute_cache: FxHashMap<u64, u64>,

    /// 种族源头基因组：clan_hash -> 建族者的 genome（用于后代相似度比较）
    clan_genomes: FxHashMap<u64, Genome>,

    /// 火山喷发半径（动态根据可视空间计算）
    pub volcano_radius: f64,

    /// 优势种库
    pub dominant_species: Vec<DominantCandidate>,
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
            trail_points: Vec::new(),
            creature_grid: SpatialGrid::new(config.vision_range * 1.5),
            energy_grid: SpatialGrid::new(config.vision_range * 1.5),
            trail_grid: SpatialGrid::new(config.vision_range * 1.5),
            time: 0.0,
            volcano_timer: 0.0,
            meteorite_timer: 0.0,
            next_creature_id: 0,
            next_energy_id: 0,
            // 初始视窗居中于原点
            viewport_min_x: -700.0,
            viewport_min_y: -500.0,
            viewport_max_x: 700.0,
            viewport_max_y: 500.0,

            volcano_range_check_timer: 0.0,
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
            trail_query_buf: Vec::new(),
            trail_disabled: false,
            trail_spawn_paused: false,
            neural_bridge: None,
            neural_output_cache: FxHashMap::default(),
            neural_compute_cache: FxHashMap::default(),
            clan_genomes: FxHashMap::default(),
            volcano_radius: config.volcano_radius,
            dominant_species: Vec::new(),
        };
        // 初始连喷三波，提供充足起始能量（直接落地，不杀伤）
        for _ in 0..3 {
            world.volcano_erupt(config);
        }
        for _ in 0..config.min_creatures {
            world.spawn_creature(config);
        }
        world
    }

    /// 设置异步神经桥
    pub fn set_neural_bridge(&mut self, bridge: NeuralBridge) {
        // 向桥注册所有已有生物
        for creature in &self.creatures {
            if creature.alive {
                bridge.send_event(CreatureEvent::Born {
                    id: creature.id,
                    genome: creature.genome.clone(),
                });
            }
        }
        self.neural_bridge = Some(bridge);
    }

    /// 向桥发送 Born 事件
    fn notify_born(&self, creature: &Creature) {
        if let Some(ref bridge) = self.neural_bridge {
            bridge.send_event(CreatureEvent::Born {
                id: creature.id,
                genome: creature.genome.clone(),
            });
        }
    }

    /// 向桥发送 Died 事件
    fn notify_died(&self, creature_id: u64) {
        if let Some(ref bridge) = self.neural_bridge {
            bridge.send_event(CreatureEvent::Died { id: creature_id });
        }
    }

    /// 获取缓存的相似度
    fn get_similarity(&self, creature_a: &Creature, creature_b: &Creature) -> f64 {
        let hash_a = creature_a.genome_hash;
        let hash_b = creature_b.genome_hash;
        let key = if hash_a <= hash_b {
            (hash_a, hash_b)
        } else {
            (hash_b, hash_a)
        };
        let mut cache = self.similarity_cache.borrow_mut();
        *cache
            .entry(key)
            .or_insert_with(|| creature_a.genome.similarity(&creature_b.genome))
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

        // 每5秒检查一次火山半径外的生物（动态读取配置）
        self.volcano_range_check_timer += dt;
        if self.volcano_range_check_timer >= 5.0 {
            self.volcano_range_check_timer -= 5.0;

            let volcano_x = config.volcano_x;
            let volcano_y = config.volcano_y;
            let limit_r = config.volcano_radius * 1.5;
            let limit_r2 = limit_r * limit_r;

            for creature in self.creatures.iter_mut() {
                if creature.alive {
                    let dx = creature.x - volcano_x;
                    let dy = creature.y - volcano_y;
                    if dx * dx + dy * dy > limit_r2 {
                        creature.alive = false;
                    }
                }
            }
        }

        // 更新能量粒子
        self.update_energy_particles(dt, config);

        // 更新痕迹点
        if !self.trail_disabled {
            self.update_trail_points(dt, config);
        }

        // 清理
        self.cleanup();
    }

    /// 火山喷发倒计时
    pub fn volcano_countdown(&self, config: &Config) -> f64 {
        (config.current_volcano_interval(self.time) - self.volcano_timer).max(0.0)
    }

    /// 火山计时器当前值
    pub fn volcano_timer(&self) -> f64 {
        self.volcano_timer
    }

    /// 陨石计时器当前值
    pub fn meteorite_timer(&self) -> f64 {
        self.meteorite_timer
    }

    // ========== 生成 ==========

    fn replenish_creatures(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        loop {
            let alive_count = self.creatures.iter().filter(|c| c.alive).count();
            if alive_count >= config.min_creatures {
                break;
            }
            if !self.dominant_species.is_empty() && rng.gen_bool(0.5) {
                // 50% 从优势种库取一个
                let idx = rng.gen_range(0..self.dominant_species.len());
                let candidate = self.dominant_species[idx].clone();
                self.spawn_from_template(config, &candidate.genome, config.initial_energy);
            } else {
                // 50% 随机生成
                self.spawn_creature(config);
            }
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
        // 自然生成：以自身基因建族
        self.clan_genomes
            .insert(creature.clan_hash, creature.genome.clone());
        self.notify_born(&creature);
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
        self.notify_born(&creature);
        self.creatures.push(creature);
    }

    /// 杀死指定生物
    pub fn kill_creature(&mut self, id: u64) {
        if let Some(creature) = self.creatures.iter_mut().find(|c| c.id == id) {
            creature.alive = false;
            self.notify_died(id);
        }
    }

    /// 生成能量粒子（火山喷发 + 随机陨石）
    fn spawn_energy(&mut self, dt: f64, config: &Config) {
        self.volcano_timer += dt;
        let current_volcano_interval = config.current_volcano_interval(self.time);
        if self.volcano_timer >= current_volcano_interval {
            self.volcano_timer = 0.0;
            self.volcano_erupt(config);
        }

        self.meteorite_timer += dt;
        let current_meteorite_interval = config.current_meteorite_interval(self.time);
        if self.meteorite_timer >= current_meteorite_interval {
            self.meteorite_timer = 0.0;
            self.meteorite_fall(config);
        }
    }

    fn volcano_erupt(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let current_energy = config.current_volcano_energy(self.time);
        let kill_r2 = config.volcano_kill_radius * config.volcano_kill_radius;
        for _ in 0..config.volcano_count {
            let angle = rng.gen_range(0.0..std::f64::consts::TAU);
            // 中心富集：u^1.5 分布，比 u² 稍平缓，远处粒子更多
            let u: f64 = rng.gen_range(0.0..1.0);
            let r = u.powf(1.5) * self.volcano_radius;
            let x = config.volcano_x + r * angle.cos();
            let y = config.volcano_y + r * angle.sin();
            let energy_id = self.next_energy_id;
            self.next_energy_id += 1;
            self.energy_particles.push(EnergyParticle::new(
                energy_id,
                x,
                y,
                current_energy,
                f64::MAX,
                ParticleSource::Volcano,
            ));
            // 即时杀伤
            for c in &mut self.creatures {
                if c.alive {
                    let dx = c.x - x;
                    let dy = c.y - y;
                    if dx * dx + dy * dy < kill_r2 {
                        c.alive = false;
                    }
                }
            }
        }
    }

    fn meteorite_fall(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let current_energy = config.current_meteorite_energy(self.time);
        // 陨石中心在火山半径内随机
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let u: f64 = rng.gen_range(0.0..1.0);
        let r = u.sqrt() * config.volcano_radius;
        let cx = config.volcano_x + r * angle.cos();
        let cy = config.volcano_y + r * angle.sin();
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let dx = angle.cos();
        let dy = angle.sin();
        let half_len = config.meteorite_length / 2.0;
        let kill_r2 = config.meteorite_kill_radius * config.meteorite_kill_radius;

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
                energy_id,
                x,
                y,
                current_energy,
                f64::MAX,
                ParticleSource::Meteorite,
            ));
            // 即时杀伤
            for c in &mut self.creatures {
                if c.alive {
                    let cdx = c.x - x;
                    let cdy = c.y - y;
                    if cdx * cdx + cdy * cdy < kill_r2 {
                        c.alive = false;
                    }
                }
            }
        }
    }

    // ========== 空间索引 ==========

    fn rebuild_spatial_index(&mut self) {
        self.creature_grid.clear();
        self.energy_grid.clear();
        if !self.trail_disabled {
            self.trail_grid.clear();
        }

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
        if !self.trail_disabled {
            for (i, t) in self.trail_points.iter().enumerate() {
                if t.alive {
                    self.trail_grid.insert(i, t.x, t.y);
                }
            }
        }
    }

    // ========== 周围能量（粒子+生物） ==========

    /// 查询 vision_range 内所有粒子能量 + 生物能量
    fn compute_nearby_energy(&self, x: f64, y: f64, config: &Config) -> f64 {
        let range = config.vision_range;
        let mut total = 0.0;
        // 粒子
        for &idx in &self.energy_grid.query(x, y, range) {
            if self.energy_particles[idx].alive {
                total += self.energy_particles[idx].energy;
            }
        }
        // 生物
        for &idx in &self.creature_grid.query(x, y, range) {
            if self.creatures[idx].alive {
                total += self.creatures[idx].energy;
            }
        }
        total
    }

    // ========== 更新生物 ==========

    fn update_creatures(&mut self, dt: f64, config: &Config) {
        let has_bridge = self.neural_bridge.is_some();

        // 异步模式：交换缓冲区，读取输出
        if has_bridge {
            if let Some(ref bridge) = self.neural_bridge {
                bridge.swap_outputs();
                let outputs = bridge.read_outputs();
                for o in &outputs {
                    self.neural_output_cache.insert(o.creature_id, o.outputs);
                    self.neural_compute_cache
                        .insert(o.creature_id, o.compute_ns);
                }
            }
        }

        let creature_count = self.creatures.len();
        let mut alive_count = 0;
        let mut perceive_time = 0.0;
        let mut forward_time = 0.0;
        let mut actions_time = 0.0;

        let total_start = Instant::now();

        // 异步模式：收集感知数据
        let mut bridge_inputs: Vec<CreatureInput> = if has_bridge {
            Vec::with_capacity(creature_count)
        } else {
            Vec::new()
        };

        for i in 0..creature_count {
            if !self.creatures[i].alive {
                continue;
            }
            alive_count += 1;

            let creature_t0 = Instant::now();

            // 周围能量（粒子+生物）
            let nearby_energy =
                self.compute_nearby_energy(self.creatures[i].x, self.creatures[i].y, config);

            // 基础代谢
            let age_multiplier = 1.0 + self.creatures[i].age * config.age_metabolism_factor;
            let metabolism_cost = config.base_metabolism * age_multiplier * dt;
            self.creatures[i].energy -= metabolism_cost;

            // 冷却递减
            self.creatures[i].eye_cooldown_timer -= dt;
            self.creatures[i].mouth_cooldown_timer -= dt;
            self.creatures[i].reproduce_cooldown_timer -= dt;

            // 体温逸散：指数衰减 + floor
            // heat_factor = heat_floor + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)
            let body_radius = (self.creatures[i].energy.max(0.0) * 1.28).cbrt();
            let circumference = body_radius * std::f64::consts::TAU;
            let heat_factor = config.heat_floor
                + (1.0 - config.heat_floor) * (-nearby_energy / config.energy_denominator).exp();
            let heat_cost = config.heat_dissipation_coefficient * circumference * heat_factor * dt;
            self.creatures[i].energy -= heat_cost;

            self.creatures[i].age += dt;

            if self.creatures[i].energy <= 0.0 {
                self.creatures[i].alive = false;
                continue;
            }

            // 感知（带冷却的按需扫描）
            let t0 = Instant::now();
            self.compute_perception_with_cooldown(i, config);
            perceive_time += t0.elapsed().as_secs_f64() * 1000.0;

            if has_bridge {
                // 异步模式：发布感知 → 读缓存输出
                bridge_inputs.push(CreatureInput {
                    creature_id: self.creatures[i].id,
                    perception: self.creatures[i].perception_cache,
                });
                let outputs = self
                    .neural_output_cache
                    .get(&self.creatures[i].id)
                    .copied()
                    .unwrap_or(self.creatures[i].last_outputs);
                self.creatures[i].last_outputs = outputs;

                let t2 = Instant::now();
                self.execute_actions(i, &outputs.to_vec(), dt, config);
                actions_time += t2.elapsed().as_secs_f64() * 1000.0;

                // 主线程耗时 + 异步 SNN 耗时
                let main_ns = creature_t0.elapsed().as_nanos() as u64;
                let snn_ns = self
                    .neural_compute_cache
                    .get(&self.creatures[i].id)
                    .copied()
                    .unwrap_or(0);
                self.creatures[i].frame_compute_ns = main_ns + snn_ns;
            } else {
                // 同步模式：SNN tick（首次注入输入，后续 tick_free，脉冲输出用发放率）
                let t1 = Instant::now();
                let perception = self.creatures[i].perception_cache;
                let snn_ticks = config.snn_ticks_per_frame;
                let outputs = self.creatures[i].brain.tick_multi(&perception, snn_ticks);
                for (j, &v) in outputs.iter().enumerate().take(6) {
                    self.creatures[i].last_outputs[j] = v;
                }
                forward_time += t1.elapsed().as_secs_f64() * 1000.0;

                let t2 = Instant::now();
                self.execute_actions(i, &outputs, dt, config);
                actions_time += t2.elapsed().as_secs_f64() * 1000.0;

                // 同步模式下 SNN 已在 main_ns 中
                let main_ns = creature_t0.elapsed().as_nanos() as u64;
                self.creatures[i].frame_compute_ns = main_ns;
            }
        }

        // 异步模式：写入感知并交换
        if has_bridge {
            if let Some(ref bridge) = self.neural_bridge {
                bridge.write_inputs(bridge_inputs);
                bridge.swap_inputs();
            }
        }

        // 算力能量扣除（在计时区间外，避免递归膨胀）
        if config.compute_energy_factor > 0.0 {
            let mut died_ids = Vec::new();
            for i in 0..self.creatures.len() {
                if !self.creatures[i].alive {
                    continue;
                }
                let cost = (self.creatures[i].frame_compute_ns as f64 / 100_000_000.0)
                    * config.compute_energy_factor;
                self.creatures[i].energy -= cost;
                if self.creatures[i].energy <= 0.0 {
                    self.creatures[i].alive = false;
                    died_ids.push(self.creatures[i].id);
                }
            }
            for id in died_ids {
                if let Some(ref bridge) = self.neural_bridge {
                    bridge.send_event(CreatureEvent::Died { id });
                }
            }
        }

        let total_time = total_start.elapsed().as_secs_f64() * 1000.0;
        self.perf_stats.perceive_ms = perceive_time;
        self.perf_stats.snn_ms = forward_time;
        self.perf_stats.actions_ms = actions_time;
        self.perf_stats.total_ms = total_time;
        self.perf_stats.creature_count = alive_count;

        // 平均算力统计
        if alive_count > 0 {
            let total_ns: u64 = self
                .creatures
                .iter()
                .filter(|c| c.alive)
                .map(|c| c.frame_compute_ns)
                .sum();
            self.perf_stats.avg_compute_ns = total_ns as f64 / alive_count as f64;
        } else {
            self.perf_stats.avg_compute_ns = 0.0;
        }
    }

    // ========== 感知系统（双眼，带冷却） ==========

    /// 带冷却的感知：仅在冷却结束时触发扫描，否则保持缓存
    fn compute_perception_with_cooldown(&mut self, creature_idx: usize, config: &Config) {
        let eyes_ready = self.creatures[creature_idx].eye_cooldown_timer <= 0.0;

        // 自身状态 [8..9] 始终更新
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;
        self.creatures[creature_idx].perception_cache[8] =
            (self.creatures[creature_idx].energy / 200.0).min(1.0);
        self.creatures[creature_idx].perception_cache[9] =
            (self.compute_nearby_energy(cx, cy, config) / config.energy_denominator).min(1.0);

        if !eyes_ready {
            return; // 冷却中，保持缓存不变
        }

        // 执行实际扫描
        self.compute_perception_inner(creature_idx, config);

        // 重置冷却
        self.creatures[creature_idx].eye_cooldown_timer = config.eye_cooldown;
    }

    /// 实际感知扫描（双眼 + 温度通道）
    fn compute_perception_inner(&mut self, creature_idx: usize, config: &Config) {
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;
        let heading = self.creatures[creature_idx].heading;
        let threshold = config.species_similarity_threshold;

        let eye_range = config.vision_range;
        let eye_half_fov = 70.0_f64.to_radians();
        let eye_offset_angle = 50.0_f64.to_radians();
        let eye_dirs = [heading - eye_offset_angle, heading + eye_offset_angle];
        // 眼睛世界坐标（用于计算从眼睛出发的距离）
        let body_radius = (self.creatures[creature_idx].energy * 1.28).cbrt();
        let eye_positions = [
            (
                cx + body_radius * (heading - eye_offset_angle).cos(),
                cy + body_radius * (heading - eye_offset_angle).sin(),
            ),
            (
                cx + body_radius * (heading + eye_offset_angle).cos(),
                cy + body_radius * (heading + eye_offset_angle).sin(),
            ),
        ];

        // 眼睛：最近距离
        let mut eye_food = [f64::MAX; 2];
        let mut eye_ally = [f64::MAX; 2];
        let mut eye_enemy = [f64::MAX; 2];
        // 眼睛视锥内能量累加（粒子+生物，用于能量密度通道）
        let mut eye_group_energy = [0.0_f64; 2];
        let mut eye_particle_energy = [0.0_f64; 2];

        // 临时取出缓冲区
        let mut creature_buf = std::mem::take(&mut self.creature_query_buf);
        let mut energy_buf = std::mem::take(&mut self.energy_query_buf);

        // === 能量粒子 ===
        self.energy_grid
            .query_into(cx, cy, eye_range, &mut energy_buf);
        for &idx in &energy_buf {
            let particle = &self.energy_particles[idx];
            if !particle.alive {
                continue;
            }

            let dx = particle.x - cx;
            let dy = particle.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 0.0 {
                continue;
            }

            let angle = dy.atan2(dx);

            // 双眼（从眼睛位置算距离）
            for (eye_i, &eye_dir) in eye_dirs.iter().enumerate() {
                let diff = angle_diff(angle, eye_dir);
                if diff.abs() <= eye_half_fov {
                    let (ex, ey) = eye_positions[eye_i];
                    let edx = particle.x - ex;
                    let edy = particle.y - ey;
                    let eye_dist = (edx * edx + edy * edy).sqrt();
                    if eye_dist <= eye_range {
                        if eye_dist < eye_food[eye_i] {
                            eye_food[eye_i] = eye_dist;
                        }
                        // 累加视锥内粒子能量
                        eye_particle_energy[eye_i] += particle.energy;
                    }
                }
            }
        }

        // === 生物 ===
        self.creature_grid
            .query_into(cx, cy, eye_range, &mut creature_buf);
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
            if dist <= 0.0 {
                continue;
            }

            let angle = dy.atan2(dx);
            let similarity =
                self.get_similarity(&self.creatures[creature_idx], &self.creatures[idx]);
            let is_ally = similarity >= threshold;

            // 双眼（从眼睛位置算距离）
            for (eye_i, &eye_dir) in eye_dirs.iter().enumerate() {
                let diff = angle_diff(angle, eye_dir);
                if diff.abs() <= eye_half_fov {
                    let (ex, ey) = eye_positions[eye_i];
                    let edx = other.x - ex;
                    let edy = other.y - ey;
                    let eye_dist = (edx * edx + edy * edy).sqrt();
                    if eye_dist <= eye_range {
                        if is_ally {
                            if eye_dist < eye_ally[eye_i] {
                                eye_ally[eye_i] = eye_dist;
                            }
                        } else if eye_dist < eye_enemy[eye_i] {
                            eye_enemy[eye_i] = eye_dist;
                        }
                        // 累加视锥内生物能量（用于热感通道）
                        eye_group_energy[eye_i] += other.energy;
                    }
                }
            }
        }

        // 归还缓冲区
        self.creature_query_buf = creature_buf;
        self.energy_query_buf = energy_buf;

        // === 写入通道 ===
        // 左眼 [0..3]: 食物接近度, 同族接近度, 异族接近度, 热感温度
        self.creatures[creature_idx].perception_cache[0] = if eye_food[0] < f64::MAX {
            body_radius / (body_radius + eye_food[0])
        } else {
            0.0
        };
        self.creatures[creature_idx].perception_cache[1] = if eye_ally[0] < f64::MAX {
            body_radius / (body_radius + eye_ally[0])
        } else {
            0.0
        };
        self.creatures[creature_idx].perception_cache[2] = if eye_enemy[0] < f64::MAX {
            body_radius / (body_radius + eye_enemy[0])
        } else {
            0.0
        };

        // 右眼 [4..7]: 食物接近度, 同族接近度, 异族接近度, 热感温度
        self.creatures[creature_idx].perception_cache[4] = if eye_food[1] < f64::MAX {
            body_radius / (body_radius + eye_food[1])
        } else {
            0.0
        };
        self.creatures[creature_idx].perception_cache[5] = if eye_ally[1] < f64::MAX {
            body_radius / (body_radius + eye_ally[1])
        } else {
            0.0
        };
        self.creatures[creature_idx].perception_cache[6] = if eye_enemy[1] < f64::MAX {
            body_radius / (body_radius + eye_enemy[1])
        } else {
            0.0
        };

        // 眼睛能量密度：(视锥内粒子能量 + 生物能量) / energy_denominator
        self.creatures[creature_idx].perception_cache[3] =
            ((eye_particle_energy[0] + eye_group_energy[0]) / config.energy_denominator).min(1.0);
        self.creatures[creature_idx].perception_cache[7] =
            ((eye_particle_energy[1] + eye_group_energy[1]) / config.energy_denominator).min(1.0);
    }

    // ========== 动作系统（6输出） ==========

    /// 执行动作：转向(0), 速度(1), 嘴(2), 繁殖(3), 繁殖阈值(4), 子代能量比例(5)
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        // 输出0: 转向
        let turn = outputs.get(0).copied().unwrap_or(0.0);
        // 输出1: 速度
        let speed = outputs.get(1).copied().unwrap_or(0.0);
        // 输出2: 嘴
        let mouth = outputs.get(2).copied().unwrap_or(0.0);
        // 输出3: 繁殖意愿
        let reproduce = outputs.get(3).copied().unwrap_or(0.0);
        // 输出4: 繁殖阈值 tanh(-1~1) → sigmoid → 20~200
        let raw4 = outputs.get(4).copied().unwrap_or(0.0);
        let reproduce_threshold = 20.0 + (raw4 * 0.5 + 0.5).clamp(0.0, 1.0) * 180.0;
        // 输出5: 子代能量比例 tanh(-1~1) → sigmoid → 0.1~0.5
        let raw5 = outputs.get(5).copied().unwrap_or(0.0);
        let reproduce_ratio = 0.1 + (raw5 * 0.5 + 0.5).clamp(0.0, 1.0) * 0.4;

        // 转向 + 移动
        let turn_rate = std::f64::consts::PI * 2.0; // 最大每秒一圈
        let turn_amount = turn * turn_rate * dt;
        self.creatures[creature_idx].heading += turn_amount;
        // 转向消耗：与角速度的平方成正比（慢转低耗，急转高耗）
        let angular_speed = turn_amount.abs() / dt;
        self.creatures[creature_idx].energy -= turn_amount.abs() * config.move_cost * angular_speed;

        let actual_speed = speed.abs() * 25.0;
        self.creatures[creature_idx].current_speed = actual_speed;
        if actual_speed > 0.05 {
            let heading = self.creatures[creature_idx].heading;
            let dx = heading.cos() * actual_speed * dt;
            let dy = heading.sin() * actual_speed * dt;
            let old_x = self.creatures[creature_idx].x;
            let old_y = self.creatures[creature_idx].y;
            self.creatures[creature_idx].x += dx;
            self.creatures[creature_idx].y += dy;

            let distance = (dx * dx + dy * dy).sqrt();
            let move_cost = distance * config.move_cost * actual_speed;
            self.creatures[creature_idx].energy -= move_cost;
            self.action_counts[0] += 1; // 移动

            // 生成痕迹点（能量守恒：移动消耗转化为痕迹，每个生物独立计时）
            if !self.trail_disabled && !self.trail_spawn_paused {
                self.creatures[creature_idx].trail_emit_timer -= dt;
                if self.creatures[creature_idx].trail_emit_timer <= 0.0 {
                    self.creatures[creature_idx].trail_emit_timer = config.trail_emit_interval;
                    let creature_radius = (self.creatures[creature_idx].energy * 1.28).cbrt();
                    let suppress_radius = config.trail_suppress_radius;
                    let sr2 = suppress_radius * suppress_radius;
                    let nearby_trails = self.trail_grid.query(old_x, old_y, suppress_radius);
                    let has_nearby_trail = nearby_trails.iter().any(|&ti| {
                        let t = &self.trail_points[ti];
                        if !t.alive {
                            return false;
                        }
                        let dx = t.x - old_x;
                        let dy = t.y - old_y;
                        dx * dx + dy * dy <= sr2
                    });
                    if !has_nearby_trail {
                        let creator_id = self.creatures[creature_idx].id;
                        let genome_hash = self.creatures[creature_idx].genome_hash;
                        self.trail_points.push(TrailPoint::new(
                            old_x,
                            old_y,
                            move_cost,
                            genome_hash,
                            creator_id,
                            creature_radius,
                        ));
                    }
                }
            }
        }

        // 嘴：接触食物自动吸收 + 对生物咬/喂
        self.action_mouth(creature_idx, mouth, config);

        // 繁殖（受冷却限制）
        if reproduce > 0.2 && self.creatures[creature_idx].reproduce_cooldown_timer <= 0.0 {
            if self.action_reproduce(creature_idx, reproduce_threshold, reproduce_ratio, config) {
                self.creatures[creature_idx].reproduce_cooldown_timer = config.reproduce_cooldown;
                self.action_counts[4] += 1; // 繁殖
            }
        }
    }

    /// 嘴动作：接触食物/痕迹自动吸收（不受冷却限制），对生物咬/喂（受冷却限制）
    fn action_mouth(&mut self, idx: usize, mouth: f64, config: &Config) {
        let body_radius = (self.creatures[idx].energy * 1.28).cbrt();
        let mouth_stroke = body_radius * 0.25;
        let mouth_arc_r = body_radius * 1.05 + mouth_stroke * 0.5;
        let heading = self.creatures[idx].heading;
        let cx = self.creatures[idx].x;
        let cy = self.creatures[idx].y;

        // 锥形吸收区域：从身体中心到嘴巴弧线外缘，heading ± 25° 扇形
        let mouth_outer_r = mouth_arc_r + mouth_stroke * 0.5;
        let half_arc: f64 = 0.4363; // 25° ≈ 0.4363 rad（与渲染一致）
        let mouth_outer_r_sq = mouth_outer_r * mouth_outer_r;

        // 接触食物自动吸收（锥形区域判定，不受冷却限制）
        let nearby_energy = self.energy_grid.query(cx, cy, mouth_outer_r);
        for &particle_idx in &nearby_energy {
            if self.energy_particles[particle_idx].alive {
                let px = self.energy_particles[particle_idx].x;
                let py = self.energy_particles[particle_idx].y;
                let dx = px - cx;
                let dy = py - cy;
                if dx * dx + dy * dy <= mouth_outer_r_sq {
                    let angle_diff = (dy.atan2(dx) - heading)
                        .sin()
                        .atan2((dy.atan2(dx) - heading).cos());
                    if angle_diff.abs() <= half_arc {
                        let energy = self.energy_particles[particle_idx].consume();
                        self.creatures[idx].energy += energy;
                        self.action_counts[1] += 1;
                        break;
                    }
                }
            }
        }

        // 接触痕迹点自动吸收（锥形区域判定，不受冷却限制）
        if !self.trail_disabled {
            let my_genome_hash = self.creatures[idx].genome_hash;
            let nearby_trails = self.trail_grid.query(cx, cy, mouth_outer_r);
            for &trail_idx in &nearby_trails {
                let trail = &self.trail_points[trail_idx];
                if trail.alive && trail.age > 2.0 && trail.genome_hash != my_genome_hash {
                    let dx = trail.x - cx;
                    let dy = trail.y - cy;
                    if dx * dx + dy * dy <= mouth_outer_r_sq {
                        let angle_diff = (dy.atan2(dx) - heading)
                            .sin()
                            .atan2((dy.atan2(dx) - heading).cos());
                        if angle_diff.abs() <= half_arc {
                            let energy = self.trail_points[trail_idx].consume();
                            self.creatures[idx].energy += energy;
                            break;
                        }
                    }
                }
            }
        }

        // 对生物的咬/喂 — 受嘴巴冷却限制
        if mouth.abs() <= 0.1 {
            return;
        }
        if self.creatures[idx].mouth_cooldown_timer > 0.0 {
            return;
        }

        let mx = cx + heading.cos() * mouth_arc_r;
        let my = cy + heading.sin() * mouth_arc_r;
        let bite_query_range = mouth_stroke + config.contact_range;
        let nearby_creatures = self.creature_grid.query(mx, my, bite_query_range);
        for &other_idx in &nearby_creatures {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }
            let ox = self.creatures[other_idx].x;
            let oy = self.creatures[other_idx].y;
            let other_radius = (self.creatures[other_idx].energy * 1.28).cbrt();
            let dist = ((ox - mx).powi(2) + (oy - my).powi(2)).sqrt();
            if dist >= mouth_stroke + other_radius {
                continue;
            }

            if mouth < -0.1 {
                // 咬（捕食）— 咬合力 = |mouth|
                let bite_force = (-mouth).min(1.0);
                let my_energy = self.creatures[idx].energy;
                let other_energy = self.creatures[other_idx].energy;

                // 攻击方战力 × 咬合力
                let my_speed_norm = (self.creatures[idx].current_speed / 50.0).min(1.0);
                let my_ally_energy = self.compute_nearby_ally_energy(idx, config);
                let attacker_score =
                    config.combat_power(my_energy, my_speed_norm, my_ally_energy) * bite_force;

                // 防御方战力
                let other_speed_norm = (self.creatures[other_idx].current_speed / 50.0).min(1.0);
                let other_ally_energy = self.compute_nearby_ally_energy(other_idx, config);
                let defender_score =
                    config.combat_power(other_energy, other_speed_norm, other_ally_energy);

                let damage_ratio = attacker_score / (attacker_score + defender_score + 0.001);
                let transfer = other_energy * damage_ratio * config.bite_transfer_rate;

                let similarity =
                    self.get_similarity(&self.creatures[idx], &self.creatures[other_idx]);
                let efficiency = 1.0 - similarity;
                self.creatures[other_idx].energy -= transfer;
                self.creatures[idx].energy += transfer * efficiency;
                self.action_counts[2] += 1;
            } else {
                // 喂（哺育）— 无战力评估
                let feed_strength = mouth.min(1.0);
                let my_energy = self.creatures[idx].energy;
                let transfer = my_energy * feed_strength * 0.2;
                self.creatures[idx].energy -= transfer;
                self.creatures[other_idx].energy += transfer * config.feed_efficiency;
                self.action_counts[3] += 1;
            }

            // 重置嘴巴冷却
            self.creatures[idx].mouth_cooldown_timer = config.mouth_cooldown;
            break; // 每次只对一个目标
        }
    }

    /// 计算指定生物附近同族总能量（用于战力计算，范围=vision_range）
    fn compute_nearby_ally_energy(&self, creature_idx: usize, config: &Config) -> f64 {
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;
        let nearby = self.creature_grid.query(cx, cy, config.vision_range);
        let threshold = config.species_similarity_threshold;
        let mut total = 0.0;
        for &other_idx in &nearby {
            if other_idx == creature_idx || !self.creatures[other_idx].alive {
                continue;
            }
            let similarity =
                self.get_similarity(&self.creatures[creature_idx], &self.creatures[other_idx]);
            if similarity >= threshold {
                total += self.creatures[other_idx].energy;
            }
        }
        total
    }

    /// 繁殖（支持有性/无性）
    fn action_reproduce(
        &mut self,
        idx: usize,
        threshold: f64,
        ratio: f64,
        config: &Config,
    ) -> bool {
        if self.creatures[idx].energy < threshold {
            return false;
        }

        let child_energy = self.creatures[idx].energy * ratio;
        self.creatures[idx].energy -= child_energy;

        let mut rng = rand::thread_rng();
        let heading = self.creatures[idx].heading;
        let body_radius = (self.creatures[idx].energy * 1.28).cbrt();
        let behind_dist = body_radius * 2.0 + rng.gen_range(0.0..5.0);
        let offset_x = -heading.cos() * behind_dist + rng.gen_range(-3.0..3.0);
        let offset_y = -heading.sin() * behind_dist + rng.gen_range(-3.0..3.0);

        // 尝试找同种配偶
        let mate_genome = self.find_mate(idx, config);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;

        let mut child = if let Some(mate_genome) = mate_genome {
            let crossover_genome =
                Genome::crossover(&self.creatures[idx].genome, &mate_genome, true);
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

        // 种族颜色继承：与源头基因比较，相似度 >= 阈值则同族，否则建新族
        let parent_clan = self.creatures[idx].clan_hash;
        let is_same_clan = if let Some(founder_genome) = self.clan_genomes.get(&parent_clan) {
            founder_genome.similarity(&child.genome) >= config.species_similarity_threshold
        } else {
            // 源头基因已丢失，回退为与父代比较
            self.creatures[idx].genome.similarity(&child.genome)
                >= config.species_similarity_threshold
        };
        if is_same_clan {
            child.clan_hash = parent_clan;
        } else {
            // 建新族，记录源头基因
            self.clan_genomes
                .insert(child.clan_hash, child.genome.clone());
        }

        self.notify_born(&child);
        self.creatures.push(child);
        true
    }

    fn find_mate(&self, idx: usize, config: &Config) -> Option<Genome> {
        let creature = &self.creatures[idx];
        let nearby = self
            .creature_grid
            .query(creature.x, creature.y, config.contact_range);

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

    // ========== 更新/清理 ==========

    fn update_energy_particles(&mut self, dt: f64, config: &Config) {
        for particle in &mut self.energy_particles {
            let decay = match particle.source {
                ParticleSource::Volcano => config.volcano_decay_rate,
                ParticleSource::Meteorite => config.meteorite_decay_rate,
            };
            particle.update(dt, decay);
        }
    }

    fn update_trail_points(&mut self, dt: f64, config: &Config) {
        for trail in &mut self.trail_points {
            if trail.alive {
                trail.update(dt, config.trail_decay_rate);
            }
        }
    }

    fn cleanup(&mut self) {
        let mut had_deaths = false;

        for creature in &self.creatures {
            if !creature.alive {
                had_deaths = true;
                self.notify_died(creature.id);
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

        // 死亡生物的痕迹点也消失
        if !self.trail_disabled {
            for creature in &self.creatures {
                if !creature.alive {
                    let dead_id = creature.id;
                    for trail in &mut self.trail_points {
                        if trail.alive && trail.creator_id == dead_id {
                            trail.alive = false;
                        }
                    }
                }
            }
        }

        self.creatures.retain(|c| c.alive);
        self.energy_particles.retain(|e| e.alive);
        if !self.trail_disabled {
            self.trail_points.retain(|t| t.alive);
        }

        // 每10秒清理一次相似度缓存
        if self.time - self.cache_cleanup_timer >= 10.0 {
            self.cache_cleanup_timer = self.time;
            let alive_hashes: rustc_hash::FxHashSet<u64> = self
                .creatures
                .iter()
                .filter(|c| c.alive)
                .map(|c| c.genome_hash)
                .collect();
            let mut cache = self.similarity_cache.borrow_mut();
            cache.retain(|(h1, h2), _| alive_hashes.contains(h1) && alive_hashes.contains(h2));

            // 清理无活生物引用的种族源头基因
            let alive_clans: rustc_hash::FxHashSet<u64> = self
                .creatures
                .iter()
                .filter(|c| c.alive)
                .map(|c| c.clan_hash)
                .collect();
            self.clan_genomes.retain(|k, _| alive_clans.contains(k));
        }
    }

    // ========== 统计 ==========

    pub fn stats(&self, species_threshold: f64, config: &Config) -> WorldStats {
        let alive_creatures: Vec<_> = self.creatures.iter().filter(|c| c.alive).collect();
        let max_generation = alive_creatures
            .iter()
            .map(|c| c.generation)
            .max()
            .unwrap_or(0);

        let creature_energy: f64 = alive_creatures.iter().map(|c| c.energy).sum();
        let particle_energy: f64 = self
            .energy_particles
            .iter()
            .filter(|e| e.alive)
            .map(|e| e.energy)
            .sum();
        let trail_energy: f64 = self
            .trail_points
            .iter()
            .filter(|t| t.alive)
            .map(|t| t.energy)
            .sum();
        let trail_count = self.trail_points.iter().filter(|t| t.alive).count();
        let total_energy = creature_energy + particle_energy + trail_energy;
        let avg_energy = if alive_creatures.is_empty() {
            0.0
        } else {
            creature_energy / alive_creatures.len() as f64
        };

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
            trail_count,
            total_energy,
            creature_energy,
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

    /// 获取渲染上下文数据（clan_hash 作为颜色标识，出生时确定，终身不变）
    pub fn get_render_data(&self, _threshold: f64) -> FxHashMap<u64, u64> {
        let mut creature_species: FxHashMap<u64, u64> = FxHashMap::default();
        for creature in &self.creatures {
            if creature.alive {
                creature_species.insert(creature.id, creature.clan_hash);
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
        if total_count < 5 {
            return None;
        }

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
            if count < 5 {
                continue;
            }

            let ratio = count as f64 / total_count as f64;
            if ratio < 0.3 {
                continue;
            }

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

            if max_age < config.dominant_min_age {
                continue;
            }
            if has_death_data && sp_avg_age < death_median {
                continue;
            }

            let ratio_score = ratio;
            let energy_score = if global_avg_energy > 0.0 {
                (sp_avg_energy / global_avg_energy).min(2.0) / 2.0
            } else {
                0.0
            };
            let age_score = if has_death_data && death_median > 0.0 {
                (sp_avg_age / death_median).min(2.0) / 2.0
            } else {
                0.5
            };
            let gen_score = if global_max_generation > 0 {
                sp_max_gen as f64 / global_max_generation as f64
            } else {
                0.0
            };

            let score =
                0.30 * ratio_score + 0.20 * energy_score + 0.30 * age_score + 0.20 * gen_score;
            if score < 0.6 {
                continue;
            }

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
        let mut id_to_local: FxHashMap<u64, usize> =
            FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, c) in alive_creatures.iter().enumerate() {
            id_to_local.insert(c.id, i);
        }

        // 为每个生物找到族长
        let mut creature_clan_map: FxHashMap<usize, u64> =
            FxHashMap::with_capacity_and_hasher(n, Default::default());
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
                id_to_local
                    .get(&leader_id)
                    .map(|&local| alive_creatures[local].genome_hash)
                    .unwrap_or(0)
            });
        }

        let species_count = clan_counts.len();

        let mut species_vec: Vec<_> = clan_counts
            .into_iter()
            .map(|(leader_id, count)| RankedEntry {
                id: leader_id as usize,
                count,
            })
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
            if depth > 1000 {
                break;
            } // 安全上限
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

/// 角度差归一化到 [-π, π]
#[inline]
fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = a - b;
    while d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    while d < -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    d
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
    pub trail_count: usize,
    pub total_energy: f64,
    pub creature_energy: f64,
    pub max_generation: usize,
    pub avg_energy: f64,
    pub action_counts: [usize; 5],
    pub death_age_stats: DeathAgeStats,
    pub species_count: usize,
    pub top_species: Vec<RankedEntry>,
    pub creature_species_map: FxHashMap<u64, u64>,
    pub dominant_candidate: Option<DominantCandidate>,
}

impl Drop for World {
    fn drop(&mut self) {
        if let Some(ref bridge) = self.neural_bridge {
            bridge.shutdown();
        }
    }
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
