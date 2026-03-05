use eframe::egui;
use std::fs::OpenOptions;
use std::io::Write;
use crate::config::Config;
use crate::store::{Store, CreatureTemplate};
use crate::world::World;
use crate::render::{WorldCanvas, StatsPanel, Selection, VisibleWorldBounds, PanelAction, RenderContext};

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
                    let c = &mut self.config;
                    let mut changed = false;

                    ui.collapsing("能量源", |ui| {
                        changed |= config_drag_f64(ui, "火山间隔(秒)", &mut c.volcano_interval, 0.1, 5.0..=120.0);
                        changed |= config_drag_f64(ui, "火山半径", &mut c.volcano_radius, 1.0, 100.0..=2000.0);
                        changed |= config_drag_usize(ui, "火山粒子数", &mut c.volcano_count, 10..=200);
                        changed |= config_drag_f64(ui, "火山粒子能量", &mut c.volcano_particle_energy, 0.1, 5.0..=100.0);
                        changed |= config_drag_f64(ui, "陨石间隔(秒)", &mut c.meteorite_interval, 0.1, 2.0..=60.0);
                        changed |= config_drag_usize(ui, "陨石粒子数", &mut c.meteorite_count, 5..=100);
                        changed |= config_drag_f64(ui, "陨石长度", &mut c.meteorite_length, 1.0, 50.0..=500.0);
                        changed |= config_drag_f64(ui, "陨石粒子能量", &mut c.meteorite_particle_energy, 0.1, 5.0..=100.0);
                        changed |= config_drag_f64(ui, "火山衰减率", &mut c.volcano_decay_rate, 0.001, 0.001..=0.1);
                        changed |= config_drag_f64(ui, "陨石衰减率", &mut c.meteorite_decay_rate, 0.001, 0.001..=0.1);
                        changed |= config_drag_f64(ui, "火山杀伤半径", &mut c.volcano_kill_radius, 0.5, 1.0..=50.0);
                        changed |= config_drag_f64(ui, "陨石杀伤半径", &mut c.meteorite_kill_radius, 0.5, 1.0..=50.0);
                        let v_max = c.volcano_interval;
                        changed |= config_drag_f64(ui, "火山下落时长", &mut c.volcano_fall_duration, 0.1, 0.0..=v_max);
                        let m_max = c.meteorite_interval;
                        changed |= config_drag_f64(ui, "陨石下落时长", &mut c.meteorite_fall_duration, 0.1, 0.0..=m_max);
                    });

                    ui.collapsing("代谢", |ui| {
                        changed |= config_drag_f64(ui, "基础代谢", &mut c.base_metabolism, 0.001, 0.01..=0.5);
                        changed |= config_drag_f64(ui, "年龄代谢倍率", &mut c.age_metabolism_factor, 0.001, 0.0..=0.2);
                        changed |= config_drag_f64(ui, "移动消耗", &mut c.move_cost, 0.0001, 0.0001..=0.01);
                        changed |= config_drag_f64(ui, "体温逸散系数", &mut c.heat_dissipation_coefficient, 0.0001, 0.0..=0.01);
                        changed |= config_drag_f64(ui, "喂食效率", &mut c.feed_efficiency, 0.01, 0.1..=1.0);
                    });

                    ui.collapsing("生物", |ui| {
                        changed |= config_drag_f64(ui, "初始能量", &mut c.initial_energy, 1.0, 10.0..=200.0);
                        changed |= config_drag_usize(ui, "最小生物数", &mut c.min_creatures, 5..=100);
                    });

                    ui.collapsing("感知", |ui| {
                        changed |= config_drag_f64(ui, "视觉半径", &mut c.vision_range, 1.0, 50.0..=500.0);
                        changed |= config_drag_f64(ui, "接触距离", &mut c.contact_range, 0.5, 5.0..=50.0);
                        changed |= config_drag_f64(ui, "鼻子半角(弧度)", &mut c.nose_half_angle, 0.01, 0.1..=1.5);
                    });

                    ui.collapsing("进化", |ui| {
                        changed |= config_drag_f64(ui, "变异率", &mut c.mutation_rate, 0.01, 0.01..=0.5);
                        changed |= config_drag_usize(ui, "初始连接数min", &mut c.initial_connections_min, 1..=20);
                        changed |= config_drag_usize(ui, "初始连接数max", &mut c.initial_connections_max, 2..=30);
                        changed |= config_drag_f64(ui, "种族相似度阈值", &mut c.species_similarity_threshold, 0.01, 0.5..=1.0);
                    });

                    ui.collapsing("战力", |ui| {
                        changed |= config_drag_f64(ui, "体温权重", &mut c.combat_temp_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "速度权重", &mut c.combat_speed_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "同族援助权重", &mut c.combat_ally_weight, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "同族援助范围", &mut c.combat_ally_range, 1.0, 10.0..=200.0);
                    });

                    ui.collapsing("器官系统", |ui| {
                        changed |= config_drag_f64(ui, "鼻子冷却(秒)", &mut c.nose_cooldown, 0.01, 0.05..=2.0);
                        changed |= config_drag_f64(ui, "眼睛冷却(秒)", &mut c.eye_cooldown, 0.01, 0.05..=1.0);
                        changed |= config_drag_f64(ui, "嘴巴冷却(秒)", &mut c.mouth_cooldown, 0.01, 0.1..=5.0);
                        changed |= config_drag_f64(ui, "鼻子扫描成本", &mut c.nose_scan_cost, 0.001, 0.0..=0.2);
                        changed |= config_drag_f64(ui, "眼睛扫描成本", &mut c.eye_scan_cost, 0.001, 0.0..=0.2);
                        changed |= config_drag_f64(ui, "咬消耗", &mut c.bite_cost, 0.01, 0.0..=2.0);
                        changed |= config_drag_f64(ui, "咬转移率", &mut c.bite_transfer_rate, 0.01, 0.01..=0.5);
                    });

                    ui.collapsing("环境温度", |ui| {
                        changed |= config_drag_f64(ui, "热辐射范围", &mut c.volcano_heat_range, 1.0, 200.0..=2000.0);
                        changed |= config_drag_f64(ui, "远离火山额外散热", &mut c.cold_loss_factor, 0.01, 0.0..=3.0);
                        changed |= config_drag_f64(ui, "散热上限(×体型)", &mut c.thermal_mass_factor, 0.5, 1.0..=30.0);
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
