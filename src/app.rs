use eframe::egui;
use std::fs::OpenOptions;
use std::io::Write;
use crate::config::Config;
use crate::world::World;
use crate::render::{WorldCanvas, StatsPanel, Selection, VisibleWorldBounds};

/// 主应用
pub struct CellWorldApp {
    world: World,
    config: Config,
    canvas: WorldCanvas,
    panel: StatsPanel,
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

        let config = Config::default();
        let world = World::new(&config);
        let now = std::time::Instant::now();

        Self {
            world,
            config,
            canvas: WorldCanvas::new(),
            panel: StatsPanel::new(),
            paused: false,
            speed: 1.0,
            last_update: now,
            fps: 0.0,
            frame_count: 0,
            fps_timer: now,
            last_log_time: 0.0,
            log_initialized: false,
            selection: Selection::None,
            last_visible_bounds: None,
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
            if let Ok(mut file) = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(log_path)
            {
                let _ = writeln!(file, "# Cell World 运行日志\n");
                let _ = writeln!(file, "| 时间(s) | 生物数 | 能量粒子 | 存活家族 | 灭绝家族 | 最大家族 |");
                let _ = writeln!(file, "|---------|--------|----------|----------|----------|----------|");
            }
            self.log_initialized = true;
        }

        // 追加数据行
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .append(true)
            .open(log_path)
        {
            let _ = writeln!(
                file,
                "| {:.0} | {} | {} | {} | {} | {} |",
                stats.time,
                stats.creature_count,
                stats.energy_particle_count,
                stats.alive_families,
                stats.extinct_families,
                stats.largest_family
            );
        }
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
        if let Some(bounds) = self.last_visible_bounds {
            self.world.set_viewport(bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);
        }

        // 更新世界（如果未暂停）
        if !self.paused {
            self.world.update(dt * self.speed, &self.config);
        }

        // 更新面板缓存
        self.panel.update(&self.world);

        // 每10秒记录一次日志
        self.log_stats();

        // 侧边栏面板
        egui::SidePanel::right("panel")
            .min_width(250.0)
            .show(ctx, |ui| {
                self.panel.render(ui, self.fps);
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button(if self.paused { "▶ 继续" } else { "⏸ 暂停" }).clicked() {
                        self.paused = !self.paused;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("速度:");
                    ui.add(egui::Slider::new(&mut self.speed, 0.1..=10.0).logarithmic(true));
                });

                // 显示选中信息
                self.panel.render_selection(ui, &self.selection, &self.world);
            });

        // 主画布
        let world_width = self.config.world_width;
        let world_height = self.config.world_height;
        egui::CentralPanel::default().show(ctx, |ui| {
            let bounds = self.canvas.render(ui, &self.world, world_width, world_height, &mut self.selection);
            self.last_visible_bounds = Some(bounds);
        });

        // 持续刷新
        ctx.request_repaint();
    }
}
