use crate::config::Config;
use crate::render::{
    format_dhms, PanelAction, RenderContext, Selection, StatsPanel, VisibleWorldBounds, WorldCanvas,
};
use crate::snapshot::WorldSnapshot;
use crate::store::{CreatureTemplate, Store};
use crate::world::World;
use eframe::egui;
use std::fs::OpenOptions;
use std::io::Write;

/// 帧级性能统计
#[derive(Default)]
pub struct FramePerfStats {
    pub world_update_ms: f64,
    pub panel_update_ms: f64,
    pub render_ctx_ms: f64,
    pub render_ms: f64,
    pub frame_total_ms: f64,
    pub egui_overhead_ms: f64, // egui 框架开销
}

/// 主应用
pub struct CellWorldApp {
    world: World,
    config: Config,
    canvas: WorldCanvas,
    panel: StatsPanel,
    store: Store,
    paused: bool,
    speed: f64,
    last_update: std::time::Instant,
    fps: f64,
    frame_count: u32,
    fps_timer: std::time::Instant,
    // 日志记录
    last_log_time: f64,
    log_initialized: bool,
    // 选中状态
    selection: Selection,
    // 上一帧的可见范围
    last_visible_bounds: Option<VisibleWorldBounds>,
    // 渲染上下文缓存
    render_ctx_cache: Option<RenderContext>,
    last_render_ctx_update: std::time::Instant,
    // 帧级性能统计
    frame_perf: FramePerfStats,
    // 快照恢复：启动时如果存在存档，暂存在此
    pending_restore: Option<WorldSnapshot>,
    // 保存确认弹框
    snapshot_confirm_save: bool,
    // 是否启用画布渲染
    render_enabled: bool,
}

impl CellWorldApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 配置中文字体
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "chinese".to_owned(),
            egui::FontData::from_static(include_bytes!("C:/Windows/Fonts/msyh.ttc")),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "chinese".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("chinese".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let config = Config::load();
        let mut world = World::new(&config);
        let store = Store::new();
        world.dominant_species = store.dominant_species().clone();
        let now = std::time::Instant::now();

        // 初始化异步神经线程（非 legacy 模式）
        if config.neural_backend != "legacy" {
            let bridge = crate::neural::thread::spawn_neural_thread(&config);
            world.set_neural_bridge(bridge);
        }

        let initial_speed = config.initial_speed;
        let initial_scale = config.initial_scale;

        // 尝试加载存档
        let pending_restore = WorldSnapshot::load();

        Self {
            world,
            config,
            canvas: WorldCanvas::new(initial_scale),
            panel: StatsPanel::new(),
            store,
            paused: false,
            speed: initial_speed,
            last_update: now,
            fps: 0.0,
            frame_count: 0,
            fps_timer: now,
            last_log_time: 0.0,
            log_initialized: false,
            selection: Selection::None,
            last_visible_bounds: None,
            render_ctx_cache: None,
            last_render_ctx_update: now,
            frame_perf: FramePerfStats::default(),
            pending_restore,
            snapshot_confirm_save: false,
            render_enabled: true,
        }
    }
}

impl CellWorldApp {
    /// 记录统计数据到日志文件
    fn log_stats(&mut self) {
        let world_time = self.world.time;

        // 每10秒记录一次
        if world_time - self.last_log_time < 10.0 {
            return;
        }
        self.last_log_time = world_time;

        let stats = self.panel.stats();
        let log_path = "docs/LOG.md";

        // 首次写入时创建文件头
        if !self.log_initialized {
            // 原有数据日志
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(log_path)
            {
                let _ = writeln!(file, "# Cell World 运行日志\n");
                let _ = writeln!(file, "| 时间 | 生物 | 粒子 | 痕迹 | 总能 | 代 | 种群 | 寿命(均/中/长/短/死) | 行为(移动/吸收/咬/繁殖) |");
                let _ = writeln!(file, "|------|------|------|------|------|----|------|----------------------|------------------------|");
            }
            // 性能分析日志
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("docs/run.log")
            {
                let _ = writeln!(file, "# Cell World 性能分析日志\n");
                let _ = writeln!(file, "| 时间 | FPS | 生物 | 世界ms | 面板ms | 聚类ms | 渲染ms | egui | 帧总ms | 感知ms | 网络ms |");
                let _ = writeln!(file, "|------|-----|------|--------|--------|--------|--------|------|--------|--------|--------|");
            }
            self.log_initialized = true;
        }

        // 追加原有数据日志
        if let Ok(mut file) = OpenOptions::new().write(true).append(true).open(log_path) {
            let acts = &stats.action_counts;
            let death = &stats.death_age_stats;
            let _ = writeln!(
                file,
                "| {:.0} | {} | {} | {} | {:.0} | {} | {} | {:.1}/{:.1}/{:.1}/{:.1}/{} | {}/{}/{}/{} |",
                stats.time,
                stats.creature_count,
                stats.energy_particle_count,
                stats.trail_count,
                stats.total_energy,
                stats.max_generation,
                stats.clan_count,
                death.avg, death.median, death.max, death.min, death.count,
                acts[0], acts[1], acts[2], acts[3]
            );
        }

        // 追加性能分析日志
        let perf = &self.world.perf_stats;
        let fperf = &self.frame_perf;
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .append(true)
            .open("docs/run.log")
        {
            let _ = writeln!(
                file,
                "| {:.0} | {:.0} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
                stats.time,
                stats.fps,
                perf.creature_count,
                fperf.world_update_ms,
                fperf.panel_update_ms,
                fperf.render_ctx_ms,
                fperf.render_ms,
                fperf.egui_overhead_ms,
                fperf.frame_total_ms,
                perf.perceive_ms,
                perf.snn_ms
            );
        }
    }
}

impl CellWorldApp {
    /// 自动保存优势种
    /// 进化适应度：跨时间可比的多维综合评分
    fn evolutionary_fitness(avg_age: f64, avg_energy: f64, max_generation: usize, population_ratio: f64) -> f64 {
        // 生存力：平均寿命，上限300s
        let survival = (avg_age.min(300.0) / 300.0).max(0.0);
        // 资源力：平均能量，上限500
        let prosperity = (avg_energy.min(500.0) / 500.0).max(0.0);
        // 进化深度：最大代数，上限100
        let depth = ((max_generation as f64).min(100.0) / 100.0).max(0.0);
        // 统治力：种群占比，已归一化
        let dominance = population_ratio.clamp(0.0, 1.0);

        0.30 * survival + 0.25 * prosperity + 0.25 * depth + 0.20 * dominance
    }

    fn auto_save_dominant(&mut self) {
        let candidate = match self.panel.stats().dominant_candidate.clone() {
            Some(c) => c,
            None => return,
        };

        let version = env!("CARGO_PKG_VERSION");
        let candidate_fitness = Self::evolutionary_fitness(
            candidate.avg_age,
            candidate.avg_energy,
            candidate.max_generation,
            candidate.population_ratio,
        );

        // 扫描所有自动记录模板，计算适应度和相似度
        struct AutoEntry {
            name: String,
            fitness: f64,
            similarity: f64,
        }
        let auto_entries: Vec<AutoEntry> = self
            .store
            .templates()
            .iter()
            .filter(|t| t.auto_recorded == Some(true))
            .map(|t| {
                let fitness = Self::evolutionary_fitness(
                    t.avg_age.unwrap_or(0.0),
                    t.avg_energy.unwrap_or(0.0),
                    t.max_generation.unwrap_or(0),
                    t.population_ratio.unwrap_or(0.0),
                );
                let similarity = candidate.genome.similarity(&t.genome);
                AutoEntry {
                    name: t.name.clone(),
                    fitness,
                    similarity,
                }
            })
            .collect();

        // 查找同种（相似度≥0.9）中适应度最高的
        let same_species = auto_entries
            .iter()
            .filter(|e| e.similarity >= 0.9)
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap());

        // 查找所有自动记录中适应度最低的
        let weakest = auto_entries
            .iter()
            .min_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap());

        let auto_count = auto_entries.len();
        const MAX_AUTO: usize = 10;

        let make_template = |name: String, candidate: &crate::world::DominantCandidate, version: &str, time: f64| {
            CreatureTemplate {
                name,
                genome: candidate.genome.clone(),
                initial_energy: candidate.avg_energy,
                version: Some(version.to_string()),
                score: Some(candidate.score),
                population_ratio: Some(candidate.population_ratio),
                avg_energy: Some(candidate.avg_energy),
                avg_age: Some(candidate.avg_age),
                max_generation: Some(candidate.max_generation),
                recorded_at: Some(time),
                auto_recorded: Some(true),
            }
        };

        if let Some(existing) = same_species {
            // 同种已存在：仅当进化适应度更优时覆盖
            if candidate_fitness > existing.fitness {
                eprintln!(
                    "[优势种] 同种更新 '{}': 适应度 {:.3} → {:.3} (寿命:{:.0}s 能量:{:.0} 代:{} 占比:{:.0}%)",
                    existing.name, existing.fitness, candidate_fitness,
                    candidate.avg_age, candidate.avg_energy, candidate.max_generation,
                    candidate.population_ratio * 100.0
                );
                let template = make_template(existing.name.clone(), &candidate, version, self.world.time);
                if let Err(e) = self.store.save(template) {
                    eprintln!("自动保存优势种失败: {}", e);
                }
            }
        } else {
            // 新种：需要与已有模板比较
            if auto_count < MAX_AUTO {
                // 尚有空位：适应度达到最低门槛即可保存
                if candidate_fitness >= 0.10 {
                    let now = chrono::Local::now();
                    let name = format!("v{}_{}", version, now.format("%m%d_%H%M"));
                    eprintln!(
                        "[优势种] 新种保存 '{}': 适应度 {:.3} (寿命:{:.0}s 能量:{:.0} 代:{} 占比:{:.0}%)",
                        name, candidate_fitness,
                        candidate.avg_age, candidate.avg_energy, candidate.max_generation,
                        candidate.population_ratio * 100.0
                    );
                    let template = make_template(name, &candidate, version, self.world.time);
                    if let Err(e) = self.store.save(template) {
                        eprintln!("自动保存优势种失败: {}", e);
                    }
                }
            } else if let Some(w) = weakest {
                // 已满：仅当新种适应度显著优于最弱者（>10%）时替换
                if candidate_fitness > w.fitness * 1.1 {
                    eprintln!(
                        "[优势种] 淘汰 '{}' (适应度:{:.3})，新种适应度:{:.3} (寿命:{:.0}s 能量:{:.0} 代:{} 占比:{:.0}%)",
                        w.name, w.fitness, candidate_fitness,
                        candidate.avg_age, candidate.avg_energy, candidate.max_generation,
                        candidate.population_ratio * 100.0
                    );
                    let weak_name = w.name.clone();
                    self.store.delete(&weak_name);
                    let now = chrono::Local::now();
                    let name = format!("v{}_{}", version, now.format("%m%d_%H%M"));
                    let template = make_template(name, &candidate, version, self.world.time);
                    if let Err(e) = self.store.save(template) {
                        eprintln!("自动保存优势种失败: {}", e);
                    }
                }
            }
        }
    }
}

/// 配置项辅助：f64 拖拽值
fn config_drag_f64(
    ui: &mut egui::Ui,
    title: &str,
    desc: &str,
    value: &mut f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    let changed = ui
        .horizontal(|ui| {
            ui.label(title);
            let r = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::DragValue::new(value).speed(speed).range(range))
                        .changed()
                })
                .inner;
            r
        })
        .inner;
    if !desc.is_empty() {
        ui.label(
            egui::RichText::new(desc)
                .color(egui::Color32::from_gray(110))
                .size(10.0),
        );
    }
    changed
}

/// 配置项辅助：usize 拖拽值
fn config_drag_usize(
    ui: &mut egui::Ui,
    title: &str,
    desc: &str,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
) -> bool {
    let changed = ui
        .horizontal(|ui| {
            ui.label(title);
            let r = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::DragValue::new(value).speed(0.1).range(range))
                        .changed()
                })
                .inner;
            r
        })
        .inner;
    if !desc.is_empty() {
        ui.label(
            egui::RichText::new(desc)
                .color(egui::Color32::from_gray(110))
                .size(10.0),
        );
    }
    changed
}

impl CellWorldApp {
    fn render_settings_inline(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("⚙ 设置");
        {
            ui.checkbox(&mut self.world.trail_disabled, "禁用痕迹系统");

            let c = &mut self.config;
            let mut changed = false;

            ui.collapsing("进化与竞争", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "变异率",
                    "所有NEAT变异共用此概率",
                    &mut c.mutation_rate,
                    0.01,
                    0.01..=0.5,
                );
                changed |= config_drag_usize(
                    ui,
                    "初始连接min",
                    "新生物最少神经连接数",
                    &mut c.initial_connections_min,
                    1..=20,
                );
                changed |= config_drag_usize(
                    ui,
                    "初始连接max",
                    "新生物最多神经连接数",
                    &mut c.initial_connections_max,
                    2..=30,
                );
                changed |= config_drag_f64(
                    ui,
                    "种族相似阈值",
                    "基因相似度>=此值视为同族",
                    &mut c.species_similarity_threshold,
                    0.01,
                    0.5..=1.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "繁殖冷却",
                    "两次繁殖间最短间隔(秒)",
                    &mut c.reproduce_cooldown,
                    0.5,
                    1.0..=60.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "速度战力权重",
                    "移速对战力的加成系数",
                    &mut c.combat_speed_weight,
                    0.01,
                    0.0..=2.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "同族援助权重",
                    "附近同族对战力的加成，越大群体越强",
                    &mut c.combat_ally_weight,
                    0.1,
                    0.0..=10.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "咬转移率",
                    "咬合时能量转移比例",
                    &mut c.bite_transfer_rate,
                    0.01,
                    0.01..=1.0,
                );
            });

            ui.collapsing("环境压力", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "散热系数",
                    "体温逸散=此值*周长*heat_factor*dt",
                    &mut c.heat_dissipation_coefficient,
                    0.001,
                    0.0..=1.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "能量分母",
                    "nearby_energy归一化分母，越大需更多聚集才降温",
                    &mut c.energy_denominator,
                    50.0,
                    100.0..=5000.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "散热下限",
                    "聚集区散热最低比例(0~1)",
                    &mut c.heat_floor,
                    0.01,
                    0.0..=1.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "代谢指数",
                    "energy^此值，越大大体型越重",
                    &mut c.metabolism_exponent,
                    0.1,
                    1.0..=5.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "年龄代谢倍率",
                    "age*此值=额外代谢倍率",
                    &mut c.age_metabolism_factor,
                    0.001,
                    0.0..=0.2,
                );
                changed |= config_drag_f64(
                    ui,
                    "移动消耗",
                    "每单位距离消耗*速度",
                    &mut c.move_cost,
                    0.0001,
                    0.0001..=0.01,
                );
            });

            ui.collapsing("感知系统", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "视觉半径",
                    "眼睛最大探测距离(px)",
                    &mut c.vision_range,
                    1.0,
                    50.0..=500.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "扫描速度",
                    "眼睛扫描速度(度/秒)",
                    &mut c.eye_scan_speed,
                    10.0,
                    50.0..=1000.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "接触距离",
                    "嘴巴/接触判定距离(px)",
                    &mut c.contact_range,
                    0.5,
                    5.0..=50.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "嘴巴冷却",
                    "咬合动作冷却时间(秒)",
                    &mut c.mouth_cooldown,
                    0.01,
                    0.1..=5.0,
                );
            });

            ui.collapsing("生物基础", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "初始能量",
                    "新生成生物的能量",
                    &mut c.initial_energy,
                    1.0,
                    10.0..=500.0,
                );
                changed |= config_drag_usize(
                    ui,
                    "最小生物数",
                    "低于此数自动补充",
                    &mut c.min_creatures,
                    5..=200,
                );
                changed |= ui
                    .horizontal(|ui| {
                        ui.label("算力系数");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut c.compute_energy_factor)
                                    .speed(0.1)
                                    .range(0.0..=200.0)
                                    .max_decimals(1),
                            )
                            .changed()
                        })
                        .inner
                    })
                    .inner;
                ui.label(
                    egui::RichText::new("CPU算力转能量，0=禁用")
                        .color(egui::Color32::from_gray(110))
                        .size(10.0),
                );
            });

            ui.collapsing("痕迹点", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "衰减率",
                    "每秒能量衰减比例",
                    &mut c.trail_decay_rate,
                    0.01,
                    0.01..=0.5,
                );
                changed |= config_drag_f64(
                    ui,
                    "抑制半径",
                    "范围内有他人痕迹则不产生(px)",
                    &mut c.trail_suppress_radius,
                    1.0,
                    5.0..=100.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "生成间隔",
                    "每个生物独立计时(秒)",
                    &mut c.trail_emit_interval,
                    0.01,
                    0.05..=2.0,
                );
            });

            ui.collapsing("其他", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "优势种最低年龄",
                    "种群最老成员须达此年龄",
                    &mut c.dominant_min_age,
                    10.0,
                    100.0..=2000.0,
                );
            });

            ui.separator();
            if ui
                .button("重置默认")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *c = Config::default();
                changed = true;
            }

            if changed {
                c.save();
            }
        }
    }
}

impl CellWorldApp {
    fn render_energy_trend(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("能量趋势");
        let history = &self.panel.energy_history;
        let chart_width_f32 = ui.available_width().min(480.0);
        let chart_height_f32 = 100.0_f32;
        let (response, painter) = ui.allocate_painter(
            egui::vec2(chart_width_f32, chart_height_f32),
            egui::Sense::hover(),
        );
        let rect = response.rect;

        // 背景
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

        if history.len() >= 2 {
            let t_min = history.first().unwrap().0;
            let t_max = history.last().unwrap().0;
            let t_range = (t_max - t_min).max(1.0);

            // Y 轴范围：同时考虑总能量、生命能量和粒子理论总能量
            let (e_min, e_max) = history.iter().fold(
                (f64::MAX, f64::MIN),
                |(lo, hi), &(_, total, creature, particle_init)| {
                    (
                        lo.min(total).min(creature).min(particle_init),
                        hi.max(total).max(creature).max(particle_init),
                    )
                },
            );
            let e_min = e_min * 0.9;
            let e_max = e_max * 1.1;
            let e_range = (e_max - e_min).max(1.0);

            let color_total = egui::Color32::from_rgb(100, 200, 255);
            let color_life = egui::Color32::from_rgb(100, 255, 130);
            let color_particle_init = egui::Color32::from_rgb(255, 180, 80);

            // 绘制粒子理论总能量曲线
            let particle_init_pts: Vec<egui::Pos2> = history
                .iter()
                .map(|&(t, _, _, pi)| {
                    let x = rect.left() + ((t - t_min) / t_range * chart_width_f32 as f64) as f32;
                    let y =
                        rect.bottom() - ((pi - e_min) / e_range * chart_height_f32 as f64) as f32;
                    egui::pos2(x, y.clamp(rect.top(), rect.bottom()))
                })
                .collect();
            for pair in particle_init_pts.windows(2) {
                painter.line_segment(
                    [pair[0], pair[1]],
                    egui::Stroke::new(1.5, color_particle_init),
                );
            }
            if let Some(&last) = particle_init_pts.last() {
                painter.circle_filled(last, 3.0, color_particle_init);
            }

            // 绘制总能量曲线
            let total_pts: Vec<egui::Pos2> = history
                .iter()
                .map(|&(t, e, _, _)| {
                    let x = rect.left() + ((t - t_min) / t_range * chart_width_f32 as f64) as f32;
                    let y =
                        rect.bottom() - ((e - e_min) / e_range * chart_height_f32 as f64) as f32;
                    egui::pos2(x, y.clamp(rect.top(), rect.bottom()))
                })
                .collect();
            for pair in total_pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5, color_total));
            }
            if let Some(&last) = total_pts.last() {
                painter.circle_filled(last, 3.0, color_total);
            }

            // 绘制生命能量曲线
            let life_pts: Vec<egui::Pos2> = history
                .iter()
                .map(|&(t, _, c, _)| {
                    let x = rect.left() + ((t - t_min) / t_range * chart_width_f32 as f64) as f32;
                    let y =
                        rect.bottom() - ((c - e_min) / e_range * chart_height_f32 as f64) as f32;
                    egui::pos2(x, y.clamp(rect.top(), rect.bottom()))
                })
                .collect();
            for pair in life_pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5, color_life));
            }
            if let Some(&last) = life_pts.last() {
                painter.circle_filled(last, 3.0, color_life);
            }

            // Y轴标注
            let label_color = egui::Color32::from_gray(160);
            painter.text(
                egui::pos2(rect.left() + 2.0, rect.top() + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{:.0}", e_max),
                egui::FontId::proportional(10.0),
                label_color,
            );
            painter.text(
                egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.0}", e_min),
                egui::FontId::proportional(10.0),
                label_color,
            );

            // 时间标注
            painter.text(
                egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
                egui::Align2::RIGHT_BOTTOM,
                format_dhms(t_max),
                egui::FontId::proportional(10.0),
                label_color,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "采集数据中...",
                egui::FontId::proportional(12.0),
                egui::Color32::from_gray(120),
            );
        }

        // 图例 + 当前值
        let stats = self.panel.stats();
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(255, 180, 80),
                format!("投放/m: {:.0}", stats.theoretical_energy),
            );
            ui.colored_label(
                egui::Color32::from_rgb(100, 200, 255),
                format!("粒子: {:.0}", stats.total_energy),
            );
            ui.colored_label(
                egui::Color32::from_rgb(100, 255, 130),
                format!("生命: {:.0}", stats.creature_energy),
            );
        });
    }
}

impl CellWorldApp {
    fn render_energy_settings_inline(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("🌋 能量源设置");
        {
            let c = &mut self.config;
            let mut changed = false;

            ui.collapsing("火山参数", |ui| {
                changed |= config_drag_f64(
                    ui,
                    "火山半径",
                    "喷发散布半径(px)",
                    &mut c.volcano_radius,
                    1.0,
                    100.0..=5000.0,
                );
                changed |= config_drag_usize(
                    ui,
                    "火山粒子数",
                    "每次喷发粒子数",
                    &mut c.volcano_count,
                    10..=2000,
                );
                changed |= config_drag_f64(
                    ui,
                    "火山间隔",
                    "喷发间隔(秒)",
                    &mut c.volcano_interval,
                    0.1,
                    5.0..=120.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "火山粒子能量",
                    "每个粒子能量",
                    &mut c.volcano_particle_energy,
                    0.1,
                    5.0..=1000.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "火山衰减率",
                    "粒子能量每秒衰减比例",
                    &mut c.volcano_decay_rate,
                    0.001,
                    0.001..=0.1,
                );
                changed |= config_drag_f64(
                    ui,
                    "火山杀伤半径",
                    "落地时杀死半径内生物(px)",
                    &mut c.volcano_kill_radius,
                    0.5,
                    1.0..=50.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "落地杀伤系数",
                    "落地杀伤乘数(0.1~3.0)",
                    &mut c.landing_damage_multiplier,
                    0.1,
                    0.1..=3.0,
                );
                ui.separator();
                ui.label("正弦周期调制");
                changed |= config_drag_f64(
                    ui,
                    "间隔周期",
                    "火山间隔正弦周期(秒)，0=关闭",
                    &mut c.volcano_interval_cycle,
                    10.0,
                    0.0..=3600.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "间隔振幅",
                    "间隔振幅比(0~0.9)",
                    &mut c.volcano_interval_amplitude,
                    0.01,
                    0.0..=0.9,
                );
                changed |= config_drag_f64(
                    ui,
                    "能量周期",
                    "火山能量正弦周期(秒)，0=关闭",
                    &mut c.volcano_energy_cycle,
                    10.0,
                    0.0..=3600.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "能量振幅",
                    "能量振幅比(0~0.9)",
                    &mut c.volcano_energy_amplitude,
                    0.01,
                    0.0..=0.9,
                );
            });

            // 温泉信息
            let active_springs = self.world.hot_springs.iter().filter(|s| s.alive).count();
            ui.separator();
            ui.strong(format!(
                "♨ 温泉 ({}/{})",
                active_springs, c.spring_max_count
            ));

            ui.collapsing("温泉参数", |ui| {
                changed |= config_drag_usize(
                    ui,
                    "最大数量",
                    "同时活跃温泉数上限",
                    &mut c.spring_max_count,
                    1..=20,
                );
                changed |= config_drag_f64(
                    ui,
                    "生成间隔",
                    "新温泉出现间隔(秒)",
                    &mut c.spring_spawn_interval,
                    1.0,
                    10.0..=1200.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "寿命",
                    "温泉存在时长(秒)",
                    &mut c.spring_lifetime,
                    1.0,
                    60.0..=3600.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "喷出间隔",
                    "粒子喷出间隔(秒)",
                    &mut c.spring_emit_interval,
                    0.1,
                    0.1..=10.0,
                );
                changed |= config_drag_usize(
                    ui,
                    "喷出粒子数",
                    "每次喷出粒子数",
                    &mut c.spring_emit_count,
                    1..=50,
                );
                changed |= config_drag_f64(
                    ui,
                    "粒子能量",
                    "温泉粒子能量",
                    &mut c.spring_particle_energy,
                    0.1,
                    1.0..=500.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "喷出半径",
                    "粒子散布范围(px)",
                    &mut c.spring_radius,
                    1.0,
                    10.0..=500.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "粒子衰减率",
                    "温泉粒子每秒衰减比例",
                    &mut c.spring_decay_rate,
                    0.001,
                    0.001..=0.1,
                );
                changed |= config_drag_f64(
                    ui,
                    "最小间距",
                    "温泉间最小距离(px)",
                    &mut c.spring_min_distance,
                    1.0,
                    50.0..=1000.0,
                );
                changed |= config_drag_f64(
                    ui,
                    "最大距离",
                    "链式扩散最大距离(px)",
                    &mut c.spring_max_distance,
                    1.0,
                    100.0..=2000.0,
                );
            });

            if changed {
                c.save();
            }
        }
    }
}

impl eframe::App for CellWorldApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 启动恢复弹框
        if self.pending_restore.is_some() {
            let mut chose_restore = false;
            let mut chose_new = false;
            egui::Window::new("发现存档")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("检测到世界存档，是否恢复？");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("恢复存档").clicked() {
                            chose_restore = true;
                        }
                        if ui.button("新开始").clicked() {
                            chose_new = true;
                        }
                    });
                });
            if chose_restore {
                if let Some(snapshot) = self.pending_restore.take() {
                    let (mut world, config) = snapshot.into_world();
                    world.dominant_species = self.store.dominant_species().clone();
                    // 重建 neural bridge
                    if config.neural_backend != "legacy" {
                        let bridge = crate::neural::thread::spawn_neural_thread(&config);
                        world.set_neural_bridge(bridge);
                    }
                    self.config = config;
                    self.world = world;
                    self.render_ctx_cache = None;
                }
            }
            if chose_new {
                self.pending_restore = None;
            }
            // 有弹框时暂停世界逻辑
            ctx.request_repaint();
            return;
        }

        // 保存确认弹框
        if self.snapshot_confirm_save {
            let mut chose_save = false;
            let mut chose_cancel = false;
            let has_existing = WorldSnapshot::exists();
            egui::Window::new("保存存档")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if has_existing {
                        ui.label("将覆盖现有存档，确定保存？");
                    } else {
                        ui.label("保存当前世界存档？");
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("确定").clicked() {
                            chose_save = true;
                        }
                        if ui.button("取消").clicked() {
                            chose_cancel = true;
                        }
                    });
                });
            if chose_save {
                let snapshot = WorldSnapshot::capture(&self.world, &self.config);
                if let Err(e) = snapshot.save() {
                    eprintln!("保存快照失败: {}", e);
                }
                self.snapshot_confirm_save = false;
            }
            if chose_cancel {
                self.snapshot_confirm_save = false;
            }
        }

        // 计算 delta time
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;

        // FPS 计算
        self.frame_count += 1;
        let fps_elapsed = now.duration_since(self.fps_timer).as_secs_f64();
        if fps_elapsed >= 1.0 {
            self.fps = self.frame_count as f64 / fps_elapsed;
            self.frame_count = 0;
            self.fps_timer = now;
        }

        // FPS低于30时自动暂停痕迹生成（不影响已有痕迹的渲染/衰减/吸收）
        self.world.trail_spawn_paused = self.fps > 0.0 && self.fps < 30.0;

        // 使用上一帧的可见范围更新视窗
        // 第一帧时 last_visible_bounds 为 None，跳过更新，等待渲染获取视窗大小
        let t_world = std::time::Instant::now();
        if let Some(bounds) = self.last_visible_bounds {
            self.world
                .set_viewport(bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);

            // 更新世界（如果未暂停）
            // 高倍速时拆分子步，防止大 dt 导致扫描跳过目标、冷却判定粗糙
            if !self.paused {
                let total_dt = dt * self.speed;
                let max_step = 0.2; // 单步最大 200ms，≥6x 才触发
                if total_dt > max_step {
                    let steps = ((total_dt / max_step).ceil() as usize).min(5);
                    let step_dt = total_dt / steps as f64;
                    for _ in 0..steps {
                        self.world.update(step_dt, &self.config);
                    }
                } else {
                    self.world.update(total_dt, &self.config);
                }
            }
        }
        self.frame_perf.world_update_ms = t_world.elapsed().as_secs_f64() * 1000.0;

        // 更新面板缓存
        let t_panel = std::time::Instant::now();
        self.panel.update(
            &self.world,
            &self.config,
            self.config.species_similarity_threshold,
            self.fps,
            now,
        );
        self.frame_perf.panel_update_ms = t_panel.elapsed().as_secs_f64() * 1000.0;

        // 每10秒记录一次日志
        self.log_stats();

        // 自动保存优势种
        self.auto_save_dominant();

        // 侧边栏面板
        let mut panel_action = PanelAction::default();
        let mut selection_action = PanelAction::default();
        let mut gene_action = PanelAction::default();

        let old_speed = self.speed;
        egui::SidePanel::right("panel")
            .min_width(400.0)
            .show(ctx, |ui| {
                // 固定区域：统计面板 + 能量趋势 + 选中信息
                panel_action = self.panel.render(
                    ui,
                    self.fps,
                    self.canvas.scale,
                    &mut self.speed,
                    &mut self.paused,
                    &mut self.render_enabled,
                    &mut self.config,
                    &self.store,
                );

                // 能量趋势常驻显示
                self.render_energy_trend(ui);

                // 显示选中信息
                selection_action = self
                    .panel
                    .render_selection(ui, &self.selection, &self.world);

                // 滚动区域：基因库/能量/配置面板（互斥）
                if self.panel.templates_open || self.panel.settings_open || self.panel.energy_settings_open {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let margin = egui::Margin {
                            right: 6.0,
                            ..Default::default()
                        };
                        egui::Frame::none().inner_margin(margin).show(ui, |ui| {
                            if self.panel.templates_open {
                                gene_action = self.panel.render_gene_library(ui, &self.store);
                            }
                            if self.panel.settings_open {
                                self.render_settings_inline(ui);
                            }
                            if self.panel.energy_settings_open {
                                self.render_energy_settings_inline(ui);
                            }
                        });
                    });
                }
            });

        // 速度变化时同步到配置文件
        if (self.speed - old_speed).abs() > f64::EPSILON {
            self.config.initial_speed = self.speed;
            self.config.save();
        }

        // 合并基因库操作
        if gene_action.spawn.is_some() {
            panel_action.spawn = gene_action.spawn;
        }
        if gene_action.delete_template.is_some() {
            panel_action.delete_template = gene_action.delete_template;
        }
        if gene_action.clear_dominant {
            panel_action.clear_dominant = true;
        }
        if gene_action.save_clan.is_some() {
            panel_action.save_clan = gene_action.save_clan;
        }

        // 处理保存快照
        if panel_action.save_snapshot {
            self.snapshot_confirm_save = true;
        }

        // 处理添加生物按钮（每次添加5个）
        if let Some(template_name) = panel_action.spawn {
            for _ in 0..5 {
                match &template_name {
                    None => {
                        // 随机生成
                        self.world.spawn_creature(&self.config);
                    }
                    Some(name) => {
                        // 从模板生成
                        if let Some(template) = self.store.get(name) {
                            self.world.spawn_from_template(
                                &self.config,
                                &template.genome,
                                template.initial_energy,
                            );
                        }
                    }
                }
            }
        }

        // 处理删除选中
        if selection_action.delete_selected {
            if let Selection::Creature(id) = self.selection {
                self.world.kill_creature(id);
                self.selection = Selection::None;
            }
        }

        // 处理保存选中
        if let Some(name) = selection_action.save_selected {
            if let Selection::Creature(id) = self.selection {
                if let Some(creature) = self.world.creatures.iter().find(|c| c.id == id && c.alive)
                {
                    let saved_name = name.clone();
                    let template = CreatureTemplate {
                        name,
                        genome: creature.genome.clone(),
                        initial_energy: creature.energy,
                        version: None,
                        score: None,
                        population_ratio: None,
                        avg_energy: None,
                        avg_age: None,
                        max_generation: None,
                        recorded_at: None,
                        auto_recorded: None,
                    };
                    if let Err(e) = self.store.save(template) {
                        eprintln!("保存失败: {}", e);
                    } else {
                        // 保存成功后，自动选中新模板
                        if let Some(idx) = self.store.names().iter().position(|n| *n == saved_name)
                        {
                            self.panel.selected_template = idx + 1; // +1 因为第0项是"随机"
                        }
                    }
                }
            }
        }

        // 处理删除模板
        if let Some(name) = panel_action.delete_template {
            self.store.delete(&name);
            self.panel.reset_template_selection();
        }

        // 保存种族代表基因
        if let Some(clan_hash) = panel_action.save_clan {
            // 找到该族中能量最高的活生物作为代表
            if let Some(representative) = self
                .world
                .creatures
                .iter()
                .filter(|c| c.alive && c.clan_hash == clan_hash)
                .max_by(|a, b| a.energy.partial_cmp(&b.energy).unwrap())
            {
                let name = format!("族_{:08X}", clan_hash);
                let template = CreatureTemplate {
                    name: name.clone(),
                    genome: representative.genome.clone(),
                    initial_energy: representative.energy,
                    version: None,
                    score: None,
                    population_ratio: None,
                    avg_energy: None,
                    avg_age: None,
                    max_generation: None,
                    recorded_at: None,
                    auto_recorded: None,
                };
                if let Err(e) = self.store.save(template) {
                    eprintln!("保存族失败: {}", e);
                }
            }
        }

        // 处理清空优势种
        if panel_action.clear_dominant {
            self.store.clear_dominant();
        }

        // 同步帧率到画布（用于自适应渲染质量）
        self.canvas.fps = self.fps;

        // 主画布
        let t_central_panel = std::time::Instant::now();
        let mut render_ctx_time = 0.0;
        let mut render_time = 0.0;
        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.render_enabled {
                // 渲染关闭：仅计算视口范围，跳过所有渲染
                let screen_rect = ui.available_rect_before_wrap();
                let bounds = self.canvas.get_visible_world_bounds(screen_rect);
                self.last_visible_bounds = Some(bounds);
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("渲染已暂停 - 仅数据模拟")
                            .size(20.0)
                            .color(egui::Color32::from_gray(80)),
                    );
                });
                return;
            }
            // 每1秒（真实时间）更新一次渲染上下文（避免频繁计算O(n²)的种族聚类）
            if self.render_ctx_cache.is_none()
                || now
                    .duration_since(self.last_render_ctx_update)
                    .as_secs_f64()
                    >= 1.0
            {
                let t_ctx = std::time::Instant::now();
                let creature_species = self
                    .world
                    .get_render_data(self.config.species_similarity_threshold);
                self.render_ctx_cache = Some(RenderContext { creature_species });
                self.last_render_ctx_update = now;
                render_ctx_time = t_ctx.elapsed().as_secs_f64() * 1000.0;
            }
            let render_ctx = self.render_ctx_cache.as_ref().unwrap();
            let t_render = std::time::Instant::now();
            let bounds = self.canvas.render(
                ui,
                &self.world,
                &mut self.selection,
                render_ctx,
                &self.config,
            );
            render_time = t_render.elapsed().as_secs_f64() * 1000.0;
            self.last_visible_bounds = Some(bounds);
        });
        let central_panel_time = t_central_panel.elapsed().as_secs_f64() * 1000.0;
        self.frame_perf.render_ctx_ms = render_ctx_time;
        self.frame_perf.render_ms = render_time;
        // egui 开销 = CentralPanel 总时间 - 我们测量的代码时间
        self.frame_perf.egui_overhead_ms = central_panel_time - render_ctx_time - render_time;
        self.frame_perf.frame_total_ms =
            self.frame_perf.world_update_ms + self.frame_perf.panel_update_ms + central_panel_time;

        // 持续刷新
        ctx.request_repaint();
    }
}
