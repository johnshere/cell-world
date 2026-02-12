use eframe::egui;
use crate::config::Config;
use crate::world::World;
use crate::render::{WorldCanvas, StatsPanel};

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
}

impl CellWorldApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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

        // 更新世界（如果未暂停）
        if !self.paused {
            self.world.update(dt * self.speed, &self.config);
        }

        // 更新面板缓存
        self.panel.update(&self.world);

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
            });

        // 主画布
        let world_width = self.config.world_width;
        let world_height = self.config.world_height;
        egui::CentralPanel::default().show(ctx, |ui| {
            self.canvas.render(ui, &self.world, world_width, world_height);
        });

        // 持续刷新
        ctx.request_repaint();
    }
}
