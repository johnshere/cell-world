use rand::Rng;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use super::{
    Creature, EnergyParticle, ParticleSource, SpatialGrid, TerrainMap, TerrainParams, TrailPoint,
};
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

/// 繁殖滑动窗口大小
const MAX_RECENT_REPRO: usize = 10_000;

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

    // 熔岩流待扩散队列（帧间延迟处理）
    lava_pending: Vec<(f64, f64, u8)>, // (x, y, chain_depth)

    // ID 计数器
    next_creature_id: u64,
    next_energy_id: u64,

    // 视窗范围（世界坐标，动态跟随）
    viewport_min_x: f64,
    viewport_min_y: f64,
    viewport_max_x: f64,
    viewport_max_y: f64,

    // 性能统计
    pub perf_stats: PerfStats,

    // 相似度缓存
    similarity_cache: RefCell<FxHashMap<(u64, u64), f64>>,
    cache_cleanup_timer: f64,

    // 行为触发次数统计（5事件：移动/吸收/咬/无性繁殖/有性繁殖）
    pub action_counts: [usize; 5],

    // 近1万次繁殖滑动窗口（true=有性, false=无性），用于面板统计
    recent_repro_events: VecDeque<bool>,

    // 奖励触发次数统计（3通道：能量/痕迹/集体）
    pub reward_counts: [usize; 3],

    // 死亡年龄统计
    death_ages: Vec<f64>,
    death_age_sum: f64,
    death_stats_dirty: bool,
    pub death_age_stats: DeathAgeStats,
    total_deaths: usize,

    // 种族缓存（祖先追溯模型）
    clan_cache: RefCell<Option<ClanCache>>,
    clan_cache_time: RefCell<f64>,
    /// 族长追溯缓存：creature_id → leader_id，避免每帧 O(generation) 链式查找
    /// 在 ensure_clan_cache 重建时清空
    clan_leader_cache: RefCell<FxHashMap<u64, u64>>,

    // 空间查询缓冲区（已迁移到 rayon 线程局部变量，保留字段兼容快照）
    #[allow(dead_code)]
    creature_query_buf: Vec<usize>,
    #[allow(dead_code)]
    energy_query_buf: Vec<usize>,
    #[allow(dead_code)]
    trail_query_buf: Vec<usize>,

    /// 痕迹系统完全禁用（手动开关，屏蔽所有痕迹逻辑）
    pub trail_disabled: bool,
    /// 痕迹生成暂停（FPS<30时自动开启，仅停止生成新痕迹）
    pub trail_spawn_paused: bool,

    // 空间索引脏标记（粒子/痕迹不移动，仅在增删时需要重建）
    energy_grid_dirty: bool,
    trail_grid_dirty: bool,

    /// 神经桥（None = legacy 纯 CPU 同步模式）
    neural_bridge: Option<NeuralBridge>,

    /// 种族源头基因组：clan_hash -> 建族者的 genome（用于后代相似度比较）
    clan_genomes: FxHashMap<u64, Genome>,

    /// 优势种库
    pub dominant_species: Vec<DominantCandidate>,

    /// 当前优势种检测结果缓存（每秒由 sim_thread 刷新一次，stats() 直接读取）
    pub dominant_candidate_cache: Option<DominantCandidate>,

    /// 自动投放定时器
    auto_spawn_timer: f64,

    /// 地形高度图（生成后冻结）
    pub terrain: TerrainMap,
}

/// 种族缓存（祖先追溯模型）
#[derive(Clone)]
struct ClanCache {
    /// 活生物局部索引 -> 族长 creature id
    creature_clan_map: FxHashMap<usize, u64>,
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
            lava_pending: Vec::new(),
            next_creature_id: 0,
            next_energy_id: 0,
            // 初始视窗居中于原点
            viewport_min_x: -700.0,
            viewport_min_y: -500.0,
            viewport_max_x: 700.0,
            viewport_max_y: 500.0,

            perf_stats: PerfStats::default(),
            similarity_cache: RefCell::new(FxHashMap::default()),
            cache_cleanup_timer: 0.0,
            action_counts: [0; 5],
            recent_repro_events: VecDeque::new(),
            reward_counts: [0; 3],
            death_ages: Vec::new(),
            death_age_sum: 0.0,
            death_stats_dirty: false,
            death_age_stats: DeathAgeStats::default(),
            total_deaths: 0,
            clan_cache: RefCell::new(None),
            clan_cache_time: RefCell::new(-999.0),
            clan_leader_cache: RefCell::new(FxHashMap::default()),
            creature_query_buf: Vec::new(),
            energy_query_buf: Vec::new(),
            trail_query_buf: Vec::new(),
            trail_disabled: false,
            trail_spawn_paused: false,
            energy_grid_dirty: true,
            trail_grid_dirty: true,
            neural_bridge: None,
            clan_genomes: FxHashMap::default(),
            dominant_species: Vec::new(),
            dominant_candidate_cache: None,
            auto_spawn_timer: 0.0,
            terrain: TerrainMap::default(),
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

    /// 设置神经桥（同步批处理模式）
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

        // 自动投放随机生物（定时，不从基因库取）
        if config.auto_spawn_interval > 0.0 {
            self.auto_spawn_timer += dt;
            if self.auto_spawn_timer >= config.auto_spawn_interval {
                self.auto_spawn_timer = 0.0;
                let alive_count = self.creatures.iter().filter(|c| c.alive).count();
                if config.max_creatures == 0 || alive_count < config.max_creatures {
                    self.spawn_creature(config);
                }
            }
        }

        // 重建空间索引
        let spatial_start = Instant::now();
        self.rebuild_spatial_index();
        self.perf_stats.spatial_ms = spatial_start.elapsed().as_secs_f64() * 1000.0;

        // 更新生物
        self.update_creatures(dt, config);

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

    /// 下一个生物ID
    pub fn next_creature_id(&self) -> u64 {
        self.next_creature_id
    }

    /// 下一个能量ID
    pub fn next_energy_id(&self) -> u64 {
        self.next_energy_id
    }

    /// 死亡年龄记录
    pub fn death_ages(&self) -> &[f64] {
        &self.death_ages
    }

    /// 死亡年龄总和
    pub fn death_age_sum(&self) -> f64 {
        self.death_age_sum
    }

    /// 种族源头基因组
    pub fn clan_genomes(&self) -> &FxHashMap<u64, Genome> {
        &self.clan_genomes
    }

    /// 从快照重建 World
    #[allow(clippy::too_many_arguments)]
    pub fn from_snapshot(
        creatures: Vec<Creature>,
        energy_particles: Vec<EnergyParticle>,
        trail_points: Vec<TrailPoint>,
        creature_grid: SpatialGrid,
        energy_grid: SpatialGrid,
        trail_grid: SpatialGrid,
        time: f64,
        volcano_timer: f64,
        next_creature_id: u64,
        next_energy_id: u64,
        action_counts: [usize; 5],
        death_age_stats: DeathAgeStats,
        death_ages: Vec<f64>,
        death_age_sum: f64,
        trail_disabled: bool,
        clan_genomes: FxHashMap<u64, Genome>,
        dominant_species: Vec<DominantCandidate>,
        _config: &Config,
    ) -> Self {
        Self {
            creatures,
            energy_particles,
            trail_points,
            creature_grid,
            energy_grid,
            trail_grid,
            time,
            volcano_timer,
            lava_pending: Vec::new(),
            next_creature_id,
            next_energy_id,
            viewport_min_x: -700.0,
            viewport_min_y: -500.0,
            viewport_max_x: 700.0,
            viewport_max_y: 500.0,
            perf_stats: PerfStats::default(),
            similarity_cache: RefCell::new(FxHashMap::default()),
            cache_cleanup_timer: 0.0,
            action_counts,
            recent_repro_events: VecDeque::new(),
            reward_counts: [0; 3],
            death_ages,
            death_age_sum,
            death_stats_dirty: true,
            total_deaths: death_age_stats.total_deaths,
            death_age_stats,
            clan_cache: RefCell::new(None),
            clan_cache_time: RefCell::new(-999.0),
            clan_leader_cache: RefCell::new(FxHashMap::default()),
            creature_query_buf: Vec::new(),
            energy_query_buf: Vec::new(),
            trail_query_buf: Vec::new(),
            trail_disabled,
            trail_spawn_paused: false,
            energy_grid_dirty: true,
            trail_grid_dirty: true,
            neural_bridge: None,
            clan_genomes,
            dominant_species,
            dominant_candidate_cache: None,
            auto_spawn_timer: 0.0,
            terrain: TerrainMap::default(),
        }
    }

    /// 一次性生成地形（按当前 config.volcano_radius + 用户参数锁定）
    pub fn generate_terrain(&mut self, config: &Config, params: &TerrainParams) {
        self.terrain.generate(config.volcano_radius, params);
    }

    /// 地形对移动消耗的乘子。地形未生成或所在位置无数据时返回 1.0。
    /// 公式：1 + max(dh/dist, 0) × terrain_slope_cost  （上坡加成，下坡不补贴）
    fn terrain_move_factor(
        &self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        distance: f64,
        config: &Config,
    ) -> f64 {
        if !self.terrain.is_generated() || distance <= 0.0 {
            return 1.0;
        }
        let h0 = self.terrain.height_at(x0, y0);
        let h1 = self.terrain.height_at(x1, y1);
        let (h0, h1) = match (h0, h1) {
            (Some(a), Some(b)) => (a as f64, b as f64),
            _ => return 1.0,
        };
        let dh = h1 - h0;
        1.0 + (dh / distance).max(0.0) * config.terrain_slope_cost
    }

    // ========== 生成 ==========

    fn replenish_creatures(&mut self, config: &Config) {
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();

        // 低于最小数量时
        if alive_count < config.min_creatures {
            // 如果开启了 stop_on_extinction，触发暂停（一次性事件，由 app.rs 的 auto-pause 处理）
            if config.stop_on_extinction {
                return;
            }

            // 否则执行原有补充逻辑
            let mut rng = rand::thread_rng();
            loop {
                let alive_count = self.creatures.iter().filter(|c| c.alive).count();
                if alive_count >= config.min_creatures {
                    break;
                }
                if !self.dominant_species.is_empty() && rng.gen_bool(0.5) {
                    let idx = rng.gen_range(0..self.dominant_species.len());
                    let candidate = self.dominant_species[idx].clone();
                    // 继承优势种的代数：与手动投放保持一致（手动投放传 template.generation，
                    // 而 auto_save_dominant 把 template.generation 设为 max_generation）
                    self.spawn_from_template(
                        config,
                        &candidate.genome,
                        config.initial_energy,
                        candidate.max_generation,
                    );
                } else {
                    self.spawn_creature(config);
                }
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
        let creature = Creature::random(creature_id, x, y, energy);
        // 自然生成：以自身基因建族
        self.clan_genomes
            .insert(creature.clan_hash, creature.genome.clone());
        self.notify_born(&creature);
        self.creatures.push(creature);
    }

    /// 从模板生成生物
    pub fn spawn_from_template(
        &mut self,
        config: &Config,
        genome: &Genome,
        initial_energy: f64,
        generation: usize,
    ) {
        let mut rng = rand::thread_rng();
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let r = rng.gen_range(0.0_f64..1.0).sqrt() * config.volcano_radius * 0.8;
        let x = config.volcano_x + r * angle.cos();
        let y = config.volcano_y + r * angle.sin();
        let energy = initial_energy.max(config.initial_energy * 0.5);

        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;
        let creature = Creature::new(
            creature_id,
            x,
            y,
            energy,
            genome.clone(),
            generation,
            None,
            None,
        );
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

    /// 生成能量粒子（火山喷发 + 熔岩流扩散）
    fn spawn_energy(&mut self, dt: f64, config: &Config) {
        self.volcano_timer += dt;
        let current_volcano_interval = config.current_volcano_interval(self.time);
        if self.volcano_timer >= current_volcano_interval {
            self.volcano_timer = 0.0;
            self.volcano_erupt(config);
        }

        // 处理上一帧积累的熔岩流扩散
        self.process_lava_spread(config);
    }

    fn volcano_erupt(&mut self, config: &Config) {
        let mut rng = rand::thread_rng();
        let current_energy = config.current_volcano_energy(self.time);
        let kill_r2 = config.volcano_kill_radius * config.volcano_kill_radius;
        for _ in 0..config.volcano_count {
            let angle = rng.gen_range(0.0..std::f64::consts::TAU);
            // 半径上线性 PDF：p(r) = (1 - bias·(2r/R - 1)) / R, bias∈[-1,1]
            // bias<0 外围更密，bias>0 中心更密，bias=0 半径均匀；反 CDF 采样
            let u: f64 = rng.gen_range(0.0..1.0);
            let k = -config.volcano_spread_bias;
            let s = if k.abs() < 1e-9 {
                u
            } else {
                let one_minus_k = 1.0 - k;
                let disc = one_minus_k * one_minus_k + 4.0 * k * u;
                (-one_minus_k + disc.max(0.0).sqrt()) / (2.0 * k)
            };
            let r = s.clamp(0.0, 1.0) * config.volcano_radius;
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
            self.energy_grid_dirty = true;
            // 落地杀伤
            for c in &mut self.creatures {
                if c.alive && c.energy > 0.0 {
                    let dx = c.x - x;
                    let dy = c.y - y;
                    if dx * dx + dy * dy < kill_r2 {
                        let damage = c.energy
                            * (1.0
                                - (-current_energy * config.landing_damage_multiplier / c.energy)
                                    .exp());
                        c.energy = (c.energy - damage).max(0.0);
                        if c.energy <= 0.0 {
                            c.alive = false;
                        }
                    }
                }
            }
            // 子粒子簇生：10 次伯努利掷骰，每次以 lava_spread_probability 概率
            // 在落点周围梭形区域（长 200 / 短 20）内追加一个同能量普通粒子
            // 长轴方向 = 火山中心 → 落点的径向（沿粒子被甩出的轨迹延伸）
            let splash_half_len = 100.0;
            let splash_half_width = 10.0;
            let splash_prob = config.lava_spread_probability.clamp(0.0, 1.0);
            let dir_x = angle.cos();
            let dir_y = angle.sin();
            for _ in 0..10 {
                if !rng.gen_bool(splash_prob) {
                    continue;
                }
                // 椭圆均匀采样：单位圆 sqrt(u) 半径 → 缩放到 (半长, 半宽)
                let cangle = rng.gen_range(0.0..std::f64::consts::TAU);
                let cu: f64 = rng.gen_range(0.0..1.0);
                let cr = cu.sqrt();
                let local_x = cr * cangle.cos() * splash_half_len;
                let local_y = cr * cangle.sin() * splash_half_width;
                // 把本地坐标系旋转到径向：local_x 轴 → (dir_x, dir_y)
                let cx = x + local_x * dir_x - local_y * dir_y;
                let cy = y + local_x * dir_y + local_y * dir_x;
                let cid = self.next_energy_id;
                self.next_energy_id += 1;
                self.energy_particles.push(EnergyParticle::new(
                    cid,
                    cx,
                    cy,
                    current_energy,
                    f64::MAX,
                    ParticleSource::Volcano,
                ));
            }
        }

        // 熔岩流粒子：火山口附近小范围内喷出
        let lava_spawn_radius = config.volcano_radius * 0.05;
        for _ in 0..config.lava_count {
            let angle = rng.gen_range(0.0..std::f64::consts::TAU);
            let u: f64 = rng.gen_range(0.0..1.0);
            let r = u * lava_spawn_radius;
            let x = config.volcano_x + r * angle.cos();
            let y = config.volcano_y + r * angle.sin();
            let energy_id = self.next_energy_id;
            self.next_energy_id += 1;
            self.energy_particles.push(EnergyParticle::new_lava(
                energy_id,
                x,
                y,
                current_energy,
                0,
            ));
            self.energy_grid_dirty = true;
            // 熔岩流落地杀伤（火山口附近，系数=2）
            let dist_to_volcano = r; // 已在火山口附近
            let kill_factor = (2.0 * (1.0 - dist_to_volcano / config.volcano_radius)).max(0.0);
            let kill_r = config.volcano_kill_radius * kill_factor;
            let kill_r2 = kill_r * kill_r;
            if kill_r > 0.0 {
                for c in &mut self.creatures {
                    if c.alive && c.energy > 0.0 {
                        let dx = c.x - x;
                        let dy = c.y - y;
                        if dx * dx + dy * dy < kill_r2 {
                            let damage = c.energy
                                * (1.0
                                    - (-current_energy * config.landing_damage_multiplier
                                        / c.energy)
                                        .exp());
                            c.energy = (c.energy - damage).max(0.0);
                            if c.energy <= 0.0 {
                                c.alive = false;
                            }
                        }
                    }
                }
            }
        }
    }

    /// 计算区块等效液面高度。
    /// 等效高度 = 地形高度 + lava_level_per_particle × n，n = 区块内存活粒子数 + extra。
    fn effective_chunk_height(
        terrain: &TerrainMap,
        cx: i32,
        cy: i32,
        chunk_count: &FxHashMap<(i32, i32), usize>,
        lpp: f64,
        extra: usize,
    ) -> f64 {
        use super::terrain::GRID_WORLD_SIZE;
        let wx = cx as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
        let wy = cy as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
        let th = terrain.height_at(wx, wy).unwrap_or(i32::MAX) as f64;
        let n = chunk_count.get(&(cx, cy)).copied().unwrap_or(0) + extra;
        th + lpp * n as f64
    }

    /// 构建区块粒子计数快照（所有存活粒子，含普通粒子和熔岩粒子）
    fn build_chunk_count(&self) -> FxHashMap<(i32, i32), usize> {
        use super::terrain::GRID_WORLD_SIZE;
        let mut m = FxHashMap::default();
        for p in &self.energy_particles {
            if p.alive {
                let cx = (p.x / GRID_WORLD_SIZE).floor() as i32;
                let cy = (p.y / GRID_WORLD_SIZE).floor() as i32;
                *m.entry((cx, cy)).or_insert(0) += 1;
            }
        }
        m
    }

    /// 溢流机制：从 start_chunk 开始，模拟粒子沿液面梯度流向最低区块。
    /// 返回 Some(目标区块坐标) 或 None（超过 max_depth / 超出火山半径 → 丢弃粒子）。
    fn overflow_find_target(
        terrain: &TerrainMap,
        start_cx: i32,
        start_cy: i32,
        chunk_count: &FxHashMap<(i32, i32), usize>,
        lpp: f64,
        max_depth: usize,
        volcano_cx: f64,
        volcano_cy: f64,
        volcano_radius: f64,
    ) -> Option<(i32, i32)> {
        use super::terrain::GRID_WORLD_SIZE;

        const NEIGHBORS: [(i32, i32); 8] = [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ];

        let mut rng = rand::thread_rng();
        let mut cur_cx = start_cx;
        let mut cur_cy = start_cy;
        let mut visited = FxHashMap::default();
        visited.insert((cur_cx, cur_cy), true);

        for _ in 0..max_depth {
            // 当前区块模拟 +1 粒子后的等效高度（粒子固定贡献 1 单位地形，不随 lpp 放大）
            let cur_h =
                Self::effective_chunk_height(terrain, cur_cx, cur_cy, chunk_count, lpp, 0) + 1.0;

            // 找 8 邻居中有效高度最低的（不含 extra）
            let mut lowest_h = f64::MAX;
            let mut candidates: Vec<(i32, i32)> = Vec::new();
            for &(dx, dy) in &NEIGHBORS {
                let nx = cur_cx + dx;
                let ny = cur_cy + dy;
                if visited.contains_key(&(nx, ny)) {
                    continue;
                }
                let h = Self::effective_chunk_height(terrain, nx, ny, chunk_count, lpp, 0);
                if h < lowest_h {
                    lowest_h = h;
                    candidates.clear();
                    candidates.push((nx, ny));
                } else if (h - lowest_h).abs() < 1e-9 {
                    candidates.push((nx, ny));
                }
            }

            // 没有比当前区块+1更低的邻居 → 正常落下
            if candidates.is_empty() || lowest_h >= cur_h {
                // 检查火山半径
                let wx = cur_cx as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
                let wy = cur_cy as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
                let dist = ((wx - volcano_cx).powi(2) + (wy - volcano_cy).powi(2)).sqrt();
                if dist > volcano_radius {
                    return None;
                }
                return Some((cur_cx, cur_cy));
            }

            // 溢流到最低邻居（多个则随机选一个）
            let &(next_cx, next_cy) = if candidates.len() == 1 {
                &candidates[0]
            } else {
                &candidates[rng.gen_range(0..candidates.len())]
            };

            visited.insert((next_cx, next_cy), true);
            cur_cx = next_cx;
            cur_cy = next_cy;
        }

        // 超过 max_depth → 丢弃
        None
    }

    /// 处理熔岩流链式扩散（溢流机制：等效液面梯度 + visited 防回弹 + max_overflow_depth）
    fn process_lava_spread(&mut self, config: &Config) {
        use super::terrain::GRID_WORLD_SIZE;

        if self.lava_pending.is_empty() {
            return;
        }

        // 按 id 排序确保确定性处理顺序（lava_pending 存的是 (x, y, chain_depth)）
        let mut pending: Vec<_> = std::mem::take(&mut self.lava_pending);
        pending.sort_by_key(|&(_, _, depth)| depth);

        let current_energy = config.current_volcano_energy(self.time);
        let lpp = config.lava_level_per_particle;
        let max_depth = config.lava_max_overflow_depth;

        // 实时计数：每放置一个粒子后更新
        let mut chunk_count = self.build_chunk_count();

        let mut rng = rand::thread_rng();

        for (px, py, depth) in pending {
            let parent_cx = (px / GRID_WORLD_SIZE).floor() as i32;
            let parent_cy = (py / GRID_WORLD_SIZE).floor() as i32;

            // 至少分裂 1 个，以固定概率分裂第 2 个
            let extra_count = if rng.gen_bool(config.lava_spread_probability.clamp(0.0, 1.0)) {
                1
            } else {
                0
            };
            let total = 1 + extra_count;
            for _ in 0..total {
                // 子粒子初始落入父粒子所在区块，然后走溢流机制
                let target = Self::overflow_find_target(
                    &self.terrain,
                    parent_cx,
                    parent_cy,
                    &chunk_count,
                    lpp,
                    max_depth,
                    config.volcano_x,
                    config.volcano_y,
                    config.volcano_radius,
                );

                let (target_cx, target_cy) = match target {
                    Some(t) => t,
                    None => continue, // 溢流失败或超出半径 → 丢弃
                };

                // 在目标区块内随机落点
                let nx = target_cx as f64 * GRID_WORLD_SIZE + rng.gen_range(0.0..GRID_WORLD_SIZE);
                let ny = target_cy as f64 * GRID_WORLD_SIZE + rng.gen_range(0.0..GRID_WORLD_SIZE);

                // 放置粒子
                let eid = self.next_energy_id;
                self.next_energy_id += 1;
                self.energy_particles.push(EnergyParticle::new_lava(
                    eid,
                    nx,
                    ny,
                    current_energy,
                    depth + 1,
                ));
                self.energy_grid_dirty = true;

                // 实时更新区块计数
                *chunk_count.entry((target_cx, target_cy)).or_insert(0) += 1;
            }
        }
    }

    // ========== 空间索引 ==========

    fn rebuild_spatial_index(&mut self) {
        // 生物每帧移动，必须重建
        self.creature_grid.clear();
        for (i, c) in self.creatures.iter().enumerate() {
            if c.alive {
                self.creature_grid.insert(i, c.x, c.y);
            }
        }

        // 粒子不移动，仅在新增/删除时重建
        if self.energy_grid_dirty {
            self.energy_grid.clear();
            for (i, e) in self.energy_particles.iter().enumerate() {
                if e.alive {
                    self.energy_grid.insert(i, e.x, e.y);
                }
            }
            self.energy_grid_dirty = false;
        }

        // 痕迹不移动，仅在新增/删除时重建
        if !self.trail_disabled && self.trail_grid_dirty {
            self.trail_grid.clear();
            for (i, t) in self.trail_points.iter().enumerate() {
                if t.alive {
                    self.trail_grid.insert(i, t.x, t.y);
                }
            }
            self.trail_grid_dirty = false;
        }
    }

    // ========== 更新生物 ==========

    fn update_creatures(&mut self, dt: f64, config: &Config) {
        let has_bridge = self.neural_bridge.is_some();

        let creature_count = self.creatures.len();
        let need_per_creature_timing = config.compute_energy_factor > 0.0;

        let total_start = Instant::now();

        // ========== 阶段1: 并行感知 + 代谢 ==========
        let perceive_start = Instant::now();

        // 收集活跃生物的索引
        let alive_indices: Vec<usize> = (0..creature_count)
            .filter(|&i| self.creatures[i].alive)
            .collect();
        let alive_count = alive_indices.len();

        // 并行计算感知和代谢（只读访问空间索引和生物列表）
        let creatures_ref = &self.creatures;
        let energy_particles_ref = &self.energy_particles;
        let trail_points_ref = &self.trail_points;
        let energy_grid_ref = &self.energy_grid;
        let creature_grid_ref = &self.creature_grid;
        let trail_grid_ref = &self.trail_grid;
        let trail_disabled = self.trail_disabled;
        let terrain_ref = &self.terrain;

        let perception_results: Vec<PerceptionResult> = alive_indices
            .par_iter()
            .map(|&i| {
                // 每线程局部缓冲区
                let mut energy_buf = Vec::new();
                let mut creature_buf = Vec::new();
                let mut trail_buf = Vec::new();

                let creature = &creatures_ref[i];

                // 周围能量
                let nearby_energy = compute_nearby_energy_pure(
                    creature.x,
                    creature.y,
                    config,
                    energy_grid_ref,
                    creature_grid_ref,
                    trail_grid_ref,
                    energy_particles_ref,
                    creatures_ref,
                    trail_points_ref,
                    trail_disabled,
                    &mut energy_buf,
                    &mut creature_buf,
                    &mut trail_buf,
                );

                // 基础代谢
                let energy_ratio = creature.energy / config.initial_energy;
                let size_factor = energy_ratio.powf(config.metabolism_exponent);
                let age_multiplier = 1.0 + creature.age * config.age_metabolism_factor;
                let metabolism_cost = 0.025 * size_factor * age_multiplier * dt;
                let mut energy = creature.energy - metabolism_cost;

                // 体温逸散
                let body_radius = (energy.max(0.0) * 1.28).cbrt();
                let circumference = body_radius * std::f64::consts::TAU;
                let heat_factor = config.heat_floor
                    + (1.0 - config.heat_floor)
                        * (-nearby_energy / config.energy_denominator).exp();
                let heat_cost =
                    config.heat_dissipation_coefficient * circumference * heat_factor * dt;
                energy -= heat_cost;

                // 检查存活
                let alive = energy > 0.0 && !energy.is_nan() && !energy.is_infinite();

                // 感知计算（仅存活时）
                let (perception_cache, eye_scan_offset, follow_degree_raw, light_scan_offset) =
                    if alive {
                        compute_perception_pure(
                            i,
                            creature,
                            dt,
                            config,
                            creatures_ref,
                            energy_particles_ref,
                            trail_points_ref,
                            energy_grid_ref,
                            creature_grid_ref,
                            trail_grid_ref,
                            trail_disabled,
                            terrain_ref,
                            &mut energy_buf,
                            &mut creature_buf,
                            &mut trail_buf,
                        )
                    } else {
                        (
                            creature.perception_cache,
                            creature.eye_scan_offset,
                            0.0,
                            creature.light_scan_offset,
                        )
                    };

                // 跟随度稀疏计算：计时器 > 0 时复用缓存
                let follow_degree = if creature.follow_update_timer > 0.0 {
                    creature.follow_degree_cache
                } else {
                    follow_degree_raw
                };

                // vision_range 内活邻居数（用于孤独代价判定，不含自己）
                let neighbor_count = if alive {
                    creature_grid_ref.query_circle_into(
                        creature.x,
                        creature.y,
                        config.vision_range,
                        &mut creature_buf,
                        |j| (creatures_ref[j].x, creatures_ref[j].y),
                    );
                    creature_buf
                        .iter()
                        .filter(|&&j| j != i && creatures_ref[j].alive)
                        .count()
                } else {
                    0
                };

                PerceptionResult {
                    creature_idx: i,
                    perception_cache,
                    eye_scan_offset,
                    light_scan_offset,
                    energy_after_metabolism: energy,
                    alive,
                    follow_degree,
                    neighbor_count,
                }
            })
            .collect();

        let perceive_time = perceive_start.elapsed().as_secs_f64() * 1000.0;

        // ========== 阶段2a: 应用感知/代谢结果 + 收集 bridge 输入 ==========
        let mut bridge_inputs: Vec<CreatureInput> = if has_bridge {
            Vec::with_capacity(alive_count)
        } else {
            Vec::new()
        };

        for result in &perception_results {
            let i = result.creature_idx;

            // 应用代谢结果
            self.creatures[i].energy = result.energy_after_metabolism;
            self.creatures[i].perception_cache = result.perception_cache;
            self.creatures[i].eye_scan_offset = result.eye_scan_offset;
            self.creatures[i].light_scan_offset = result.light_scan_offset;

            // 孤独代价：vision_range 内无活邻居 → age 加速 solitude_penalty 倍
            // 与无性繁殖代价共用同一个系数，统一压制"独立行为"
            let lonely = result.neighbor_count == 0;
            let age_factor = if lonely { config.solitude_penalty } else { 1.0 };
            self.creatures[i].age += dt * age_factor;

            // 跟随度稀疏计算计时器
            self.creatures[i].follow_update_timer -= dt;
            if self.creatures[i].follow_update_timer <= 0.0 {
                self.creatures[i].follow_degree_cache = result.follow_degree;
                self.creatures[i].follow_update_timer = config.follow_update_interval;
            }

            // 指数平滑 follow_level
            let target = self.creatures[i].follow_degree_cache;
            let rate = 4.0; // ~0.25s 响应时间
            let current = self.creatures[i].follow_level;
            self.creatures[i].follow_level += (target - current) * (rate * dt).min(1.0);

            // 冷却递减
            self.creatures[i].mouth_cooldown_timer -= dt;

            if !result.alive {
                self.creatures[i].alive = false;
                continue;
            }

            if has_bridge {
                bridge_inputs.push(CreatureInput {
                    creature_id: self.creatures[i].id,
                    perception: self.creatures[i].perception_cache,
                });
            }
        }

        // ========== 阶段2b: 同步执行神经批 ==========
        // 每次 update 固定 snn_ticks 个 tick，加速仅增加 update 频率，保证结果与倍速无关
        let snn_ticks = (config.neural_tick_rate * dt).round().max(1.0) as usize;
        let snn_start = Instant::now();
        let output_map: FxHashMap<u64, ([f64; 8], u64)> = if has_bridge {
            let outputs = self
                .neural_bridge
                .as_ref()
                .unwrap()
                .run_batch_sync(bridge_inputs, snn_ticks);
            outputs
                .into_iter()
                .map(|o| (o.creature_id, (o.outputs, o.compute_ns)))
                .collect()
        } else {
            FxHashMap::default()
        };
        let snn_time = snn_start.elapsed().as_secs_f64() * 1000.0;

        // ========== 阶段2c: 串行动作执行 ==========
        let actions_start = Instant::now();
        for result in &perception_results {
            let i = result.creature_idx;
            if !self.creatures[i].alive {
                continue;
            }

            let creature_t0 = if need_per_creature_timing {
                Instant::now()
            } else {
                total_start
            };

            if has_bridge {
                let (outputs, snn_ns) = output_map
                    .get(&self.creatures[i].id)
                    .copied()
                    .unwrap_or((self.creatures[i].last_outputs, 0));
                self.creatures[i].last_outputs = outputs;

                self.creatures[i].physio.clear();
                self.execute_actions(i, &outputs.to_vec(), dt, config);

                // 群体奖励：极化 × 朝向稳定 × 在移动（破毛线球，惩罚原地打转）
                if config.reward_group_enabled {
                    self.compute_group_reward(i, dt, config);
                }

                // 奖励信号通过 bridge 发送给神经线程（延迟一帧应用到正确的 SpikingNetwork）
                let total_reward = self.creatures[i]
                    .physio
                    .total_reward(&self.creatures[i].genome.physio);
                if let Some(ref bridge) = self.neural_bridge {
                    bridge.send_reward(self.creatures[i].id, total_reward);
                }

                if need_per_creature_timing {
                    let main_ns = creature_t0.elapsed().as_nanos() as u64;
                    self.creatures[i].frame_compute_ns = main_ns + snn_ns;
                }
            } else {
                // legacy 纯 CPU 同步路径
                let perception = self.creatures[i].perception_cache;
                let outputs = self.creatures[i].brain.tick_multi(&perception, snn_ticks);
                for (j, &v) in outputs.iter().enumerate().take(8) {
                    self.creatures[i].last_outputs[j] = v;
                }

                self.creatures[i].physio.clear();
                self.execute_actions(i, &outputs, dt, config);

                // 群体奖励：极化 × 朝向稳定 × 在移动（破毛线球，惩罚原地打转）
                if config.reward_group_enabled {
                    self.compute_group_reward(i, dt, config);
                }

                let total_reward = self.creatures[i]
                    .physio
                    .total_reward(&self.creatures[i].genome.physio);
                self.creatures[i].brain.apply_physiology(total_reward);

                if need_per_creature_timing {
                    let main_ns = creature_t0.elapsed().as_nanos() as u64;
                    self.creatures[i].frame_compute_ns = main_ns;
                }
            }
        }
        let actions_time = actions_start.elapsed().as_secs_f64() * 1000.0;

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
        self.perf_stats.snn_ms = snn_time;
        self.perf_stats.actions_ms = actions_time;
        self.perf_stats.total_ms = total_time;
        self.perf_stats.creature_count = alive_count;

        if need_per_creature_timing && alive_count > 0 {
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

    // ========== 动作系统（8输出） ==========

    /// 执行动作：转向(0), 速度(1), 嘴(2), 繁殖(3), 繁殖阈值(4), 子代能量比例(5), 痕迹强度(6), 发光(7)
    fn execute_actions(&mut self, creature_idx: usize, outputs: &[f64], dt: f64, config: &Config) {
        // 输出0: 转向 (block 25)
        let turn = outputs.get(0).copied().unwrap_or(0.0);
        // 输出1: 速度 (block 25)
        let speed = outputs.get(1).copied().unwrap_or(0.0);
        // 输出2: 嘴（负=咬，接触食物自动吸收）(block 25)
        let mouth = outputs.get(2).copied().unwrap_or(0.0);
        // 输出3: 繁殖意愿 (block -25)
        let reproduce = outputs.get(3).copied().unwrap_or(0.0);
        // 输出4: 繁殖阈值 tanh(-1~1) → sigmoid → 20~200 (block -25)
        let raw4 = outputs.get(4).copied().unwrap_or(0.0);
        let reproduce_threshold = 20.0 + (raw4 * 0.5 + 0.5).clamp(0.0, 1.0) * 180.0;
        // 输出5: 子代能量比例 tanh(-1~1) → sigmoid → 0.1~0.5 (block -25)
        let raw5 = outputs.get(5).copied().unwrap_or(0.0);
        let reproduce_ratio = 0.1 + (raw5 * 0.5 + 0.5).clamp(0.0, 1.0) * 0.4;
        // 输出6: 痕迹强度 tanh(-1~1) → 正半轴 0~0.3 (block 25)
        let raw6 = outputs.get(6).copied().unwrap_or(0.0);
        let trail_strength = raw6.max(0.0) * 0.3;
        // 输出7: 发光强度 tanh(-1~1) → (v+1)/2 → 0~1, 量化一位小数 (block 26)
        let raw7 = outputs.get(7).copied().unwrap_or(0.0);
        let light = ((raw7 + 1.0) * 0.5).clamp(0.0, 1.0);
        self.creatures[creature_idx].light_intensity = (light * 10.0).round() / 10.0;

        // 转向 + 移动
        let turn_rate = std::f64::consts::PI * 2.0; // 最大每秒一圈
        let turn_amount = turn * turn_rate * dt;
        self.creatures[creature_idx].heading += turn_amount;
        // 转向消耗：与角速度的平方成正比（慢转低耗，急转高耗）
        let angular_speed = turn_amount.abs() / dt;
        let radius_cubed = self.creatures[creature_idx].energy.max(0.0) * 1.28;
        self.creatures[creature_idx].energy -=
            turn_amount.abs() * config.move_cost * 0.00001 * radius_cubed * angular_speed;

        let actual_speed = speed.abs() * config.max_speed;
        self.creatures[creature_idx].current_speed = actual_speed;
        let mut move_cost = 0.0;
        if actual_speed > 0.05 {
            let heading = self.creatures[creature_idx].heading;
            let prev_x = self.creatures[creature_idx].x;
            let prev_y = self.creatures[creature_idx].y;
            let dx = heading.cos() * actual_speed * dt;
            let dy = heading.sin() * actual_speed * dt;
            self.creatures[creature_idx].x += dx;
            self.creatures[creature_idx].y += dy;

            let distance = (dx * dx + dy * dy).sqrt();
            let follow_discount =
                self.creatures[creature_idx].follow_level * config.follow_cost_discount;
            // 地形因子：未生成时 = 1.0
            let terrain_factor = self.terrain_move_factor(
                prev_x,
                prev_y,
                self.creatures[creature_idx].x,
                self.creatures[creature_idx].y,
                distance,
                config,
            );
            move_cost = distance
                * config.move_cost
                * 0.00001
                * radius_cubed
                * actual_speed
                * (1.0 - follow_discount)
                * terrain_factor;
            self.creatures[creature_idx].energy -= move_cost;
            self.action_counts[0] += 1; // 移动
        }

        // 痕迹系统（基础：移动消耗 + 神经网络控制的额外能量投放）
        if !self.trail_disabled && !self.trail_spawn_paused {
            self.creatures[creature_idx].trail_emit_timer -= dt;
            if self.creatures[creature_idx].trail_emit_timer <= 0.0 {
                self.creatures[creature_idx].trail_emit_timer = config.trail_emit_interval;
                let cx = self.creatures[creature_idx].x;
                let cy = self.creatures[creature_idx].y;
                let creature_radius = (self.creatures[creature_idx].energy * 1.28).cbrt();
                let suppress_radius = config.trail_suppress_radius;
                let trail_points = &self.trail_points;
                let nearby_trails = self.trail_grid.query_circle(cx, cy, suppress_radius, |i| {
                    (trail_points[i].x, trail_points[i].y)
                });
                let has_nearby_trail = nearby_trails.iter().any(|&ti| self.trail_points[ti].alive);
                if !has_nearby_trail {
                    // 基础痕迹：移动消耗（已扣除，无额外开销）
                    let mut trail_energy = move_cost;
                    // 神经网络控制的额外投放（从自身能量扣除）
                    if trail_strength > 0.01 {
                        let extra = self.creatures[creature_idx].energy * trail_strength * dt;
                        self.creatures[creature_idx].energy -= extra;
                        trail_energy += extra;
                        // 主动释放痕迹的奖励
                        if config.reward_trail_enabled {
                            self.creatures[creature_idx].physio.pleasure_trail +=
                                extra / config.initial_energy;
                            self.reward_counts[1] += 1;
                        }
                    }
                    if trail_energy > 0.001 {
                        let creator_id = self.creatures[creature_idx].id;
                        let clan_hash = self.creatures[creature_idx].clan_hash;
                        self.trail_points.push(TrailPoint::new(
                            cx,
                            cy,
                            trail_energy,
                            clan_hash,
                            creator_id,
                            creature_radius,
                        ));
                        self.trail_grid_dirty = true;
                    }
                }
            }
        }

        // 嘴：接触食物自动吸收 + 对生物咬
        self.action_mouth(creature_idx, mouth, config);

        // 繁殖（数量上限，无冷却——完全由神经网络控制节奏）
        let alive_count = self.creatures.iter().filter(|c| c.alive).count();
        let pop_ok = config.max_creatures == 0 || alive_count < config.max_creatures;
        if reproduce > 0.2 && pop_ok {
            self.action_reproduce(creature_idx, reproduce_threshold, reproduce_ratio, config);
        }
    }

    /// 嘴动作：接触食物/痕迹自动吸收（不受冷却限制），对生物咬（受冷却限制）
    fn action_mouth(&mut self, idx: usize, mouth: f64, config: &Config) {
        let body_radius = (self.creatures[idx].energy * 1.28).cbrt();
        let mouth_stroke = body_radius * 0.25;
        let mouth_arc_r = body_radius * 1.05 + mouth_stroke * 0.5;
        let heading = self.creatures[idx].heading;
        let cx = self.creatures[idx].x;
        let cy = self.creatures[idx].y;
        let my_clan_hash = self.creatures[idx].clan_hash;

        // 锥形吸收区域：从身体中心到嘴巴弧线外缘，heading ± 25° 扇形
        let mouth_outer_r = mouth_arc_r + mouth_stroke * 0.5;
        let half_arc: f64 = 0.4363; // 25° ≈ 0.4363 rad（与渲染一致）

        // 接触食物自动吸收（锥形区域判定，不受冷却限制）
        let energy_particles_ref = &self.energy_particles;
        let nearby_energy = self.energy_grid.query_circle(cx, cy, mouth_outer_r, |i| {
            (energy_particles_ref[i].x, energy_particles_ref[i].y)
        });
        for &particle_idx in &nearby_energy {
            if self.energy_particles[particle_idx].alive {
                let px = self.energy_particles[particle_idx].x;
                let py = self.energy_particles[particle_idx].y;
                let dx = px - cx;
                let dy = py - cy;
                let angle_diff = (dy.atan2(dx) - heading)
                    .sin()
                    .atan2((dy.atan2(dx) - heading).cos());
                if angle_diff.abs() <= half_arc {
                    let energy = self.energy_particles[particle_idx].consume();
                    self.creatures[idx].energy += energy;
                    if config.reward_energy_enabled {
                        self.creatures[idx].physio.pleasure_energy +=
                            energy / config.initial_energy;
                        self.reward_counts[0] += 1;
                    }
                    self.action_counts[1] += 1;
                    break;
                }
            }
        }

        // 接触痕迹点自动吸收（锥形区域判定，不受冷却限制，自己的痕迹除外）
        if !self.trail_disabled {
            let my_id = self.creatures[idx].id;
            let trail_points_ref = &self.trail_points;
            let nearby_trails = self.trail_grid.query_circle(cx, cy, mouth_outer_r, |i| {
                (trail_points_ref[i].x, trail_points_ref[i].y)
            });
            for &trail_idx in &nearby_trails {
                let trail = &self.trail_points[trail_idx];
                if trail.alive && trail.creator_id != my_id && trail.clan_hash == my_clan_hash {
                    let dx = trail.x - cx;
                    let dy = trail.y - cy;
                    let angle_diff = (dy.atan2(dx) - heading)
                        .sin()
                        .atan2((dy.atan2(dx) - heading).cos());
                    if angle_diff.abs() <= half_arc {
                        let energy = self.trail_points[trail_idx].consume();
                        self.creatures[idx].energy += energy;
                        if config.reward_trail_enabled {
                            self.creatures[idx].physio.pleasure_trail +=
                                energy / config.initial_energy;
                            self.reward_counts[1] += 1;
                        }
                        break;
                    }
                }
            }
        }

        // 对生物的咬 — 受嘴巴冷却限制
        if mouth >= -0.1 {
            return;
        }
        if self.creatures[idx].mouth_cooldown_timer > 0.0 {
            return;
        }

        let mx = cx + heading.cos() * mouth_arc_r;
        let my = cy + heading.sin() * mouth_arc_r;
        let bite_query_range = mouth_stroke + config.contact_range;
        // 动态阈值：实际过滤用 mouth_stroke + other_radius（每只生物半径不同），故只能粗筛
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

            // 咬（捕食）— 咬合力 = |mouth|
            let bite_force = (-mouth).min(1.0);
            let my_energy = self.creatures[idx].energy;
            let other_energy = self.creatures[other_idx].energy;

            // 攻击方战力 × 咬合力
            let my_speed_norm = (self.creatures[idx].current_speed / config.max_speed).min(1.0);
            let my_ally_energy = self.compute_nearby_ally_energy(idx, config);
            let attacker_score =
                config.combat_power(my_energy, my_speed_norm, my_ally_energy) * bite_force;

            // 防御方战力
            let other_speed_norm =
                (self.creatures[other_idx].current_speed / config.max_speed).min(1.0);
            let other_ally_energy = self.compute_nearby_ally_energy(other_idx, config);
            let defender_score =
                config.combat_power(other_energy, other_speed_norm, other_ally_energy);

            let damage_ratio = attacker_score / (attacker_score + defender_score + 0.001);
            let damage = other_energy * damage_ratio;
            let actual_damage = damage.min(self.creatures[other_idx].energy);

            // 咬合效率 = 1 - 基因相似度：相似度越高获取越少，渐变而非悬崖
            let similarity = self.get_similarity(&self.creatures[idx], &self.creatures[other_idx]);
            let efficiency = 1.0 - similarity;
            self.creatures[other_idx].energy -= actual_damage;
            self.creatures[idx].energy += actual_damage * config.bite_transfer_rate * efficiency;
            self.action_counts[2] += 1;

            // 重置嘴巴冷却
            self.creatures[idx].mouth_cooldown_timer = config.mouth_cooldown;
            break; // 每次只对一个目标
        }
    }

    /// 计算指定生物附近同族总能量（按基因相似度加权，范围=vision_range）
    fn compute_nearby_ally_energy(&self, creature_idx: usize, config: &Config) -> f64 {
        let cx = self.creatures[creature_idx].x;
        let cy = self.creatures[creature_idx].y;
        let creatures = &self.creatures;
        let nearby = self
            .creature_grid
            .query_circle(cx, cy, config.vision_range, |i| {
                (creatures[i].x, creatures[i].y)
            });
        let mut total = 0.0;
        for &other_idx in &nearby {
            if other_idx == creature_idx || !self.creatures[other_idx].alive {
                continue;
            }
            // 按基因相似度加权援助：相似度越高援助越大，渐变过渡
            let similarity =
                self.get_similarity(&self.creatures[creature_idx], &self.creatures[other_idx]);
            total += self.creatures[other_idx].energy * similarity;
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

        // 繁殖能量代价：对称指数曲线，p = age / maturation_time（发育度）
        // 父辈消耗 = energy × ratio × exp(+k·p)，后辈得到 = energy × ratio × exp(-k·p)
        // p=0 时双方=1（无损耗），p>0 时损耗指数上升；与 structure_factor 形成姊妹曲线
        let mat = self.creatures[idx].genome.maturation_time.max(1.0);
        let p = self.creatures[idx].age / mat;
        let k = config.reproduction_age_cost_rate;
        let take_factor = (k * p).exp();
        let give_factor = (-k * p).exp();
        let parent_energy = self.creatures[idx].energy;
        let parent_cost = (parent_energy * ratio * take_factor).min(parent_energy);
        let child_energy = (parent_energy * ratio * give_factor).min(parent_cost);
        self.creatures[idx].energy -= parent_cost;

        let mut rng = rand::thread_rng();
        let heading = self.creatures[idx].heading;
        let body_radius = (self.creatures[idx].energy * 1.28).cbrt();
        let behind_dist = body_radius * 2.0 + rng.gen_range(0.0..5.0);
        let offset_x = -heading.cos() * behind_dist + rng.gen_range(-3.0..3.0);
        let offset_y = -heading.sin() * behind_dist + rng.gen_range(-3.0..3.0);

        // 尝试找同种配偶
        let mate = self.find_mate(idx, config);
        let is_sexual = mate.is_some();

        let reproduce_age_cost = 50.0;
        let creature_id = self.next_creature_id;
        self.next_creature_id += 1;

        let mut child = if let Some((mate_genome, mate_heading)) = mate {
            // 有性繁殖：年龄 +n（先繁殖，用原age计算结构变异/发育时间，再加惩罚）
            let parent_age_before = self.creatures[idx].age;
            let parent_maturation = self.creatures[idx].genome.maturation_time;
            let crossover_genome =
                Genome::crossover(&self.creatures[idx].genome, &mate_genome, true);
            let mut child_genome = crossover_genome.mutate(config, parent_age_before, true);
            child_genome.maturation_time = (parent_age_before + parent_maturation) / 2.0;
            self.creatures[idx].age += reproduce_age_cost;
            let child_heading = (heading + mate_heading) / 2.0;
            Creature::new(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                child_genome,
                self.creatures[idx].generation + 1,
                Some(self.creatures[idx].id),
                Some(child_heading),
            )
        } else {
            // 无性繁殖：年龄 +2n（惩罚独立繁殖，先繁殖用原age计算，再加惩罚）
            let child = self.creatures[idx].reproduce(
                creature_id,
                self.creatures[idx].x + offset_x,
                self.creatures[idx].y + offset_y,
                child_energy,
                config,
            );
            // 与有性繁殖一致 +50（先前 ×solitude_penalty 的额外惩罚已移除，差异化交由 mutation/能量等其它机制承担）
            self.creatures[idx].age += reproduce_age_cost;
            child
        };

        // 计数繁殖类型
        if is_sexual {
            self.action_counts[4] += 1;
        } else {
            self.action_counts[3] += 1;
        }
        // 滑动窗口：追踪最近1万次繁殖
        self.recent_repro_events.push_back(is_sexual);
        if self.recent_repro_events.len() > MAX_RECENT_REPRO {
            self.recent_repro_events.pop_front();
        }

        // 种族颜色继承：
        //   有性繁殖 → 与源头基因比较相似度，<阈值则建新族（基因创新登记新族）
        //   无性繁殖 → 强制继承父代 clan，永远不开新族（无性家系无创新带宽）
        let parent_clan = self.creatures[idx].clan_hash;
        if is_sexual {
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
        } else {
            child.clan_hash = parent_clan;
        }

        self.notify_born(&child);
        self.creatures.push(child);
        true
    }

    fn find_mate(&self, idx: usize, config: &Config) -> Option<(Genome, f64)> {
        let creature = &self.creatures[idx];
        let creatures = &self.creatures;
        let nearby =
            self.creature_grid
                .query_circle(creature.x, creature.y, config.contact_range, |i| {
                    (creatures[i].x, creatures[i].y)
                });

        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }
            let other = &self.creatures[other_idx];
            // 父子关系不能繁殖
            if other.parent_id == Some(self.creatures[idx].id)
                || self.creatures[idx].parent_id == Some(other.id)
            {
                continue;
            }
            // 交配阈值 = 聚类阈值 × 0.9，允许跨 clan 基因流
            // 聚类严格（0.95）、交配宽松（0.855），打破演化停滞
            if self.get_similarity(creature, other) >= config.species_similarity_threshold * 0.9 {
                return Some((other.genome.clone(), other.heading));
            }
        }
        None
    }

    /// 计算群体奖励：Vicsek 极化 × 自身朝向稳定度 × 自身在移动
    ///
    /// - polarization = |Σ单位朝向向量(自己+邻居)| / N，∈[0,1]
    ///   毛线球 → 0（向量相消）；齐头并进 → 1（向量同向叠加）
    /// - self_persist = |朝向 EWMA 模长|，∈[0,1]
    ///   直走久了→1；持续转向（含绕圈）→<1
    /// - motion = self_speed / max_speed，守门员防止"全员静止"退化
    ///
    /// 三项乘积同时满足：同向人多→大、同向时间长→大、自己也在动；
    /// 任一项塌陷则整体打折，毛线球与原地打转都拿不到奖励。
    fn compute_group_reward(&mut self, idx: usize, dt: f64, config: &Config) {
        let (cx, cy, heading, speed_norm, self_persist) = {
            let c = &mut self.creatures[idx];
            // EWMA 朝向：α = exp(-dt/τ)，τ=1s 记忆窗口
            let alpha = (-dt / 1.0).exp();
            let h = c.heading;
            c.smoothed_dir_x = alpha * c.smoothed_dir_x + (1.0 - alpha) * h.cos();
            c.smoothed_dir_y = alpha * c.smoothed_dir_y + (1.0 - alpha) * h.sin();
            let persist =
                (c.smoothed_dir_x * c.smoothed_dir_x + c.smoothed_dir_y * c.smoothed_dir_y).sqrt();
            let speed = (c.current_speed / config.max_speed).clamp(0.0, 1.0);
            (c.x, c.y, h, speed, persist)
        };

        if speed_norm <= 0.0 {
            return; // motion 守门员：静止不奖励
        }

        let creatures = &self.creatures;
        let nearby = self
            .creature_grid
            .query_circle(cx, cy, config.vision_range, |i| {
                (creatures[i].x, creatures[i].y)
            });
        let mut sum_vx = heading.cos();
        let mut sum_vy = heading.sin();
        let mut total = 1usize; // 含自己
        let mut neighbor_count = 0usize;
        for &other_idx in &nearby {
            if other_idx == idx || !self.creatures[other_idx].alive {
                continue;
            }
            let h = self.creatures[other_idx].heading;
            sum_vx += h.cos();
            sum_vy += h.sin();
            total += 1;
            neighbor_count += 1;
        }
        if neighbor_count == 0 {
            return; // 没邻居就没群体可言
        }

        let polarization = (sum_vx * sum_vx + sum_vy * sum_vy).sqrt() / (total as f64);
        let reward = polarization * self_persist * speed_norm;
        if reward > 0.0 {
            self.creatures[idx].physio.pleasure_group += reward;
            self.reward_counts[2] += 1;
        }
    }

    // ========== 更新/清理 ==========

    fn update_energy_particles(&mut self, dt: f64, config: &Config) {
        // 收集周期性杀伤事件：(x, y, kill_radius, particle_energy)
        let mut kill_events: Vec<(f64, f64, f64, f64)> = Vec::new();

        for particle in &mut self.energy_particles {
            let was_alive = particle.alive;
            let decay = if particle.lava {
                config.lava_decay_rate
            } else {
                config.volcano_decay_rate
            };
            particle.update(dt, decay, config.particle_min_energy);

            // 熔岩粒子周期性杀伤（关联扩散代数，非距离）
            if particle.alive && particle.lava {
                particle.lava_kill_timer += dt;
                let depth_ratio =
                    particle.chain_depth as f64 / config.lava_max_chain_depth.max(1) as f64;
                let interval = config.lava_kill_base_interval
                    * (1.0 + depth_ratio * config.lava_kill_distance_scale);
                if particle.lava_kill_timer >= interval {
                    particle.lava_kill_timer -= interval;
                    let kill_factor = (2.0 * (1.0 - depth_ratio)).max(0.0);
                    let kill_r = config.volcano_kill_radius * kill_factor;
                    if kill_r > 0.0 {
                        kill_events.push((particle.x, particle.y, kill_r, particle.energy));
                    }
                }
            }

            // 熔岩流粒子自然衰减死亡时入扩散队列
            if was_alive
                && !particle.alive
                && particle.lava
                && !particle.consumed
                && particle.chain_depth < config.lava_max_chain_depth
            {
                self.lava_pending
                    .push((particle.x, particle.y, particle.chain_depth));
            }
        }

        // 执行周期性杀伤
        for (kx, ky, kill_r, p_energy) in kill_events {
            let kill_r2 = kill_r * kill_r;
            for c in &mut self.creatures {
                if c.alive && c.energy > 0.0 {
                    let dx = c.x - kx;
                    let dy = c.y - ky;
                    if dx * dx + dy * dy < kill_r2 {
                        let damage = c.energy
                            * (1.0
                                - (-p_energy * config.landing_damage_multiplier / c.energy).exp());
                        c.energy = (c.energy - damage).max(0.0);
                        if c.energy <= 0.0 {
                            c.alive = false;
                        }
                    }
                }
            }
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
                self.total_deaths += 1;
                let age = creature.age;
                self.death_ages.push(age);
                self.death_age_sum += age;
                const MAX_DEATH_AGES: usize = 10_000;
                if self.death_ages.len() > MAX_DEATH_AGES {
                    let excess = self.death_ages.len() - MAX_DEATH_AGES;
                    let removed: f64 = self.death_ages.drain(0..excess).sum();
                    self.death_age_sum -= removed;
                }
                self.death_stats_dirty = true;
            }
        }

        if had_deaths {
            self.death_stats_dirty = true;
        }

        // 死亡生物的痕迹点也消失（HashSet 单次遍历，避免 O(D×T) 嵌套循环）
        if !self.trail_disabled && had_deaths {
            let dead_ids: rustc_hash::FxHashSet<u64> = self
                .creatures
                .iter()
                .filter(|c| !c.alive)
                .map(|c| c.id)
                .collect();
            for trail in &mut self.trail_points {
                if trail.alive && dead_ids.contains(&trail.creator_id) {
                    trail.alive = false;
                }
            }
        }

        self.creatures.retain(|c| c.alive);
        let old_energy_len = self.energy_particles.len();
        self.energy_particles.retain(|e| e.alive);
        if self.energy_particles.len() != old_energy_len {
            self.energy_grid_dirty = true;
        }
        if !self.trail_disabled {
            let old_trail_len = self.trail_points.len();
            self.trail_points.retain(|t| t.alive);
            if self.trail_points.len() != old_trail_len {
                self.trail_grid_dirty = true;
            }
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

            // 把三个 SpatialGrid 的 HashMap buckets 缩回当前 load
            // 避免历史峰值（曾经的种群高峰 / 粒子高峰 / 痕迹高峰）让 buckets 单调膨胀，
            // 影响后续每帧 cells.get(&key) 的 cache locality
            self.creature_grid.shrink_to_fit();
            self.energy_grid.shrink_to_fit();
            self.trail_grid.shrink_to_fit();
        }
    }

    /// 惰性重算死亡年龄统计（cleanup 仅标记脏位，stats() 首次读时触发）
    /// 使用 quickselect 求中位数 O(n)，避免全排序 O(n log n)
    fn recompute_death_stats(&mut self) {
        self.death_stats_dirty = false;
        let n = self.death_ages.len();
        if n == 0 {
            return;
        }
        // 单次遍历求 min/max（O(n)，death_ages 上限 10000 条）
        let mut min = f64::MAX;
        let mut max = 0.0_f64;
        for &v in &self.death_ages {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
        // 使用 select_nth_unstable_by 求中位数（O(n) 平均）
        let median = if n % 2 == 0 {
            let (_, m1, rest) = self
                .death_ages
                .select_nth_unstable_by(n / 2 - 1, |a, b| a.partial_cmp(b).unwrap());
            let (_, m2, _) = rest.select_nth_unstable_by(0, |a, b| a.partial_cmp(b).unwrap());
            (*m1 + *m2) / 2.0
        } else {
            let (_, m, _) = self
                .death_ages
                .select_nth_unstable_by(n / 2, |a, b| a.partial_cmp(b).unwrap());
            *m
        };
        self.death_age_stats = DeathAgeStats {
            count: n,
            avg: self.death_age_sum / n as f64,
            median,
            min,
            max,
            total_deaths: self.total_deaths,
        };
    }

    // ========== 统计 ==========

    pub fn stats(&mut self, species_threshold: f64, config: &Config) -> WorldStats {
        // 死亡统计惰性更新：放在最前面，避免与后续 alive_creatures 借用冲突
        if self.death_stats_dirty {
            self.recompute_death_stats();
        }

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
        // 理论投放速率（能量/秒）：火山每秒投放（含熔岩流）
        let volcano_interval = config.current_volcano_interval(self.time);
        let volcano_rate = if volcano_interval > 0.0 {
            let splash_expectation = 10.0 * config.lava_spread_probability.clamp(0.0, 1.0);
            let total_count =
                config.volcano_count as f64 * (1.0 + splash_expectation) + config.lava_count as f64;
            config.current_volcano_energy(self.time) * total_count / volcano_interval
        } else {
            0.0
        };
        let theoretical_energy = volcano_rate * 60.0; // 每分钟投放量
        let trail_energy: f64 = self
            .trail_points
            .iter()
            .filter(|t| t.alive)
            .map(|t| t.energy)
            .sum();
        let trail_count = self.trail_points.iter().filter(|t| t.alive).count();
        let total_energy = particle_energy + trail_energy;
        let avg_energy = if alive_creatures.is_empty() {
            0.0
        } else {
            creature_energy / alive_creatures.len() as f64
        };

        // 存活生物年龄统计
        let alive_age_stats = if alive_creatures.is_empty() {
            AliveAgeStats::default()
        } else {
            let n = alive_creatures.len();
            let mut ages: Vec<f64> = alive_creatures.iter().map(|c| c.age).collect();
            let sum: f64 = ages.iter().sum();
            let mut min = f64::MAX;
            let mut max = 0.0_f64;
            for &v in &ages {
                if v < min {
                    min = v;
                }
                if v > max {
                    max = v;
                }
            }
            let median = if n % 2 == 0 {
                let (_, m1, rest) =
                    ages.select_nth_unstable_by(n / 2 - 1, |a, b| a.partial_cmp(b).unwrap());
                let (_, m2, _) = rest.select_nth_unstable_by(0, |a, b| a.partial_cmp(b).unwrap());
                (*m1 + *m2) / 2.0
            } else {
                let (_, m, _) =
                    ages.select_nth_unstable_by(n / 2, |a, b| a.partial_cmp(b).unwrap());
                *m
            };
            AliveAgeStats {
                count: n,
                avg: sum / n as f64,
                median,
                max,
                min,
            }
        };

        // 种族统计（按 clan_hash 聚合，同时累计节点数用于算每族平均节点数）
        let mut clan_counts: FxHashMap<u64, (usize, usize)> = FxHashMap::default();
        for c in &alive_creatures {
            let entry = clan_counts.entry(c.clan_hash).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += c.genome.nodes.len();
        }
        let clan_count = clan_counts.len();
        let mut clan_vec: Vec<(u64, usize, f64)> = clan_counts
            .into_iter()
            .map(|(hash, (count, nodes_sum))| {
                let avg_nodes = nodes_sum as f64 / count.max(1) as f64;
                (hash, count, avg_nodes)
            })
            .collect();
        clan_vec.sort_by(|a, b| b.1.cmp(&a.1));
        clan_vec.truncate(10);

        // 优势种检测：每秒由 sim_thread 调用 refresh_dominant_candidate 刷新缓存，此处只读
        // 注意：种族缓存仍在 stats() 内 ensure 一次，因为面板渲染要用 clan map（不只是 dominant）
        self.ensure_clan_cache(species_threshold);
        let dominant_candidate = self.dominant_candidate_cache.clone();

        WorldStats {
            time: self.time,
            creature_count: alive_creatures.len(),
            energy_particle_count: self.energy_particles.iter().filter(|e| e.alive).count(),
            trail_count,
            total_energy,
            creature_energy,
            theoretical_energy,
            max_generation,
            avg_energy,
            action_counts: {
                let mut acts = self.action_counts;
                // 繁殖统计替换为近1万次滑动窗口计数
                let sexual = self.recent_repro_events.iter().filter(|&&s| s).count();
                let asexual = self.recent_repro_events.len() - sexual;
                acts[3] = asexual;
                acts[4] = sexual;
                acts
            },
            reward_counts: self.reward_counts,
            death_age_stats: self.death_age_stats.clone(),
            alive_age_stats,
            clan_count,
            top_clans: clan_vec,
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

    /// 每秒一次：执行优势种检测并写入缓存
    /// 由 sim_thread 主循环以 1Hz 频率调用；stats() 只读取缓存
    pub fn refresh_dominant_candidate(&mut self, config: &Config) {
        let species_threshold = config.species_similarity_threshold;
        let alive_creatures: Vec<_> = self.creatures.iter().filter(|c| c.alive).collect();
        if alive_creatures.is_empty() {
            self.dominant_candidate_cache = None;
            return;
        }
        let max_generation = alive_creatures
            .iter()
            .map(|c| c.generation)
            .max()
            .unwrap_or(0);
        let creature_energy: f64 = alive_creatures.iter().map(|c| c.energy).sum();
        let avg_energy = creature_energy / alive_creatures.len() as f64;

        self.ensure_clan_cache(species_threshold);
        let creature_clan_map = self
            .clan_cache
            .borrow()
            .as_ref()
            .unwrap()
            .creature_clan_map
            .clone();

        self.dominant_candidate_cache = self.detect_dominant(
            &alive_creatures,
            &creature_clan_map,
            avg_energy,
            max_generation,
            config,
        );
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
                creature_clan_map: FxHashMap::default(),
            };
        }

        // 清空族长追溯缓存（生物死亡/出生后缓存变脏，重建时重置）
        self.clan_leader_cache.borrow_mut().clear();

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

        ClanCache { creature_clan_map }
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

        // 查缓存（id → leader_id），避免重复走祖先链
        if let Some(&leader_id) = self.clan_leader_cache.borrow().get(&creature.id) {
            return leader_id;
        }

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

        let leader_id = if ancestor_local == local_idx {
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
        };

        self.clan_leader_cache
            .borrow_mut()
            .insert(creature.id, leader_id);
        leader_id
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

// ========== 并行感知阶段数据结构与纯函数 ==========

/// 感知阶段每只生物的计算结果（由并行阶段产出，串行阶段消费）
struct PerceptionResult {
    creature_idx: usize,
    perception_cache: [f64; 20],
    eye_scan_offset: [f64; 2],
    light_scan_offset: f64,
    energy_after_metabolism: f64,
    alive: bool,
    follow_degree: f64,
    /// vision_range 内活邻居数（不含自己）；用于孤独代价判定
    neighbor_count: usize,
}

/// 纯函数：计算 vision_range 内的周围能量密度（只读空间索引）
fn compute_nearby_energy_pure(
    x: f64,
    y: f64,
    config: &Config,
    energy_grid: &SpatialGrid,
    creature_grid: &SpatialGrid,
    trail_grid: &SpatialGrid,
    energy_particles: &[EnergyParticle],
    creatures: &[Creature],
    trail_points: &[TrailPoint],
    trail_disabled: bool,
    energy_buf: &mut Vec<usize>,
    creature_buf: &mut Vec<usize>,
    trail_buf: &mut Vec<usize>,
) -> f64 {
    let range = config.vision_range;
    let mut total = 0.0;
    const DIST_MIN_SQ: f64 = 0.01;
    // 粒子
    energy_grid.query_circle_into(x, y, range, energy_buf, |i| {
        (energy_particles[i].x, energy_particles[i].y)
    });
    for &idx in energy_buf.iter() {
        let p = &energy_particles[idx];
        if p.alive {
            let dx = p.x - x;
            let dy = p.y - y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > DIST_MIN_SQ {
                total += p.energy / dist_sq;
            }
        }
    }
    // 生物
    creature_grid.query_circle_into(x, y, range, creature_buf, |i| {
        (creatures[i].x, creatures[i].y)
    });
    for &idx in creature_buf.iter() {
        let c = &creatures[idx];
        if c.alive && c.energy > 0.0 {
            let dx = c.x - x;
            let dy = c.y - y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq > DIST_MIN_SQ {
                total += c.energy / dist_sq;
            }
        }
    }
    // 痕迹
    if !trail_disabled {
        trail_grid.query_circle_into(x, y, range, trail_buf, |i| {
            (trail_points[i].x, trail_points[i].y)
        });
        for &idx in trail_buf.iter() {
            let t = &trail_points[idx];
            if t.alive {
                let dx = t.x - x;
                let dy = t.y - y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > DIST_MIN_SQ {
                    total += t.energy / dist_sq;
                }
            }
        }
    }
    total
}

/// 纯函数：扫描眼感知核心（只读，返回 20 通道感知结果和更新后的扫描偏移量）
fn compute_perception_pure(
    creature_idx: usize,
    creature: &Creature,
    dt: f64,
    config: &Config,
    creatures: &[Creature],
    energy_particles: &[EnergyParticle],
    trail_points: &[TrailPoint],
    energy_grid: &SpatialGrid,
    creature_grid: &SpatialGrid,
    trail_grid: &SpatialGrid,
    trail_disabled: bool,
    terrain: &TerrainMap,
    energy_buf: &mut Vec<usize>,
    creature_buf: &mut Vec<usize>,
    trail_buf: &mut Vec<usize>,
) -> ([f64; 20], [f64; 2], f64, f64) {
    let mut perception = creature.perception_cache;
    let mut scan_offsets = creature.eye_scan_offset;
    let mut light_scan_offset_out = creature.light_scan_offset;

    // 自身状态 [16] 始终更新
    perception[16] = (creature.energy / 2000.0).min(1.0);

    // 地形感知 [17]：前方地面高于/低于/等于当前位置（-1/0/1）
    perception[17] = if terrain.is_generated() {
        let ahead_dist = 15.0;
        let ahead_x = creature.x + creature.heading.cos() * ahead_dist;
        let ahead_y = creature.y + creature.heading.sin() * ahead_dist;
        let h_current = terrain.height_at(creature.x, creature.y).unwrap_or(0);
        let h_ahead = terrain.height_at(ahead_x, ahead_y).unwrap_or(0);
        let dh = (h_ahead - h_current) as f64;
        dh.signum()
    } else {
        0.0
    };

    // 推进扫描角度
    let scan_advance = config.eye_scan_speed.to_radians() * dt;
    let total_fov = 140.0_f64.to_radians();
    for eye in 0..2 {
        scan_offsets[eye] += scan_advance;
        if scan_offsets[eye] >= total_fov {
            scan_offsets[eye] -= total_fov;
        }
    }

    let cx = creature.x;
    let cy = creature.y;
    let heading = creature.heading;
    let my_speed = creature.current_speed;
    let eye_range = config.vision_range;
    let body_radius = (creature.energy * 1.28).cbrt();
    let my_clan_hash = creature.clan_hash;

    let beam_angles = [
        heading + 20.0_f64.to_radians() - scan_offsets[0],
        heading - 20.0_f64.to_radians() + scan_offsets[1],
    ];
    let beam_half_width = (config.eye_scan_speed * dt / 2.0)
        .to_radians()
        .max(5.0_f64.to_radians());

    let full_fov_half = total_fov / 2.0;
    let full_fov_dirs = [
        heading + 20.0_f64.to_radians() - total_fov / 2.0,
        heading - 20.0_f64.to_radians() + total_fov / 2.0,
    ];

    let mut nearest_dist = [f64::MAX; 2];
    let mut nearest_energy = [0.0_f64; 2];
    let mut nearest_type = [0.0_f64; 2];
    let mut nearest_is_ally = [0.0_f64; 2];
    let mut eye_energy_density = [0.0_f64; 2];
    let scan_norm = [
        scan_offsets[0] / total_fov * 2.0 - 1.0,
        scan_offsets[1] / total_fov * 2.0 - 1.0,
    ];

    // === 能量粒子 ===
    energy_grid.query_circle_into(cx, cy, eye_range, energy_buf, |i| {
        (energy_particles[i].x, energy_particles[i].y)
    });
    for &idx in energy_buf.iter() {
        let particle = &energy_particles[idx];
        if !particle.alive {
            continue;
        }
        let dx = particle.x - cx;
        let dy = particle.y - cy;
        let dist_sq = dx * dx + dy * dy;
        let dist = dist_sq.sqrt();
        if dist <= 0.0 {
            continue;
        }
        let angle = dy.atan2(dx);
        for eye_i in 0..2 {
            let diff_full = angle_diff(angle, full_fov_dirs[eye_i]);
            if diff_full.abs() <= full_fov_half && dist_sq > 0.01 {
                eye_energy_density[eye_i] += particle.energy / dist_sq;
            }
            let diff_beam = angle_diff(angle, beam_angles[eye_i]);
            if diff_beam.abs() <= beam_half_width && dist < nearest_dist[eye_i] {
                nearest_dist[eye_i] = dist;
                nearest_energy[eye_i] = particle.energy;
                nearest_type[eye_i] = if particle.lava { 0.50 } else { 0.25 };
                nearest_is_ally[eye_i] = 0.0;
            }
        }
    }

    // === 痕迹点 ===
    if !trail_disabled {
        trail_grid.query_circle_into(cx, cy, eye_range, trail_buf, |i| {
            (trail_points[i].x, trail_points[i].y)
        });
        for &idx in trail_buf.iter() {
            let trail = &trail_points[idx];
            if !trail.alive || trail.creator_id == creature.id {
                continue;
            }
            let dx = trail.x - cx;
            let dy = trail.y - cy;
            let dist_sq = dx * dx + dy * dy;
            let dist = dist_sq.sqrt();
            if dist <= 0.0 {
                continue;
            }
            let angle = dy.atan2(dx);
            let is_ally = if trail.clan_hash == my_clan_hash {
                1.0
            } else {
                0.0
            };
            for eye_i in 0..2 {
                let diff_full = angle_diff(angle, full_fov_dirs[eye_i]);
                if diff_full.abs() <= full_fov_half && dist_sq > 0.01 {
                    eye_energy_density[eye_i] += trail.energy / dist_sq;
                }
                let diff_beam = angle_diff(angle, beam_angles[eye_i]);
                if diff_beam.abs() <= beam_half_width && dist < nearest_dist[eye_i] {
                    nearest_dist[eye_i] = dist;
                    nearest_energy[eye_i] = trail.energy;
                    nearest_type[eye_i] = 0.75;
                    nearest_is_ally[eye_i] = is_ally;
                }
            }
        }
    }

    // === 生物 ===
    let mut nearest_creature_idx: [Option<usize>; 2] = [None; 2];
    creature_grid.query_circle_into(cx, cy, eye_range, creature_buf, |i| {
        (creatures[i].x, creatures[i].y)
    });
    for &idx in creature_buf.iter() {
        if idx == creature_idx {
            continue;
        }
        let other = &creatures[idx];
        if !other.alive {
            continue;
        }
        let dx = other.x - cx;
        let dy = other.y - cy;
        let dist_sq = dx * dx + dy * dy;
        let dist = dist_sq.sqrt();
        if dist <= 0.0 {
            continue;
        }
        let angle = dy.atan2(dx);
        for eye_i in 0..2 {
            let diff_full = angle_diff(angle, full_fov_dirs[eye_i]);
            if diff_full.abs() <= full_fov_half && dist_sq > 0.01 {
                eye_energy_density[eye_i] += other.energy / dist_sq;
            }
            let diff_beam = angle_diff(angle, beam_angles[eye_i]);
            if diff_beam.abs() <= beam_half_width && dist < nearest_dist[eye_i] {
                nearest_dist[eye_i] = dist;
                nearest_energy[eye_i] = other.energy;
                nearest_type[eye_i] = 1.0;
                nearest_creature_idx[eye_i] = Some(idx);
            }
        }
    }

    // 延迟计算 similarity + 朝向差 + 速度差
    let mut nearest_heading_diff = [0.0_f64; 2];
    let mut nearest_speed_diff = [0.0_f64; 2];
    for eye_i in 0..2 {
        if let Some(other_idx) = nearest_creature_idx[eye_i] {
            if nearest_type[eye_i] == 1.0 {
                nearest_is_ally[eye_i] = creature.genome.similarity(&creatures[other_idx].genome);
                nearest_heading_diff[eye_i] =
                    angle_diff(creatures[other_idx].heading, heading) / std::f64::consts::PI;
                nearest_speed_diff[eye_i] = ((creatures[other_idx].current_speed - my_speed)
                    / config.max_speed)
                    .clamp(-1.0, 1.0);
            }
        }
    }

    // === 写入 17 通道 ===
    let proximity_l = if nearest_dist[0] < f64::MAX {
        body_radius / (body_radius + nearest_dist[0])
    } else {
        0.0
    };
    perception[0] = scan_norm[0];
    perception[1] = proximity_l;
    perception[2] = (nearest_energy[0] / 200.0).min(1.0);
    perception[3] = nearest_type[0];
    perception[4] = nearest_is_ally[0];
    perception[5] = (eye_energy_density[0] / config.energy_denominator).min(1.0);
    perception[6] = nearest_heading_diff[0];
    perception[7] = nearest_speed_diff[0];

    let proximity_r = if nearest_dist[1] < f64::MAX {
        body_radius / (body_radius + nearest_dist[1])
    } else {
        0.0
    };
    perception[8] = scan_norm[1];
    perception[9] = proximity_r;
    perception[10] = (nearest_energy[1] / 200.0).min(1.0);
    perception[11] = nearest_type[1];
    perception[12] = nearest_is_ally[1];
    perception[13] = (eye_energy_density[1] / config.energy_denominator).min(1.0);
    perception[14] = nearest_heading_diff[1];
    perception[15] = nearest_speed_diff[1];

    // === 发光感知（360°扫描，block -2）===
    {
        let light_scan_advance = config.eye_scan_speed.to_radians() * dt;
        let mut light_offset = creature.light_scan_offset + light_scan_advance;
        if light_offset >= std::f64::consts::TAU {
            light_offset -= std::f64::consts::TAU;
        }
        let light_beam_angle = heading + light_offset;
        let light_beam_half = (config.eye_scan_speed * dt / 2.0)
            .to_radians()
            .max(5.0_f64.to_radians());

        let mut best_dist = f64::MAX;
        let mut best_intensity = 0.0_f64;

        // 复用 creature_buf（已被生物扫描填充，但这里需要重新查询全范围）
        creature_grid.query_circle_into(cx, cy, eye_range, creature_buf, |i| {
            (creatures[i].x, creatures[i].y)
        });
        for &idx in creature_buf.iter() {
            if idx == creature_idx {
                continue;
            }
            let other = &creatures[idx];
            if !other.alive || other.light_intensity <= 0.0 {
                continue;
            }
            let dx = other.x - cx;
            let dy = other.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 0.0 {
                continue;
            }
            let angle = dy.atan2(dx);
            let diff = angle_diff(angle, light_beam_angle);
            if diff.abs() <= light_beam_half && dist < best_dist {
                best_dist = dist;
                best_intensity = other.light_intensity;
            }
        }

        perception[18] = light_offset / std::f64::consts::PI - 1.0;
        perception[19] = best_intensity;
        light_scan_offset_out = light_offset;
    }

    // === 跟随度计算（全向直接遍历，不依赖扫描波束）===
    let mut follow_total = 0.0_f64;
    let opt_angle = config.follow_optimal_angle;
    let sigma_sq = config.follow_angle_width * config.follow_angle_width;
    // creature_buf 已在上方扫描生物时填充，包含 vision_range 内所有生物索引
    for &idx in creature_buf.iter() {
        if idx == creature_idx {
            continue;
        }
        let other = &creatures[idx];
        if !other.alive {
            continue;
        }
        let dx = other.x - cx;
        let dy = other.y - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= 0.0 {
            continue;
        }

        // 因子1: 方向对齐度（双方朝向差）
        let heading_diff = angle_diff(other.heading, heading);
        let alignment = 1.0 - heading_diff.abs() / std::f64::consts::PI;

        // 因子2: 最优距离（高斯钟形）
        let target_radius = (other.energy * 1.28).cbrt();
        let optimal_dist = 2.5 * (body_radius + target_radius);
        let dist_ratio = (dist - optimal_dist) / optimal_dist;
        let distance_factor = (-dist_ratio * dist_ratio).exp();

        // 因子3: 方位角双峰（±follow_optimal_angle 最省力，正前方非最优）
        let angle_to_target = dy.atan2(dx);
        let frontal_diff = angle_diff(angle_to_target, heading).abs();
        let offset_left = (frontal_diff - opt_angle).abs();
        let offset_right = (frontal_diff + opt_angle).abs();
        let offset = offset_left.min(offset_right);
        let position_factor = (-offset * offset / (2.0 * sigma_sq)).exp();

        follow_total += alignment * distance_factor * position_factor;
    }
    let follow_degree = (1.0 - (-follow_total).exp()) * config.follow_max_level;

    (
        perception,
        scan_offsets,
        follow_degree,
        light_scan_offset_out,
    )
}

// ========== 数据结构 ==========

#[derive(Default, Clone)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub struct DeathAgeStats {
    pub count: usize,
    pub avg: f64,
    pub median: f64,
    pub max: f64,
    pub min: f64,
    #[serde(default)]
    pub total_deaths: usize,
}

/// 存活生物年龄统计（仅当前活着的个体）
#[derive(Default, Clone)]
pub struct AliveAgeStats {
    pub count: usize,
    pub avg: f64,
    pub median: f64,
    pub max: f64,
    pub min: f64,
}

#[derive(Clone, Default)]
pub struct WorldStats {
    pub time: f64,
    pub creature_count: usize,
    pub energy_particle_count: usize,
    pub trail_count: usize,
    pub total_energy: f64,
    pub creature_energy: f64,
    pub theoretical_energy: f64,
    pub max_generation: usize,
    pub avg_energy: f64,
    pub action_counts: [usize; 5],
    /// 奖励触发次数（3通道：能量/痕迹/集体）
    pub reward_counts: [usize; 3],
    pub death_age_stats: DeathAgeStats,
    /// 存活生物年龄统计
    pub alive_age_stats: AliveAgeStats,
    pub clan_count: usize,
    /// 种族前十: (clan_hash, count, avg_nodes)
    pub top_clans: Vec<(u64, usize, f64)>,
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
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub struct DominantCandidate {
    pub genome: Genome,
    pub genome_hash: u64,
    pub score: f64,
    pub population_ratio: f64,
    pub avg_energy: f64,
    pub avg_age: f64,
    pub max_generation: usize,
}
