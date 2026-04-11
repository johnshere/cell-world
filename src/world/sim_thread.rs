use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use super::{Creature, EnergyParticle, HotSpring, TerrainMap, TerrainParams, TrailPoint};
use crate::config::Config;
use crate::neural::Genome;
use crate::snapshot::WorldSnapshot;
use crate::world::world::{PerfStats, WorldStats};
use crate::world::World;

/// 模拟步长：每次 update 推进固定 1/30 模拟秒，加速通过多次 update 实现
const SIM_DT: f64 = 1.0 / 30.0;
/// 渲染帧间隔：恒定 1/30 真实秒（~30fps），不随 speed 变化
const FRAME_INTERVAL: Duration = Duration::from_nanos(33_333_333); // 1/30 s

/// 模拟线程每帧导出给主线程的只读快照
#[derive(Clone)]
pub struct SimSnapshot {
    // 渲染用原始数据
    pub creatures: Vec<Creature>,
    pub energy_particles: Vec<EnergyParticle>,
    pub trail_points: Vec<TrailPoint>,
    pub hot_springs: Vec<HotSpring>,
    pub trail_disabled: bool,

    // 预计算统计
    pub time: f64,
    pub perf_stats: PerfStats,
    pub world_stats: WorldStats,
    pub creature_species: FxHashMap<u64, u64>,
    pub volcano_countdown: f64,
    /// 模拟线程累计步数（用于计算 SIM FPS）
    pub sim_step_count: u64,
    /// 灭绝停止标志
    pub stop_extinction_triggered: bool,
    /// 目标倍速
    pub target_speed: f64,
    /// 实际达到的倍速（世界实际每真实秒推进的模拟秒数）
    pub actual_speed: f64,
    /// 神经线程实际倍速（平滑后），bridge 模式下作为世界反压上限源
    pub neural_speed: f64,
    /// 地形快照（生成后冻结，每次都附带，便于渲染线程随时取用）
    pub terrain: TerrainMap,
}

impl Default for SimSnapshot {
    fn default() -> Self {
        Self {
            creatures: Vec::new(),
            energy_particles: Vec::new(),
            trail_points: Vec::new(),
            hot_springs: Vec::new(),
            trail_disabled: false,
            time: 0.0,
            perf_stats: PerfStats::default(),
            world_stats: WorldStats::default(),
            creature_species: FxHashMap::default(),
            volcano_countdown: 0.0,
            sim_step_count: 0,
            stop_extinction_triggered: false,
            target_speed: 1.0,
            actual_speed: 0.0,
            neural_speed: 0.0,
            terrain: TerrainMap::default(),
        }
    }
}

/// 主线程→模拟线程的指令
pub enum SimCommand {
    Pause,
    Resume,
    SetSpeed(f64),
    SetConfig(Config),
    SetViewport(f64, f64, f64, f64),
    SpawnCreature,
    SpawnFromTemplate(Genome, f64),
    KillCreature(u64),
    SetTrailDisabled(bool),
    SetTrailSpawnPaused(bool),
    /// 快照捕获：模拟线程捕获后通过 oneshot 回传
    CaptureSnapshot(mpsc::Sender<WorldSnapshot>),
    /// 快照恢复
    RestoreSnapshot(WorldSnapshot),
    /// 生成地形（一次性，覆盖已有）
    GenerateTerrain(TerrainParams),
    Shutdown,
}

/// app.rs 持有的模拟线程句柄
pub struct SimHandle {
    cmd_tx: mpsc::Sender<SimCommand>,
    snapshot: Arc<RwLock<SimSnapshot>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SimHandle {
    /// 发送指令到模拟线程
    pub fn send(&self, cmd: SimCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// 读取最新快照（只读）
    pub fn snapshot(&self) -> std::sync::RwLockReadGuard<'_, SimSnapshot> {
        self.snapshot.read().unwrap()
    }
}

impl Drop for SimHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(SimCommand::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// 启动模拟线程，返回句柄
pub fn spawn_sim_thread(world: World, config: Config) -> SimHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<SimCommand>();
    let snapshot = Arc::new(RwLock::new(SimSnapshot::default()));
    let snapshot_clone = Arc::clone(&snapshot);

    let handle = thread::Builder::new()
        .name("sim-thread".to_string())
        .spawn(move || {
            sim_loop(world, config, cmd_rx, snapshot_clone);
        })
        .expect("Failed to spawn sim thread");

    SimHandle {
        cmd_tx,
        snapshot,
        thread: Some(handle),
    }
}

fn sim_loop(
    mut world: World,
    mut config: Config,
    cmd_rx: mpsc::Receiver<SimCommand>,
    snapshot: Arc<RwLock<SimSnapshot>>,
) {
    let mut paused = false;
    let mut speed = config.initial_speed;
    let mut next_tick = Instant::now();
    let mut sim_step_count: u64 = 0;
    // 累加器：支持非整数 speed（如 1.5 → 交替 1/2 次 update）
    let mut speed_accumulator: f64 = 0.0;
    // 实际速率统计
    let mut actual_speed_timer = Instant::now();
    let mut actual_speed_steps: u64 = 0;
    let mut actual_speed: f64 = 0.0;

    // 神经吞吐反压采样（bridge 模式下生效）
    // - 每帧采样 inference_count 增量 → 瞬时 raw_speed
    // - 指数平均 τ=2s，避免抖动
    // - 有效 speed = min(target, max(1, ceil(smooth × 1.2)))
    const NEURAL_EMA_TAU: f64 = 2.0;
    const PROBE_RATIO: f64 = 1.2;
    let mut neural_speed_smooth: f64 = 0.0;
    let mut last_inference_count: u64 = world.neural_inference_count();
    let mut last_neural_sample = Instant::now();

    // 导出初始快照
    export_snapshot(
        &world,
        &config,
        &snapshot,
        speed,
        actual_speed,
        neural_speed_smooth,
        sim_step_count,
    );

    loop {
        // 处理所有待处理命令
        loop {
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    SimCommand::Pause => paused = true,
                    SimCommand::Resume => {
                        paused = false;
                        // 恢复时重置神经采样基线，避免把暂停期也算进去
                        last_inference_count = world.neural_inference_count();
                        last_neural_sample = Instant::now();
                    }
                    SimCommand::SetSpeed(s) => {
                        speed = s;
                    }
                    SimCommand::SetConfig(c) => config = c,
                    SimCommand::SetViewport(min_x, min_y, max_x, max_y) => {
                        world.set_viewport(min_x, min_y, max_x, max_y);
                    }
                    SimCommand::SpawnCreature => {
                        world.spawn_creature(&config);
                    }
                    SimCommand::SpawnFromTemplate(genome, energy) => {
                        world.spawn_from_template(&config, &genome, energy);
                    }
                    SimCommand::KillCreature(id) => {
                        world.kill_creature(id);
                    }
                    SimCommand::SetTrailDisabled(disabled) => {
                        world.trail_disabled = disabled;
                    }
                    SimCommand::SetTrailSpawnPaused(paused) => {
                        world.trail_spawn_paused = paused;
                    }
                    SimCommand::CaptureSnapshot(reply) => {
                        let ws = WorldSnapshot::capture(&world, &config);
                        let _ = reply.send(ws);
                    }
                    SimCommand::GenerateTerrain(params) => {
                        world.generate_terrain(&config, &params);
                        // 立即写盘，下次启动可自动复用
                        if let Err(e) = world.terrain.save_to_disk() {
                            eprintln!("保存地形失败: {}", e);
                        }
                    }
                    SimCommand::RestoreSnapshot(ws) => {
                        let (mut new_world, new_config) = ws.into_world();
                        // 重建 neural bridge
                        if new_config.neural_backend != "legacy" {
                            let bridge = crate::neural::thread::spawn_neural_thread(&new_config);
                            new_world.set_neural_bridge(bridge);
                        }
                        // 地形独立持久化：恢复存档时也从 terrain.json 读取
                        if let Some(loaded) = crate::world::TerrainMap::load_from_disk() {
                            new_world.terrain = loaded;
                        }
                        world = new_world;
                        config = new_config;
                        // 世界被替换，重置神经采样基线
                        last_inference_count = world.neural_inference_count();
                        last_neural_sample = Instant::now();
                        neural_speed_smooth = 0.0;
                    }
                    SimCommand::Shutdown => return,
                },
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // 采样神经吞吐并更新 EMA（bridge 模式才有意义，legacy 下 count 恒为 0）
        let has_bridge = world.has_neural_bridge();
        if has_bridge && !paused {
            let now = Instant::now();
            let elapsed_real = now.duration_since(last_neural_sample).as_secs_f64();
            if elapsed_real > 1e-6 {
                let current_count = world.neural_inference_count();
                let delta = current_count.saturating_sub(last_inference_count);
                // 每 inference 代表 SIM_DT 模拟秒的决策输出
                // raw_speed = (delta × SIM_DT) / elapsed_real，单位与 target speed 一致
                let raw_speed = delta as f64 * SIM_DT / elapsed_real;
                // EMA：α = 1 - exp(-Δt/τ)
                let alpha = 1.0 - (-elapsed_real / NEURAL_EMA_TAU).exp();
                neural_speed_smooth += alpha * (raw_speed - neural_speed_smooth);
                last_inference_count = current_count;
                last_neural_sample = now;
            }
        }

        // 计算本帧有效 speed：legacy 严格 = target；bridge = 反压探针
        let effective_speed = if has_bridge {
            let probe = (neural_speed_smooth * PROBE_RATIO).ceil().max(1.0);
            speed.min(probe)
        } else {
            speed
        };

        // 固定步长模拟：dt 恒定 SIM_DT，effective_speed 控制每帧 update 次数
        if !paused {
            speed_accumulator += effective_speed;
            let steps = speed_accumulator as usize;
            speed_accumulator -= steps as f64;
            for _ in 0..steps {
                world.update(SIM_DT, &config);
                sim_step_count += 1;
                actual_speed_steps += 1;
            }
        }

        // 每秒统计实际速率
        let elapsed = actual_speed_timer.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            actual_speed = actual_speed_steps as f64 * SIM_DT / elapsed;
            actual_speed_steps = 0;
            actual_speed_timer = Instant::now();
        }

        // 导出快照
        export_snapshot(
            &world,
            &config,
            &snapshot,
            speed,
            actual_speed,
            neural_speed_smooth,
            sim_step_count,
        );

        // 精确 sleep 到下一帧（恒定 ~30fps）
        next_tick += FRAME_INTERVAL;
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
        } else {
            // 落后了，重置到当前时间
            next_tick = now;
        }
    }
}

/// 从 World 导出 SimSnapshot 并写入共享内存
fn export_snapshot(
    world: &World,
    config: &Config,
    snapshot: &Arc<RwLock<SimSnapshot>>,
    target_speed: f64,
    actual_speed: f64,
    neural_speed: f64,
    sim_step_count: u64,
) {
    let world_stats = world.stats(config.species_similarity_threshold, config);
    let creature_species = world.get_render_data(config.species_similarity_threshold);
    let volcano_countdown = world.volcano_countdown(config);

    let snap = SimSnapshot {
        creatures: world.creatures.clone(),
        energy_particles: world.energy_particles.clone(),
        trail_points: world.trail_points.clone(),
        hot_springs: world.hot_springs.clone(),
        trail_disabled: world.trail_disabled,
        time: world.time,
        perf_stats: world.perf_stats.clone(),
        world_stats,
        creature_species,
        volcano_countdown,
        sim_step_count,
        stop_extinction_triggered: world.stop_extinction_triggered,
        target_speed,
        actual_speed,
        neural_speed,
        terrain: world.terrain.clone(),
    };

    if let Ok(mut guard) = snapshot.write() {
        *guard = snap;
    }
}
