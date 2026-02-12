use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::world::World;

/// 世界画布渲染器
pub struct WorldCanvas {
    /// 视口偏移
    pub offset: Vec2,
    /// 缩放比例
    pub scale: f32,
    /// 是否正在拖拽
    dragging: bool,
    /// 上一帧鼠标位置
    last_mouse_pos: Option<Pos2>,
}

impl WorldCanvas {
    pub fn new() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.0,
            dragging: false,
            last_mouse_pos: None,
        }
    }

    /// 渲染世界
    pub fn render(&mut self, ui: &mut Ui, world: &World, world_width: f64, world_height: f64) {
        let available_size = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // 处理交互
        self.handle_interaction(&response, ui);

        // 绘制背景
        painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 10, 20));

        // 绘制世界边界
        let world_rect = self.world_to_screen_rect(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(world_width as f32, world_height as f32)),
            rect,
        );
        painter.rect_stroke(world_rect, 0.0, Stroke::new(1.0, Color32::from_rgb(40, 40, 60)));

        // 绘制网格
        self.draw_grid(&painter, rect, world_width, world_height);

        // 绘制能量粒子
        for particle in &world.energy_particles {
            if !particle.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(particle.x as f32, particle.y as f32), rect);
            if rect.contains(pos) {
                let alpha = (1.0 - particle.age / particle.lifetime) as f32;
                let color = Color32::from_rgba_unmultiplied(255, 220, 100, (alpha * 200.0) as u8);
                let radius = 2.0 * self.scale;
                painter.circle_filled(pos, radius, color);
            }
        }

        // 绘制生物
        for creature in &world.creatures {
            if !creature.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(creature.x as f32, creature.y as f32), rect);
            if rect.contains(pos) {
                let (h, s, l) = creature.color();
                let color = hsl_to_rgb(h, s, l);
                let radius = (3.0 + (creature.energy / 50.0) as f32).min(8.0) * self.scale;
                painter.circle_filled(pos, radius, color);
            }
        }
    }

    /// 处理交互
    fn handle_interaction(&mut self, response: &egui::Response, ui: &Ui) {
        // 拖拽平移
        if response.dragged() {
            let delta = response.drag_delta();
            self.offset += delta;
        }

        // 滚轮缩放
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 {
            let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
            self.scale = (self.scale * zoom_factor).clamp(0.1, 10.0);
        }
    }

    /// 绘制网格
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect, world_width: f64, world_height: f64) {
        let grid_size = 50.0 * self.scale;
        if grid_size < 10.0 {
            return; // 太密集不绘制
        }

        let grid_color = Color32::from_rgba_unmultiplied(255, 255, 255, 15);

        // 计算可见范围
        let start_x = (-self.offset.x / grid_size).floor() as i32;
        let end_x = ((rect.width() - self.offset.x) / grid_size).ceil() as i32;
        let start_y = (-self.offset.y / grid_size).floor() as i32;
        let end_y = ((rect.height() - self.offset.y) / grid_size).ceil() as i32;

        for i in start_x..=end_x {
            let x = rect.left() + self.offset.x + i as f32 * grid_size;
            if x >= rect.left() && x <= rect.right() {
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    Stroke::new(0.5, grid_color),
                );
            }
        }

        for i in start_y..=end_y {
            let y = rect.top() + self.offset.y + i as f32 * grid_size;
            if y >= rect.top() && y <= rect.bottom() {
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    Stroke::new(0.5, grid_color),
                );
            }
        }
    }

    /// 世界坐标转屏幕坐标
    fn world_to_screen(&self, world_pos: Pos2, screen_rect: Rect) -> Pos2 {
        Pos2::new(
            screen_rect.left() + self.offset.x + world_pos.x * self.scale,
            screen_rect.top() + self.offset.y + world_pos.y * self.scale,
        )
    }

    /// 世界矩形转屏幕矩形
    fn world_to_screen_rect(&self, world_rect: Rect, screen_rect: Rect) -> Rect {
        Rect::from_min_max(
            self.world_to_screen(world_rect.min, screen_rect),
            self.world_to_screen(world_rect.max, screen_rect),
        )
    }
}

impl Default for WorldCanvas {
    fn default() -> Self {
        Self::new()
    }
}

/// HSL 转 RGB
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match (h as i32) % 360 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    Color32::from_rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
