use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, epaint::PathShape};
use rustc_hash::FxHashMap;

use crate::world::World;
use super::Selection;

/// 渲染上下文（种族颜色、家族排名等）
pub struct RenderContext {
    /// 生物索引 -> 种族XOR基因哈希（用于分散且稳定的颜色）
    pub creature_species: FxHashMap<usize, u64>,
    /// 前三家族ID
    pub top_family_ids: Vec<usize>,
}

/// 世界画布渲染器
pub struct WorldCanvas {
    /// 视口偏移
    pub offset: Vec2,
    /// 缩放比例
    pub scale: f32,
    /// 是否已初始化缩放
    initialized: bool,
    /// 初始缩放比例
    initial_scale: f32,
}

/// 可见世界范围
#[derive(Clone, Copy, Debug)]
pub struct VisibleWorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl WorldCanvas {
    pub fn new(initial_scale: f32) -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.0,
            initialized: false,
            initial_scale,
        }
    }

    /// 计算当前可见的世界坐标范围
    pub fn get_visible_world_bounds(&self, screen_rect: Rect) -> VisibleWorldBounds {
        // 屏幕左上角对应的世界坐标
        let min_x = (-self.offset.x / self.scale) as f64;
        let min_y = (-self.offset.y / self.scale) as f64;
        // 屏幕右下角对应的世界坐标
        let max_x = ((screen_rect.width() - self.offset.x) / self.scale) as f64;
        let max_y = ((screen_rect.height() - self.offset.y) / self.scale) as f64;

        VisibleWorldBounds { min_x, min_y, max_x, max_y }
    }

    /// 渲染世界，返回当前可见的世界坐标范围
    pub fn render(&mut self, ui: &mut Ui, world: &World, selection: &mut Selection, ctx: &RenderContext) -> VisibleWorldBounds {
        let available_size = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // 首次渲染时设置默认缩放
        if !self.initialized {
            self.scale = self.initial_scale;
            self.initialized = true;
        }

        // 处理交互
        self.handle_interaction(&response, ui, rect);

        // 处理点击选中
        if response.clicked() {
            if let Some(click_pos) = response.interact_pointer_pos() {
                *selection = self.find_clicked_entity(click_pos, rect, world);
            }
        }

        // 验证选中是否仍有效
        self.validate_selection(selection, world);

        // 绘制背景
        painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 10, 20));

        // 绘制网格
        self.draw_grid(&painter, rect);

        // 选中描边颜色
        let selection_stroke = Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 180));

        // 绘制能量粒子
        for particle in &world.energy_particles {
            if !particle.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(particle.x as f32, particle.y as f32), rect);
            if rect.contains(pos) {
                let alpha = (1.0 - particle.age / particle.lifetime) as f32;
                let color = Color32::from_rgba_unmultiplied(255, 220, 100, (alpha * 200.0) as u8);
                let radius = 1.33 * self.scale;
                painter.circle_filled(pos, radius, color);

                // 选中描边
                if *selection == Selection::Energy(particle.id) {
                    painter.circle_stroke(pos, radius + 2.0, selection_stroke);
                }
            }
        }

        // 绘制生物
        for (idx, creature) in world.creatures.iter().enumerate() {
            if !creature.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(creature.x as f32, creature.y as f32), rect);
            if rect.contains(pos) {
                // 根据种族最小基因哈希确定颜色（稳定标识）
                let species_hash = ctx.creature_species.get(&idx).copied().unwrap_or(0);
                let color = species_to_color(species_hash);

                let radius = (3.0 + (creature.energy / 50.0) as f32).min(8.0) * self.scale;
                painter.circle_filled(pos, radius, color);

                // 家族排名四分之一圆弧（金上/银左/铜下）
                let family_rank = ctx.top_family_ids.iter().position(|&id| id == creature.family_id);
                if let Some(rank) = family_rank {
                    let (rank_color, direction) = match rank {
                        0 => (Color32::from_rgb(255, 215, 0), ArcDirection::Top),      // 金色-上
                        1 => (Color32::from_rgb(192, 192, 192), ArcDirection::Left),   // 银色-左
                        2 => (Color32::from_rgb(205, 127, 50), ArcDirection::Bottom),  // 铜色-下
                        _ => (Color32::WHITE, ArcDirection::Top),
                    };
                    draw_quarter_arc(&painter, pos, radius + 2.0, direction, Stroke::new(0.5, rank_color));
                }

                // 选中五分之一圆弧（白色-右）
                if *selection == Selection::Creature(creature.id) {
                    draw_quarter_arc(&painter, pos, radius + 2.0, ArcDirection::Right, Stroke::new(0.5, Color32::WHITE));
                }
            }
        }

        // 返回可见的世界坐标范围
        self.get_visible_world_bounds(rect)
    }

    /// 查找点击位置的实体
    fn find_clicked_entity(&self, click_pos: Pos2, rect: Rect, world: &World) -> Selection {
        // 优先检测生物（因为生物更大更重要）
        for creature in &world.creatures {
            if !creature.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(creature.x as f32, creature.y as f32), rect);
            let radius = (3.0 + (creature.energy / 50.0) as f32).min(8.0) * self.scale;
            let dist = click_pos.distance(pos);
            if dist <= radius + 5.0 {
                return Selection::Creature(creature.id);
            }
        }

        // 检测能量粒子
        for particle in &world.energy_particles {
            if !particle.alive {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(particle.x as f32, particle.y as f32), rect);
            let radius = 1.33 * self.scale;
            let dist = click_pos.distance(pos);
            if dist <= radius + 5.0 {
                return Selection::Energy(particle.id);
            }
        }

        Selection::None
    }

    /// 验证选中是否仍有效
    fn validate_selection(&self, selection: &mut Selection, world: &World) {
        match *selection {
            Selection::Creature(id) => {
                let found = world.creatures.iter().any(|c| c.id == id && c.alive);
                if !found {
                    *selection = Selection::None;
                }
            }
            Selection::Energy(id) => {
                let found = world.energy_particles.iter().any(|e| e.id == id && e.alive);
                if !found {
                    *selection = Selection::None;
                }
            }
            Selection::None => {}
        }
    }

    /// 处理交互
    fn handle_interaction(&mut self, response: &egui::Response, ui: &Ui, rect: Rect) {
        // 拖拽平移
        if response.dragged() {
            let delta = response.drag_delta();
            self.offset += delta;
        }

        // 滚轮缩放（以鼠标位置为中心）
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 {
            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                // 鼠标相对于画布的位置
                let mouse_in_canvas = mouse_pos - rect.min;

                // 鼠标对应的世界坐标
                let world_x = (mouse_in_canvas.x - self.offset.x) / self.scale;
                let world_y = (mouse_in_canvas.y - self.offset.y) / self.scale;

                // 缩放
                let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 0.9 };
                let new_scale = (self.scale * zoom_factor).clamp(0.1, 10.0);

                // 调整偏移，使鼠标位置对应的世界坐标不变
                self.offset.x = mouse_in_canvas.x - world_x * new_scale;
                self.offset.y = mouse_in_canvas.y - world_y * new_scale;
                self.scale = new_scale;
            }
        }
    }

    /// 绘制网格
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = 50.0 * self.scale;
        if grid_size < 10.0 {
            return; // 太密集不绘制
        }

        let grid_color = Color32::from_rgba_unmultiplied(255, 255, 255, 8);

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
                    Stroke::new(0.3, grid_color),
                );
            }
        }

        for i in start_y..=end_y {
            let y = rect.top() + self.offset.y + i as f32 * grid_size;
            if y >= rect.top() && y <= rect.bottom() {
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    Stroke::new(0.3, grid_color),
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

}

impl Default for WorldCanvas {
    fn default() -> Self {
        Self::new(0.6)
    }
}

/// 圆弧方向
enum ArcDirection {
    Top,
    Left,
    Bottom,
    Right,
}

/// 绘制五分之一圆弧
fn draw_quarter_arc(painter: &egui::Painter, center: Pos2, radius: f32, direction: ArcDirection, stroke: Stroke) {
    // 根据方向确定起始和结束角度（弧度），五分之一圆 = 72° = 0.4π
    let half_arc = std::f32::consts::PI * 0.2;  // 36° 半角
    let (start_angle, end_angle) = match direction {
        ArcDirection::Top => (-std::f32::consts::PI * 0.5 - half_arc, -std::f32::consts::PI * 0.5 + half_arc),    // 上: -126° 到 -54°
        ArcDirection::Left => (std::f32::consts::PI - half_arc, std::f32::consts::PI + half_arc),                  // 左: 144° 到 216°
        ArcDirection::Bottom => (std::f32::consts::PI * 0.5 - half_arc, std::f32::consts::PI * 0.5 + half_arc),   // 下: 54° 到 126°
        ArcDirection::Right => (-half_arc, half_arc),                                                              // 右: -36° 到 36°
    };

    // 用多个点近似圆弧
    let segments = 12;
    let points: Vec<Pos2> = (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let angle = start_angle + t * (end_angle - start_angle);
            Pos2::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();

    painter.add(PathShape::line(points, stroke));
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

/// 根据种族基因哈希生成颜色
fn species_to_color(species_hash: u64) -> Color32 {
    // 使用混合哈希函数使任意哈希值都能产生分散的色相
    // 基于 splitmix64 的快速混合
    let mut h = species_hash;
    h = h.wrapping_add(0x9e3779b97f4a7c15);
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
    h = h ^ (h >> 31);

    // 直接使用低位计算色相（避免 f64 精度丢失）
    // 使用黄金角（137.5°）乘数来分散相邻值
    let golden_angle = 137.5_f32;
    let hue = ((h as u32) as f32 * golden_angle) % 360.0;
    hsl_to_rgb(hue, 0.7, 0.5)
}
