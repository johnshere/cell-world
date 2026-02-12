use egui::Ui;

use crate::world::World;

/// 统计面板
pub struct StatsPanel {
    /// 更新间隔
    update_interval: f64,
    /// 上次更新时间
    last_update: f64,
    /// 缓存的统计数据
    cached_stats: CachedStats,
}

#[derive(Default)]
struct CachedStats {
    time: f64,
    creature_count: usize,
    energy_particle_count: usize,
    alive_families: usize,
    extinct_families: usize,
    largest_family: usize,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            update_interval: 0.5, // 500ms
            last_update: 0.0,
            cached_stats: CachedStats::default(),
        }
    }

    /// 更新缓存的统计数据
    pub fn update(&mut self, world: &World) {
        let current_time = world.time;
        if current_time - self.last_update >= self.update_interval {
            self.last_update = current_time;
            let stats = world.stats();
            self.cached_stats = CachedStats {
                time: stats.time,
                creature_count: stats.creature_count,
                energy_particle_count: stats.energy_particle_count,
                alive_families: stats.alive_families,
                extinct_families: stats.extinct_families,
                largest_family: stats.largest_family,
            };
        }
    }

    /// 渲染面板
    pub fn render(&self, ui: &mut Ui, fps: f64) {
        ui.heading("Cell World");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("FPS:");
            ui.label(format!("{:.1}", fps));
        });

        ui.horizontal(|ui| {
            ui.label("时间:");
            ui.label(format!("{:.1}s", self.cached_stats.time));
        });

        ui.separator();
        ui.label("种群统计");

        ui.horizontal(|ui| {
            ui.label("生物数量:");
            ui.label(format!("{}", self.cached_stats.creature_count));
        });

        ui.horizontal(|ui| {
            ui.label("能量粒子:");
            ui.label(format!("{}", self.cached_stats.energy_particle_count));
        });

        ui.horizontal(|ui| {
            ui.label("存活家族:");
            ui.label(format!("{}", self.cached_stats.alive_families));
        });

        ui.horizontal(|ui| {
            ui.label("灭绝家族:");
            ui.label(format!("{}", self.cached_stats.extinct_families));
        });

        ui.horizontal(|ui| {
            ui.label("最大家族:");
            ui.label(format!("{}", self.cached_stats.largest_family));
        });
    }
}

impl Default for StatsPanel {
    fn default() -> Self {
        Self::new()
    }
}
