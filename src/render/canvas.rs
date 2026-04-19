use egui::{
    epaint::{Mesh, PathShape, Vertex},
    Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use rustc_hash::FxHashMap;

use super::Selection;
use crate::world::{SimSnapshot, TerrainMap, GRID_WORLD_SIZE};

/// 渲染上下文（种族颜色）
pub struct RenderContext {
    /// 生物 ID -> 种族哈希（用于稳定颜色，不受 Vec 索引重排影响）
    pub creature_species: FxHashMap<u64, u64>,
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
    /// 当前帧率（用于自适应渲染质量）
    pub fps: f64,
    /// 地形显示透明度（0~255）
    pub terrain_alpha: u8,
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
            fps: 60.0,
            terrain_alpha: 5,
        }
    }

    /// 计算当前可见的世界坐标范围
    pub fn get_visible_world_bounds(&self, screen_rect: Rect) -> VisibleWorldBounds {
        let min_x = (-self.offset.x / self.scale) as f64;
        let min_y = (-self.offset.y / self.scale) as f64;
        let max_x = ((screen_rect.width() - self.offset.x) / self.scale) as f64;
        let max_y = ((screen_rect.height() - self.offset.y) / self.scale) as f64;

        VisibleWorldBounds {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// 渲染世界，返回当前可见的世界坐标范围
    pub fn render(
        &mut self,
        ui: &mut Ui,
        snapshot: &SimSnapshot,
        selection: &mut Selection,
        ctx: &RenderContext,
        config: &crate::config::Config,
    ) -> VisibleWorldBounds {
        let available_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        // 计算白色像素 UV（用于批量 Mesh 渲染纯色四边形）
        let font_image_size = ui.ctx().fonts(|f| f.font_image_size());
        let white_uv = Pos2::new(
            0.5 / font_image_size[0] as f32,
            0.5 / font_image_size[1] as f32,
        );

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
                *selection = self.find_clicked_entity(click_pos, rect, snapshot);
            }
        }

        // 验证选中是否仍有效
        self.validate_selection(selection, snapshot);

        // 绘制背景
        painter.rect_filled(rect, 0.0, Color32::from_rgb(10, 10, 20));

        // 绘制地形（如已生成）
        self.draw_terrain(&painter, rect, &snapshot.terrain);

        // 绘制网格
        self.draw_grid(&painter, rect);

        // 选中描边颜色
        let selection_stroke =
            Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 180));

        // 提前计算世界坐标可见范围（避免对每个实体做 world_to_screen）
        let vis = self.get_visible_world_bounds(rect);
        // 加一点边距（世界坐标单位），确保边缘实体不被裁掉
        let margin = 20.0 / self.scale as f64;
        let vis_min_x = vis.min_x - margin;
        let vis_max_x = vis.max_x + margin;
        let vis_min_y = vis.min_y - margin;
        let vis_max_y = vis.max_y + margin;

        // === LOD ===
        let draw_explode = self.scale > 0.4;
        let draw_trails = self.scale > 0.12 && !snapshot.trail_disabled;
        let creature_dot_mode = self.scale <= 0.15;

        // ===== 绘制能量粒子（批量 Mesh，跳过 egui 曲面细分）=====
        {
            let mut mesh = Mesh::default();
            mesh.vertices
                .reserve(snapshot.energy_particles.len().min(8000) * 4);
            mesh.indices
                .reserve(snapshot.energy_particles.len().min(8000) * 6);

            for particle in &snapshot.energy_particles {
                if !particle.alive {
                    continue;
                }
                // 世界坐标裁剪
                if particle.x < vis_min_x
                    || particle.x > vis_max_x
                    || particle.y < vis_min_y
                    || particle.y > vis_max_y
                {
                    continue;
                }
                let pos =
                    self.world_to_screen(Pos2::new(particle.x as f32, particle.y as f32), rect);
                // 爆炸特效（仅高缩放，单独 Shape — 数量有限）
                if draw_explode {
                    let age = particle.age as f32;
                    if age < 1.5 {
                        let progress = age / 1.5;
                        let radius = (1.5 + 2.5 * progress) * self.scale;
                        let g = (80.0 + 140.0 * progress) as u8;
                        let b = (20.0 + 80.0 * progress) as u8;
                        let alpha = (80.0 + 175.0 * progress) as u8;
                        painter.circle_filled(
                            pos,
                            radius,
                            Color32::from_rgba_unmultiplied(255, g, b, alpha),
                        );
                    }
                }
                // 基础粒子 → 四边形加入批量 Mesh（微粒与圆无视觉差异）
                let alpha = (particle.energy / particle.initial_energy).clamp(0.0, 1.0) as f32;
                let color = if particle.lava {
                    // 熔岩流粒子：橙红色
                    Color32::from_rgba_unmultiplied(255, 100, 30, (alpha * 220.0) as u8)
                } else {
                    Color32::from_rgba_unmultiplied(255, 220, 100, (alpha * 200.0) as u8)
                };
                let r = 1.064 * self.scale;
                add_quad(&mut mesh, pos, r, color, white_uv);

                // 选中描边（极少触发）
                if *selection == Selection::Energy(particle.id) {
                    painter.circle_stroke(pos, r + 2.0, selection_stroke);
                }
            }
            if !mesh.vertices.is_empty() {
                painter.add(egui::Shape::Mesh(mesh));
            }
        }

        // ===== 绘制痕迹点（批量 Mesh）=====
        if draw_trails {
            let mut mesh = Mesh::default();
            mesh.vertices
                .reserve(snapshot.trail_points.len().min(6000) * 4);
            mesh.indices.reserve(snapshot.trail_points.len().min(6000) * 6);

            for trail in &snapshot.trail_points {
                if !trail.alive {
                    continue;
                }
                // 世界坐标裁剪
                if trail.x < vis_min_x
                    || trail.x > vis_max_x
                    || trail.y < vis_min_y
                    || trail.y > vis_max_y
                {
                    continue;
                }
                let pos = self.world_to_screen(Pos2::new(trail.x as f32, trail.y as f32), rect);
                let age_ratio = (1.0 - trail.age / 38.0).max(0.0) as f32;
                let alpha = (80.0 * age_ratio) as u8;
                let base_color = species_to_color(trail.clan_hash);
                let color = Color32::from_rgba_unmultiplied(
                    base_color.r(),
                    base_color.g(),
                    base_color.b(),
                    alpha,
                );
                let r = (trail.visual_radius as f32 * 0.2 * age_ratio * self.scale)
                    .max(0.3 * self.scale);
                add_quad(&mut mesh, pos, r, color, white_uv);
            }
            if !mesh.vertices.is_empty() {
                painter.add(egui::Shape::Mesh(mesh));
            }
        }

        // 绘制火山喷射范围参考圈
        {
            let volcano_pos = self.world_to_screen(
                Pos2::new(config.volcano_x as f32, config.volcano_y as f32),
                rect,
            );
            let radius = config.volcano_radius as f32 * self.scale;
            if radius > 5.0 {
                painter.circle_stroke(
                    volcano_pos,
                    radius,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 120, 50, 40)),
                );
            }
        }

        // 绘制火山标记（原点 0,0）
        {
            let volcano_pos = self.world_to_screen(Pos2::new(0.0, 0.0), rect);
            if rect.contains(volcano_pos) {
                let outer_r = 8.0 * self.scale;
                let inner_r = 4.0 * self.scale;
                let volcano_color = Color32::from_rgb(255, 80, 30);
                painter.circle_filled(
                    volcano_pos,
                    outer_r,
                    Color32::from_rgba_unmultiplied(255, 80, 30, 80),
                );
                painter.circle_filled(volcano_pos, inner_r, volcano_color);
            }
        }



        // ===== 绘制生物 =====
        // 极低缩放时用批量 Mesh 渲染点
        let mut dot_mesh = if creature_dot_mode {
            let mut m = Mesh::default();
            m.vertices.reserve(snapshot.creatures.len() * 4);
            m.indices.reserve(snapshot.creatures.len() * 6);
            Some(m)
        } else {
            None
        };

        for creature in &snapshot.creatures {
            if !creature.alive {
                continue;
            }
            // 世界坐标裁剪
            if creature.x < vis_min_x
                || creature.x > vis_max_x
                || creature.y < vis_min_y
                || creature.y > vis_max_y
            {
                continue;
            }
            let pos = self.world_to_screen(Pos2::new(creature.x as f32, creature.y as f32), rect);
            let species_hash = ctx.creature_species.get(&creature.id).copied().unwrap_or(0);
            let color = species_to_color(species_hash);
            let is_selected = *selection == Selection::Creature(creature.id);

            if creature_dot_mode && !is_selected {
                // 极低缩放：仅绘制单色点，加入批量 Mesh
                let dot_r = (1.0_f32).max(self.scale);
                if let Some(ref mut mesh) = dot_mesh {
                    add_quad(mesh, pos, dot_r, color, white_uv);
                }
            } else {
                let radius = ((creature.energy as f32 * 1.28).cbrt()).clamp(1.5, 8.0) * self.scale;
                painter.circle_filled(pos, radius, color);

                let heading = creature.heading as f32;

                // 器官绘制（缩放足够大时）
                if self.scale > 0.3 {
                    // 嘴巴（弧线，粉红色）
                    {
                        let mouth_stroke = radius * 0.25;
                        let mouth_arc_r = radius * 1.05 + mouth_stroke * 0.5;
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
                        painter.add(PathShape::line(
                            points,
                            Stroke::new(mouth_stroke, Color32::from_rgb(255, 100, 100)),
                        ));
                    }

                    // 双眼（白圆+黑瞳，瞳孔跟随扫描方向）
                    {
                        let eye_r = radius * 0.25;
                        let pupil_r = eye_r * 0.5;
                        let eye_offset = 50.0_f32.to_radians(); // ±50°
                        for eye_i in 0..2 {
                            let sign = if eye_i == 0 { -1.0_f32 } else { 1.0_f32 };
                            let eye_angle = heading + sign * eye_offset;
                            let eye_pos = Pos2::new(
                                pos.x + radius * eye_angle.cos(),
                                pos.y + radius * eye_angle.sin(),
                            );
                            painter.circle_filled(eye_pos, eye_r, Color32::WHITE);
                            // 瞳孔方向跟随扫描偏移
                            let pupil_dir = if eye_i == 0 {
                                heading + 20f32.to_radians() - creature.eye_scan_offset[0] as f32
                            } else {
                                heading - 20f32.to_radians() + creature.eye_scan_offset[1] as f32
                            };
                            let pupil_pos = Pos2::new(
                                eye_pos.x + pupil_r * 0.5 * pupil_dir.cos(),
                                eye_pos.y + pupil_r * 0.5 * pupil_dir.sin(),
                            );
                            painter.circle_filled(pupil_pos, pupil_r, Color32::BLACK);
                        }
                    }
                }

                // 选中：半径大2px的白色圆
                if is_selected {
                    painter.circle_stroke(pos, radius + 2.0, Stroke::new(1.0, Color32::WHITE));
                }
            }
        }

        if let Some(mesh) = dot_mesh {
            if !mesh.vertices.is_empty() {
                painter.add(egui::Shape::Mesh(mesh));
            }
        }

        // 返回可见的世界坐标范围
        self.get_visible_world_bounds(rect)
    }

    /// 查找点击位置的实体
    fn find_clicked_entity(&self, click_pos: Pos2, rect: Rect, snapshot: &SimSnapshot) -> Selection {
        // 优先检测生物（因为生物更大更重要）
        for creature in &snapshot.creatures {
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
        for particle in &snapshot.energy_particles {
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
    fn validate_selection(&self, selection: &mut Selection, snapshot: &SimSnapshot) {
        match *selection {
            Selection::Creature(id) => {
                let found = snapshot.creatures.iter().any(|c| c.id == id && c.alive);
                if !found {
                    *selection = Selection::None;
                }
            }
            Selection::Energy(id) => {
                let found = snapshot.energy_particles.iter().any(|e| e.id == id && e.alive);
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
        let pointer_over_canvas = ui
            .input(|i| i.pointer.hover_pos().map_or(false, |p| rect.contains(p)))
            && !ui.ctx().is_pointer_over_area();
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

    /// 绘制地形：每个 chunk 一个填充矩形，颜色按高度归一化
    fn draw_terrain(&self, painter: &egui::Painter, rect: Rect, terrain: &TerrainMap) {
        if !terrain.is_generated() || terrain.chunks.is_empty() {
            return;
        }
        let chunk_size_screen = (GRID_WORLD_SIZE as f32) * self.scale;
        if chunk_size_screen < 1.5 {
            return; // 太密集不绘制
        }

        // 计算可见 chunk 范围
        let vis = self.get_visible_world_bounds(rect);
        let cx_min = (vis.min_x / GRID_WORLD_SIZE).floor() as i32 - 1;
        let cx_max = (vis.max_x / GRID_WORLD_SIZE).ceil() as i32 + 1;
        let cy_min = (vis.min_y / GRID_WORLD_SIZE).floor() as i32 - 1;
        let cy_max = (vis.max_y / GRID_WORLD_SIZE).ceil() as i32 + 1;

        let h_range = (terrain.max_h - terrain.min_h).max(1) as f32;

        // 用 Mesh 批量绘制
        let font_image_size = painter.ctx().fonts(|f| f.font_image_size());
        let white_uv = Pos2::new(
            0.5 / font_image_size[0] as f32,
            0.5 / font_image_size[1] as f32,
        );
        let mut mesh = Mesh::default();

        for cy in cy_min..=cy_max {
            for cx in cx_min..=cx_max {
                let Some(&h) = terrain.chunks.get(&(cx, cy)) else {
                    continue;
                };
                let t = ((h - terrain.min_h) as f32 / h_range).clamp(0.0, 1.0);
                let color = terrain_color(t, self.terrain_alpha);

                let wx = cx as f64 * GRID_WORLD_SIZE;
                let wy = cy as f64 * GRID_WORLD_SIZE;
                let p0 = self.world_to_screen(Pos2::new(wx as f32, wy as f32), rect);
                let p1 = Pos2::new(p0.x + chunk_size_screen, p0.y + chunk_size_screen);
                add_rect(&mut mesh, p0, p1, color, white_uv);
            }
        }

        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::Mesh(mesh));
        }
    }

    /// 绘制网格
    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = (GRID_WORLD_SIZE as f32) * self.scale;
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

/// 向 Mesh 添加一个纯色四边形（替代 circle_filled，跳过曲面细分）
#[inline]
fn add_quad(mesh: &mut Mesh, center: Pos2, half_size: f32, color: Color32, uv: Pos2) {
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: Pos2::new(center.x - half_size, center.y - half_size),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(center.x + half_size, center.y - half_size),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(center.x + half_size, center.y + half_size),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(center.x - half_size, center.y + half_size),
        uv,
        color,
    });
    mesh.indices
        .extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
}

/// 在两点间添加矩形（左上 p0、右下 p1）到 mesh
fn add_rect(mesh: &mut Mesh, p0: Pos2, p1: Pos2, color: Color32, uv: Pos2) {
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: Pos2::new(p0.x, p0.y),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(p1.x, p0.y),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(p1.x, p1.y),
        uv,
        color,
    });
    mesh.vertices.push(Vertex {
        pos: Pos2::new(p0.x, p1.y),
        uv,
        color,
    });
    mesh.indices
        .extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
}

/// 地形高度颜色映射：海底火山主题（低=深蓝 → 中=青蓝 → 高=暖橙）
fn terrain_color(t: f32, alpha: u8) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.5 {
        let k = t / 0.5;
        (
            40.0 + (70.0 - 40.0) * k,
            60.0 + (110.0 - 60.0) * k,
            100.0 + (150.0 - 100.0) * k,
        )
    } else {
        let k = (t - 0.5) / 0.5;
        (
            70.0 + (200.0 - 70.0) * k,
            110.0 + (130.0 - 110.0) * k,
            150.0 + (90.0 - 150.0) * k,
        )
    };
    Color32::from_rgba_unmultiplied(r as u8, g as u8, b as u8, alpha)
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
pub fn species_to_color(species_hash: u64) -> Color32 {
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
