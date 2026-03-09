use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, epaint::PathShape};
use rustc_hash::FxHashMap;

use crate::world::World;
use super::Selection;

/// 渲染上下文（种族颜色）
pub struct RenderContext {
    /// 生物索引 -> 种族XOR基因哈希（用于分散且稳定的颜色）
    pub creature_species: FxHashMap<usize, u64>,
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
    pub fn render(&mut self, ui: &mut Ui, world: &World, selection: &mut Selection, ctx: &RenderContext, config: &crate::config::Config) -> VisibleWorldBounds {
        let available_size = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // 首次渲染时设置默认缩放并居中视窗（原点在屏幕中心）
        if !self.initialized {
            self.scale = self.initial_scale;
            self.offset = Vec2::new(available_size.x / 2.0, available_size.y / 2.0);
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
                {
                    let age = particle.age as f32;
                    let explode_duration = 1.5_f32; // 爆炸特效时长
                    if age < explode_duration {
                        let progress = age / explode_duration;
                        let radius = (1.5 + 2.5 * progress) * self.scale;
                        let r = 255u8;
                        let g = (80.0 + 140.0 * progress) as u8;
                        let b = (20.0 + 80.0 * progress) as u8;
                        let alpha = (80.0 + 175.0 * progress) as u8;
                        let color = Color32::from_rgba_unmultiplied(r, g, b, alpha);
                        painter.circle_filled(pos, radius, color);
                    }
                    let alpha = (particle.energy / particle.initial_energy).clamp(0.0, 1.0) as f32;
                    let color = Color32::from_rgba_unmultiplied(255, 220, 100, (alpha * 200.0) as u8);
                    let radius = 1.064 * self.scale;
                    painter.circle_filled(pos, radius, color);

                    // 选中描边
                    if *selection == Selection::Energy(particle.id) {
                        painter.circle_stroke(pos, radius + 2.0, selection_stroke);
                    }
                }
            }
        }

        // 绘制痕迹点
        for trail in &world.trail_points {
            if !trail.alive { continue; }
            let pos = self.world_to_screen(Pos2::new(trail.x as f32, trail.y as f32), rect);
            if rect.contains(pos) {
                // 透明度和半径都随时间线性衰减（decay_rate=0.12，约38秒消失）
                let age_ratio = (1.0 - trail.age / 38.0).max(0.0) as f32;
                let alpha = (80.0 * age_ratio) as u8;
                let color = species_to_color(trail.genome_hash);
                let trail_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                let radius = (trail.visual_radius as f32 * 0.2 * age_ratio * self.scale).max(0.3 * self.scale);
                painter.circle_filled(pos, radius, trail_color);
            }
        }

        // 绘制火山热辐射圈
        {
            let volcano_pos = self.world_to_screen(Pos2::new(config.volcano_x as f32, config.volcano_y as f32), rect);
            let heat_radius = config.volcano_heat_range as f32 * self.scale;
            if heat_radius > 5.0 {
                painter.circle_stroke(
                    volcano_pos,
                    heat_radius,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 100, 50, 20)),
                );
            }
        }

        // 绘制初始世界范围矩形
        {
            let (ib_min_x, ib_min_y, ib_max_x, ib_max_y) = world.initial_bounds;
            let top_left = self.world_to_screen(Pos2::new(ib_min_x as f32, ib_min_y as f32), rect);
            let bottom_right = self.world_to_screen(Pos2::new(ib_max_x as f32, ib_max_y as f32), rect);
            let bounds_rect = Rect::from_min_max(top_left, bottom_right);
            painter.rect_stroke(
                bounds_rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 25)),
            );
        }

        // 绘制火山标记（原点 0,0）
        {
            let volcano_pos = self.world_to_screen(Pos2::new(0.0, 0.0), rect);
            if rect.contains(volcano_pos) {
                let outer_r = 8.0 * self.scale;
                let inner_r = 4.0 * self.scale;
                let volcano_color = Color32::from_rgb(255, 80, 30);
                painter.circle_filled(volcano_pos, outer_r, Color32::from_rgba_unmultiplied(255, 80, 30, 80));
                painter.circle_filled(volcano_pos, inner_r, volcano_color);
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

                let radius = ((creature.energy as f32 * 1.28).cbrt()).clamp(1.5, 8.0) * self.scale;
                painter.circle_filled(pos, radius, color);

                let heading = creature.heading as f32;

                // 器官绘制（缩放足够大时）
                if self.scale > 0.3 {
                    let organs = &creature.genome.organ_genes;

                    // 嘴巴（弧线，粉红色，以生物中心为圆心，从1.1倍半径向外加厚）
                    // 先绘制嘴巴，使其图层在鼻子下面
                    if organs.mouth {
                        let mouth_stroke = radius * 0.25;
                        let mouth_arc_r = radius * 1.05 + mouth_stroke * 0.5; // stroke中线，内边缘在1.05倍半径
                        let half_arc = 0.4363; // 50°/2 = 25° ≈ 0.4363 rad
                        let segments = 8;
                        let points: Vec<Pos2> = (0..=segments)
                            .map(|i| {
                                let t = i as f32 / segments as f32;
                                let a = heading - half_arc + t * half_arc * 2.0;
                                Pos2::new(
                                    pos.x + mouth_arc_r * a.cos(),
                                    pos.y + mouth_arc_r * a.sin(),
                                )
                            })
                            .collect();
                        painter.add(PathShape::line(points, Stroke::new(mouth_stroke, Color32::from_rgb(255, 100, 100))));
                    }

                    // 鼻子（正前方线段，淡蓝色，长度按 power 缩放）
                    if organs.nose {
                        let nose_len = radius * 0.4 * organs.nose_power as f32;
                        let nose_start = Pos2::new(
                            pos.x + radius * heading.cos(),
                            pos.y + radius * heading.sin(),
                        );
                        let nose_end = Pos2::new(
                            pos.x + (radius + nose_len) * heading.cos(),
                            pos.y + (radius + nose_len) * heading.sin(),
                        );
                        painter.line_segment(
                            [nose_start, nose_end],
                            Stroke::new(radius * 0.15, Color32::from_rgb(200, 200, 255)),
                        );
                    }

                    // 双眼（白圆+黑瞳，半径按 power 缩放）
                    if organs.eyes {
                        let eye_r = (radius * 0.25 * organs.eye_power as f32).max(radius * 0.12).min(radius * 0.4);
                        let pupil_r = eye_r * 0.5;
                        let eye_offset = 50.0_f32.to_radians(); // ±50°
                        for &sign in &[-1.0_f32, 1.0] {
                            let eye_angle = heading + sign * eye_offset;
                            let eye_pos = Pos2::new(
                                pos.x + radius * eye_angle.cos(),
                                pos.y + radius * eye_angle.sin(),
                            );
                            painter.circle_filled(eye_pos, eye_r, Color32::WHITE);
                            painter.circle_filled(eye_pos, pupil_r, Color32::BLACK);
                        }
                    }
                }

                // 选中：半径大2px的白色圆
                if *selection == Selection::Creature(creature.id) {
                    painter.circle_stroke(pos, radius + 2.0, Stroke::new(1.0, Color32::WHITE));
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
            let radius = ((creature.energy as f32 * 1.28).cbrt()).clamp(1.5, 8.0) * self.scale;
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
            let radius = 1.064 * self.scale;
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

        // 滚轮缩放（以鼠标位置为中心，仅当鼠标在画布区域且无其他窗口遮挡时）
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        let pointer_over_canvas = ui.input(|i| {
            i.pointer.hover_pos().map_or(false, |p| rect.contains(p))
        }) && !ui.ctx().is_pointer_over_area();
        if scroll_delta != 0.0 && pointer_over_canvas {
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
