use eframe::egui;
use std::fs::OpenOptions;
use std::io::Write;
use crate::config::Config;
use crate::store::{Store, CreatureTemplate};
use crate::world::World;
use crate::render::{WorldCanvas, StatsPanel, Selection, VisibleWorldBounds, PanelAction, RenderContext, format_dhms};

/// 帧级性能统计
#[derive(Default)]
pub struct FramePerfStats {
    pub world_update_ms: f64,
    pub panel_update_ms: f64,
    pub render_ctx_ms: f64,
    pub render_ms: f64,
    pub frame_total_ms: f64,
    pub egui_overhead_ms: f64,  // egui 框架开销
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
}

impl CellWorldApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 配置中文字体
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "chinese".to_owned(),
            egui::FontData::from_static(include_bytes!("C:/Windows/Fonts/msyh.ttc")),
        );
        fonts.families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "chinese".to_owned());
        fonts.families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("chinese".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let config = Config::load();
        let world = World::new(&config);
        let store = Store::new();
        let now = std::time::Instant::now();

        let initial_speed = config.initial_speed;
        let initial_scale = config.initial_scale;

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
                let _ = writeln!(file, "| 时间 | 生物 | 粒子 | 痕迹 | 总能 | 代 | 种群 | 寿命(均/中/长/短/死) | 行为(移动/吸收/咬/喂/繁殖) |");
                let _ = writeln!(file, "|------|------|------|------|------|----|------|----------------------|----------------------------|");
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
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .append(true)
            .open(log_path)
        {
            let acts = &stats.action_counts;
            let death = &stats.death_age_stats;
            let _ = writeln!(
                file,
                "| {:.0} | {} | {} | {} | {:.0} | {} | {} | {:.1}/{:.1}/{:.1}/{:.1}/{} | {}/{}/{}/{}/{} |",
                stats.time,
                stats.creature_count,
                stats.energy_particle_count,
                stats.trail_count,
                stats.total_energy,
                stats.max_generation,
                stats.species_count,
                death.avg, death.median, death.max, death.min, death.count,
                acts[0], acts[1], acts[2], acts[3], acts[4]
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
                perf.forward_ms
            );
        }
    }
}

impl CellWorldApp {
    /// 自动保存优势种
    fn auto_save_dominant(&mut self) {
        let candidate = match self.panel.stats().dominant_candidate.clone() {
            Some(c) => c,
            None => return,
        };

        let version = env!("CARGO_PKG_VERSION");

        // 检查是否与已有自动记录的模板相似
        let mut existing_match: Option<(String, f64)> = None;
        for template in self.store.templates() {
            if template.auto_recorded != Some(true) {
                continue;
            }
            let sim = candidate.genome.similarity(&template.genome);
            if sim >= 0.9 {
                existing_match = Some((template.name.clone(), template.score.unwrap_or(0.0)));
                break;
            }
        }

        match existing_match {
            Some((name, old_score)) => {
                // 同种且 score 更高时覆盖
                if candidate.score > old_score {
                    let template = CreatureTemplate {
                        name,
                        genome: candidate.genome,
                        initial_energy: candidate.avg_energy,
                        version: Some(version.to_string()),
                        score: Some(candidate.score),
                        population_ratio: Some(candidate.population_ratio),
                        avg_energy: Some(candidate.avg_energy),
                        avg_age: Some(candidate.avg_age),
                        max_generation: Some(candidate.max_generation),
                        recorded_at: Some(self.world.time),
                        auto_recorded: Some(true),
                    };
                    if let Err(e) = self.store.save(template) {
                        eprintln!("自动保存优势种失败: {}", e);
                    }
                }
            }
            None => {
                // 新种，创建新记录
                let name = format!(
                    "优势种_v{}_{:08X}",
                    version,
                    candidate.genome_hash
                );
                let template = CreatureTemplate {
                    name,
                    genome: candidate.genome,
                    initial_energy: candidate.avg_energy,
                    version: Some(version.to_string()),
                    score: Some(candidate.score),
                    population_ratio: Some(candidate.population_ratio),
                    avg_energy: Some(candidate.avg_energy),
                    avg_age: Some(candidate.avg_age),
                    max_generation: Some(candidate.max_generation),
                    recorded_at: Some(self.world.time),
                    auto_recorded: Some(true),
                };
                if let Err(e) = self.store.save(template) {
                    eprintln!("自动保存优势种失败: {}", e);
                }
            }
        }
    }
}

/// 配置项辅助：f64 拖拽值
fn config_drag_f64(ui: &mut egui::Ui, label: &str, value: &mut f64, speed: f64, range: std::ops::RangeInclusive<f64>) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(speed).range(range)).changed()
    }).inner
}

/// 配置项辅助：usize 拖拽值
fn config_drag_usize(ui: &mut egui::Ui, label: &str, value: &mut usize, range: std::ops::RangeInclusive<usize>) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.1).range(range)).changed()
    }).inner
}

impl CellWorldApp {
    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.panel.settings_open;
        let screen = ctx.screen_rect();
        let win_height = 500.0;
        let win_width = 350.0;
        let center_x = (screen.width() - win_width) / 2.0;
        let center_y = (screen.height() - win_height) / 2.0;
        egui::Window::new("设置")
            .open(&mut open)
            .default_width(win_width)
            .fixed_size([win_width, win_height])
            .default_pos([center_x, center_y])
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.checkbox(&mut self.world.trail_disabled, "禁用痕迹系统");

                    let c = &mut self.config;
                    let mut changed = false;

                    ui.collapsing("代谢", |ui| {
                        changed |= config_drag_f64(ui, "基础代谢", &mut c.base_metabolism, 0.001, 0.01..=0.5);
                        changed |= config_drag_f64(ui, "年龄代谢倍率", &mut c.age_metabolism_factor, 0.001, 0.0..=0.2);
                        changed |= config_drag_f64(ui, "移动消耗", &mut c.move_cost, 0.0001, 0.0001..=0.01);
                        changed |= config_drag_f64(ui, "体温逸散系数", &mut c.heat_dissipation_coefficient, 0.001, 0.0..=1.0);
                        changed |= config_drag_f64(ui, "喂食效率", &mut c.feed_efficiency, 0.01, 0.1..=1.0);
                    });

                    ui.collapsing("生物", |ui| {
                        changed |= config_drag_f64(ui, "初始能量", &mut c.initial_energy, 1.0, 10.0..=200.0);
                        changed |= config_drag_usize(ui, "最小生物数", &mut c.min_creatures, 5..=100);
                    });

                    ui.collapsing("感知", |ui| {
                        changed |= config_drag_f64(ui, "视觉半径", &mut c.vision_range, 1.0, 50.0..=500.0);
                        changed |= config_drag_f64(ui, "接触距离", &mut c.contact_range, 0.5, 5.0..=50.0);
                    });

                    ui.collapsing("进化", |ui| {
                        changed |= config_drag_f64(ui, "变异率", &mut c.mutation_rate, 0.01, 0.01..=0.5);
                        changed |= config_drag_usize(ui, "初始连接数min", &mut c.initial_connections_min, 1..=20);
                        changed |= config_drag_usize(ui, "初始连接数max", &mut c.initial_connections_max, 2..=30);
                        changed |= config_drag_f64(ui, "种族相似度阈值", &mut c.species_similarity_threshold, 0.01, 0.5..=1.0);
                        changed |= config_drag_f64(ui, "繁殖冷却(秒)", &mut c.reproduce_cooldown, 0.5, 1.0..=60.0);
                    });

                    ui.collapsing("战力", |ui| {
                        changed |= config_drag_f64(ui, "体温权重", &mut c.combat_temp_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "速度权重", &mut c.combat_speed_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "同族援助权重", &mut c.combat_ally_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "同族援助范围", &mut c.combat_ally_range, 1.0, 10.0..=200.0);
                    });

                    ui.collapsing("器官系统", |ui| {
                        changed |= config_drag_f64(ui, "眼睛冷却(秒)", &mut c.eye_cooldown, 0.001, 0.01..=1.0);
                        changed |= config_drag_f64(ui, "嘴巴冷却(秒)", &mut c.mouth_cooldown, 0.01, 0.1..=5.0);
                        changed |= config_drag_f64(ui, "咬转移率", &mut c.bite_transfer_rate, 0.01, 0.01..=0.8);
                    });

                    ui.collapsing("环境温度", |ui| {
                        changed |= config_drag_f64(ui, "热辐射范围", &mut c.volcano_heat_range, 1.0, 200.0..=2000.0);
                        changed |= config_drag_f64(ui, "集体热半径", &mut c.group_heat_radius, 1.0, 50.0..=500.0);
                        changed |= config_drag_f64(ui, "集体热分母", &mut c.group_heat_denominator, 100.0, 500.0..=20000.0);
                    });

                    ui.collapsing("痕迹点", |ui| {
                        changed |= config_drag_f64(ui, "衰减率(/秒)", &mut c.trail_decay_rate, 0.01, 0.01..=0.5);
                        changed |= config_drag_f64(ui, "抑制半径(px)", &mut c.trail_suppress_radius, 1.0, 5.0..=100.0);
                        changed |= config_drag_f64(ui, "生成间隔(秒)", &mut c.trail_emit_interval, 0.01, 0.05..=2.0);
                    });

                    ui.collapsing("其他", |ui| {
                        changed |= config_drag_f64(ui, "优势种最低年龄", &mut c.dominant_min_age, 10.0, 100.0..=2000.0);
                    });

                    ui.separator();
                    if ui.button("重置默认").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *c = Config::default();
                        changed = true;
                    }

                    if changed {
                        c.save();
                    }
                });
            });
        self.panel.settings_open = open;
    }
}

impl CellWorldApp {
    fn render_energy_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.panel.energy_settings_open;
        let screen = ctx.screen_rect();
        let win_width = 520.0;
        let win_height = 560.0;
        let center_x = (screen.width() - win_width) / 2.0;
        let center_y = (screen.height() - win_height) / 2.0;
        egui::Window::new("能量")
            .open(&mut open)
            .default_width(win_width)
            .default_pos([center_x, center_y])
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let world_time = self.world.time;
                    let volcano_timer = self.world.volcano_timer();
                    let meteorite_timer = self.world.meteorite_timer();
                    let c = &mut self.config;
                    let mut changed = false;

                    // 总能量曲线图
                    {
                        ui.strong("总能量趋势");
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

                            let (e_min, e_max) = history.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &(_, e)| {
                                (lo.min(e), hi.max(e))
                            });
                            let e_min = e_min * 0.9;
                            let e_max = e_max * 1.1;
                            let e_range = (e_max - e_min).max(1.0);

                            // 绘制曲线
                            let points: Vec<egui::Pos2> = history.iter().map(|&(t, e)| {
                                let x = rect.left() + ((t - t_min) / t_range * chart_width_f32 as f64) as f32;
                                let y = rect.bottom() - ((e - e_min) / e_range * chart_height_f32 as f64) as f32;
                                egui::pos2(x, y.clamp(rect.top(), rect.bottom()))
                            }).collect();
                            for pair in points.windows(2) {
                                painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 255)));
                            }

                            // 当前值标注
                            if let Some(&last) = points.last() {
                                painter.circle_filled(last, 3.0, egui::Color32::from_rgb(100, 200, 255));
                            }

                            // Y轴标注
                            let label_color = egui::Color32::from_gray(160);
                            painter.text(egui::pos2(rect.left() + 2.0, rect.top() + 2.0), egui::Align2::LEFT_TOP,
                                format!("{:.0}", e_max), egui::FontId::proportional(10.0), label_color);
                            painter.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM,
                                format!("{:.0}", e_min), egui::FontId::proportional(10.0), label_color);

                            // 时间标注
                            painter.text(egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0), egui::Align2::RIGHT_BOTTOM,
                                format_dhms(t_max), egui::FontId::proportional(10.0), label_color);
                        } else {
                            painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                                "采集数据中...", egui::FontId::proportional(12.0), egui::Color32::from_gray(120));
                        }

                        // 当前值
                        let current = self.panel.stats().total_energy;
                        ui.label(format!("当前总能量: {:.0}", current));
                    }
                    ui.separator();

                    ui.collapsing("基础参数", |ui| {
                        changed |= config_drag_f64(ui, "火山半径", &mut c.volcano_radius, 1.0, 100.0..=2000.0);
                        changed |= config_drag_usize(ui, "火山粒子数", &mut c.volcano_count, 10..=200);
                        changed |= config_drag_usize(ui, "陨石粒子数", &mut c.meteorite_count, 5..=100);
                        changed |= config_drag_f64(ui, "陨石长度", &mut c.meteorite_length, 1.0, 50.0..=500.0);
                        changed |= config_drag_f64(ui, "火山衰减率", &mut c.volcano_decay_rate, 0.001, 0.001..=0.1);
                        changed |= config_drag_f64(ui, "陨石衰减率", &mut c.meteorite_decay_rate, 0.001, 0.001..=0.1);
                        changed |= config_drag_f64(ui, "火山杀伤半径", &mut c.volcano_kill_radius, 0.5, 1.0..=50.0);
                        changed |= config_drag_f64(ui, "陨石杀伤半径", &mut c.meteorite_kill_radius, 0.5, 1.0..=50.0);
                    });

                    // 逐个渲染（用宏避免多重借用问题）
                    macro_rules! render_sine_group {
                        ($label:expr, $avg:expr, $avg_range:expr, $avg_speed:expr,
                         $amp:expr, $cycle:expr, $time:expr, $timer:expr, $color:expr) => {{
                            ui.separator();
                            ui.strong($label);

                            // 拖拽控件
                            ui.horizontal(|ui| {
                                ui.label("均值:");
                                changed |= ui.add(egui::DragValue::new($avg).speed($avg_speed).range($avg_range)).changed();
                                ui.label("振幅:");
                                changed |= ui.add(egui::DragValue::new($amp).speed(0.01).range(0.0..=0.9).fixed_decimals(2)).changed();
                                ui.label("周期(秒):");
                                changed |= ui.add(egui::DragValue::new($cycle).speed(10.0).range(0.0..=2000.0)).changed();
                            });

                            // 绘制正弦曲线
                            let avg_val = *$avg;
                            let amp_val = *$amp;
                            let cycle_val = *$cycle;

                            let chart_width_f32 = ui.available_width().min(480.0);
                            let chart_height_f32 = 80.0_f32;
                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(chart_width_f32, chart_height_f32),
                                egui::Sense::hover(),
                            );
                            let rect = response.rect;
                            let chart_width = chart_width_f32 as f64;
                            let chart_height = chart_height_f32 as f64;

                            // 背景
                            painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

                            // 计算显示范围：以当前时间为中心，显示2个周期
                            let display_cycle = if cycle_val > 0.0 { cycle_val } else { 200.0 };
                            let t_center = $time;
                            let t_start = t_center - display_cycle;
                            let t_end = t_center + display_cycle;

                            // Y 轴范围
                            let y_min = avg_val * (1.0 - amp_val) * 0.8;
                            let y_max = avg_val * (1.0 + amp_val) * 1.2;
                            let y_range = (y_max - y_min).max(1.0);

                            // 均值线
                            let avg_y = rect.bottom() - ((avg_val - y_min) / y_range * chart_height) as f32;
                            painter.line_segment(
                                [egui::pos2(rect.left(), avg_y), egui::pos2(rect.right(), avg_y)],
                                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                            );

                            // 正弦波
                            if cycle_val > 0.0 && amp_val > 0.0 {
                                let steps = (chart_width as usize).max(60);
                                let points: Vec<egui::Pos2> = (0..=steps).map(|i| {
                                    let frac = i as f64 / steps as f64;
                                    let t = t_start + frac * (t_end - t_start);
                                    let val = avg_val * (1.0 + amp_val * (std::f64::consts::TAU * t / cycle_val).sin());
                                    let x = rect.left() + (frac * chart_width) as f32;
                                    let y = rect.bottom() - ((val - y_min) / y_range * chart_height) as f32;
                                    egui::pos2(x, y.clamp(rect.top(), rect.bottom()))
                                }).collect();
                                for pair in points.windows(2) {
                                    painter.line_segment([pair[0], pair[1]], egui::Stroke::new(2.0, $color));
                                }
                            } else {
                                // 没有正弦调制，画平线
                                painter.line_segment(
                                    [egui::pos2(rect.left(), avg_y), egui::pos2(rect.right(), avg_y)],
                                    egui::Stroke::new(2.0, $color),
                                );
                            }

                            // 当前时间竖线（"现在"）
                            let now_x = rect.left() + (0.5 * chart_width) as f32; // t_center 在正中
                            painter.line_segment(
                                [egui::pos2(now_x, rect.top()), egui::pos2(now_x, rect.bottom())],
                                egui::Stroke::new(1.0, egui::Color32::GREEN),
                            );

                            // 当前值标注
                            let current_val = if cycle_val > 0.0 && amp_val > 0.0 {
                                avg_val * (1.0 + amp_val * (std::f64::consts::TAU * $time / cycle_val).sin())
                            } else {
                                avg_val
                            };
                            let cur_y = rect.bottom() - ((current_val - y_min) / y_range * chart_height) as f32;
                            painter.circle_filled(egui::pos2(now_x, cur_y.clamp(rect.top(), rect.bottom())), 4.0, egui::Color32::GREEN);

                            // 上一次触发标记（timer 秒前）
                            let last_trigger_t = $time - $timer;
                            if last_trigger_t >= t_start {
                                let last_frac = (last_trigger_t - t_start) / (t_end - t_start);
                                let last_x = rect.left() + (last_frac * chart_width) as f32;
                                painter.line_segment(
                                    [egui::pos2(last_x, rect.top()), egui::pos2(last_x, rect.bottom())],
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 255, 100)),
                                );
                                // 三角形标记
                                let tri_y = rect.top() + 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(last_x, tri_y + 8.0),
                                        egui::pos2(last_x - 4.0, tri_y),
                                        egui::pos2(last_x + 4.0, tri_y),
                                    ],
                                    egui::Color32::from_rgb(100, 255, 100),
                                    egui::Stroke::NONE,
                                ));
                            }

                            // 下一次触发标记
                            let next_interval = if cycle_val > 0.0 && amp_val > 0.0 {
                                avg_val * (1.0 + amp_val * (std::f64::consts::TAU * $time / cycle_val).sin())
                            } else {
                                avg_val
                            };
                            let next_trigger_t = $time + (next_interval - $timer).max(0.0);
                            if next_trigger_t <= t_end {
                                let next_frac = (next_trigger_t - t_start) / (t_end - t_start);
                                let next_x = rect.left() + (next_frac * chart_width) as f32;
                                painter.line_segment(
                                    [egui::pos2(next_x, rect.top()), egui::pos2(next_x, rect.bottom())],
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 120, 120)),
                                );
                                // 三角形标记
                                let tri_y = rect.top() + 2.0;
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        egui::pos2(next_x, tri_y + 8.0),
                                        egui::pos2(next_x - 4.0, tri_y),
                                        egui::pos2(next_x + 4.0, tri_y),
                                    ],
                                    egui::Color32::from_rgb(255, 120, 120),
                                    egui::Stroke::NONE,
                                ));
                            }

                            // 图例标注
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::GREEN, format!("现在: {:.1}", current_val));
                                ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "上次");
                                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), "下次");
                            });
                        }};
                    }

                    render_sine_group!("火山间隔(秒)", &mut c.volcano_interval, 5.0..=120.0, 0.1,
                        &mut c.volcano_interval_amplitude, &mut c.volcano_interval_cycle,
                        world_time, volcano_timer, egui::Color32::from_rgb(255, 100, 50));
                    render_sine_group!("火山能量", &mut c.volcano_particle_energy, 5.0..=200.0, 0.1,
                        &mut c.volcano_energy_amplitude, &mut c.volcano_energy_cycle,
                        world_time, volcano_timer, egui::Color32::from_rgb(255, 180, 50));
                    render_sine_group!("陨石间隔(秒)", &mut c.meteorite_interval, 2.0..=60.0, 0.1,
                        &mut c.meteorite_interval_amplitude, &mut c.meteorite_interval_cycle,
                        world_time, meteorite_timer, egui::Color32::from_rgb(80, 160, 255));
                    render_sine_group!("陨石能量", &mut c.meteorite_particle_energy, 5.0..=200.0, 0.1,
                        &mut c.meteorite_energy_amplitude, &mut c.meteorite_energy_cycle,
                        world_time, meteorite_timer, egui::Color32::from_rgb(120, 220, 180));

                    if changed {
                        c.save();
                    }
                });
            });
        self.panel.energy_settings_open = open;
    }
}

impl eframe::App for CellWorldApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            self.world.set_viewport(bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);

            // 更新世界（如果未暂停）
            if !self.paused {
                self.world.update(dt * self.speed, &self.config);
            }
        }
        self.frame_perf.world_update_ms = t_world.elapsed().as_secs_f64() * 1000.0;

        // 更新面板缓存
        let t_panel = std::time::Instant::now();
        self.panel.update(&self.world, &self.config, self.config.species_similarity_threshold, self.fps, now);
        self.frame_perf.panel_update_ms = t_panel.elapsed().as_secs_f64() * 1000.0;

        // 每10秒记录一次日志
        self.log_stats();

        // 自动保存优势种
        self.auto_save_dominant();

        // 侧边栏面板
        let mut panel_action = PanelAction::default();
        let mut selection_action = PanelAction::default();

        let old_speed = self.speed;
        egui::SidePanel::right("panel")
            .min_width(250.0)
            .show(ctx, |ui| {
                panel_action = self.panel.render(ui, self.fps, &mut self.speed, &mut self.paused, &self.store, &mut self.config);

                // 显示选中信息
                selection_action = self.panel.render_selection(ui, &self.selection, &self.world);
            });

        // 速度变化时同步到配置文件
        if (self.speed - old_speed).abs() > f64::EPSILON {
            self.config.initial_speed = self.speed;
            self.config.save();
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
                            self.world.spawn_from_template(&self.config, &template.genome, template.initial_energy);
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
                if let Some(creature) = self.world.creatures.iter().find(|c| c.id == id && c.alive) {
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
                    }
                }
            }
        }

        // 处理删除模板
        if let Some(name) = panel_action.delete_template {
            self.store.delete(&name);
            self.panel.reset_template_selection();
        }

        // 设置窗口
        if self.panel.settings_open {
            self.render_settings_window(ctx);
        }

        // 能量源周期窗口
        if self.panel.energy_settings_open {
            self.render_energy_settings_window(ctx);
        }

        // 主画布
        let t_central_panel = std::time::Instant::now();
        let mut render_ctx_time = 0.0;
        let mut render_time = 0.0;
        egui::CentralPanel::default().show(ctx, |ui| {
            // 每1秒（真实时间）更新一次渲染上下文（避免频繁计算O(n²)的种族聚类）
            if self.render_ctx_cache.is_none() || now.duration_since(self.last_render_ctx_update).as_secs_f64() >= 1.0 {
                let t_ctx = std::time::Instant::now();
                let creature_species = self.world.get_render_data(self.config.species_similarity_threshold);
                self.render_ctx_cache = Some(RenderContext {
                    creature_species,
                });
                self.last_render_ctx_update = now;
                render_ctx_time = t_ctx.elapsed().as_secs_f64() * 1000.0;
            }
            let render_ctx = self.render_ctx_cache.as_ref().unwrap();
            let t_render = std::time::Instant::now();
            let bounds = self.canvas.render(ui, &self.world, &mut self.selection, render_ctx, &self.config);
            render_time = t_render.elapsed().as_secs_f64() * 1000.0;
            self.last_visible_bounds = Some(bounds);
        });
        let central_panel_time = t_central_panel.elapsed().as_secs_f64() * 1000.0;
        self.frame_perf.render_ctx_ms = render_ctx_time;
        self.frame_perf.render_ms = render_time;
        // egui 开销 = CentralPanel 总时间 - 我们测量的代码时间
        self.frame_perf.egui_overhead_ms = central_panel_time - render_ctx_time - render_time;
        self.frame_perf.frame_total_ms = self.frame_perf.world_update_ms
            + self.frame_perf.panel_update_ms
            + central_panel_time;

        // 持续刷新
        ctx.request_repaint();
    }
}
