//! 分组力导图组件
//!
//! 自包含的力导向图可视化组件，用于展示生物神经网络的节点拓扑、
//! 连接关系和区块分组。不依赖 panel/app 细节，仅依赖 Genome 数据结构。
//!
//! 使用方式：
//! 1. `ForceGraphState::new()` 创建状态
//! 2. 每次打开新基因组时调用 `init_from_genome()`
//! 3. 每帧调用 `render(ui, genome)` 绘制

use crate::neural::genome::{Genome, NodeGene, NodeType};
use rustc_hash::FxHashMap;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 力模拟状态（跨帧持久化）
// ---------------------------------------------------------------------------

/// 力导图状态：持有节点位置、速度、温度等，跨帧维护实现动画收敛
pub struct ForceGraphState {
    /// 节点 ID → 画布坐标
    positions: FxHashMap<usize, egui::Pos2>,
    /// 节点 ID → 速度矢量
    velocities: FxHashMap<usize, egui::Vec2>,
    /// 退火温度，降至 0.01 以下收敛
    temperature: f64,
    /// 是否已收敛（停止力迭代，仅绘制）
    settled: bool,
    /// 用户缩放因子
    scale: f32,
    /// 用户平移偏移
    offset: egui::Vec2,
}

impl ForceGraphState {
    pub fn new() -> Self {
        Self {
            positions: FxHashMap::default(),
            velocities: FxHashMap::default(),
            temperature: 1.0,
            settled: false,
            scale: 1.0,
            offset: egui::Vec2::ZERO,
        }
    }

    /// 从基因组初始化位置：按 block 分组呈弧形排布，同 block 节点聚在一起
    pub fn init_from_genome(&mut self, genome: &Genome) {
        self.positions.clear();
        self.velocities.clear();
        self.temperature = 1.0;
        self.settled = false;

        // 按 block 分组
        let mut block_nodes: HashMap<i8, Vec<usize>> = HashMap::new();
        for node in &genome.nodes {
            let blk = blk_key(node);
            block_nodes.entry(blk).or_default().push(node.id);
        }

        let mut sorted_blocks: Vec<i8> = block_nodes.keys().copied().collect();
        sorted_blocks.sort();

        let radius = 180.0_f32;
        for (i, blk) in sorted_blocks.iter().enumerate() {
            let nodes = &block_nodes[blk];
            let angle_base = (i as f32 / sorted_blocks.len() as f32) * std::f32::consts::TAU
                - std::f32::consts::FRAC_PI_2;

            let n = nodes.len();
            // block 内节点在圆心角 ±10° 范围内散布
            let spread = 0.17_f32;
            for (j, &node_id) in nodes.iter().enumerate() {
                let offset_angle = if n > 1 {
                    (j as f32 / (n - 1) as f32 - 0.5) * spread
                } else {
                    0.0
                };
                let angle = angle_base + offset_angle;
                let r = radius * (1.0 + (j as f32 % 3.0) * 0.08);
                let pos = egui::pos2(angle.cos() * r, angle.sin() * r);
                self.positions.insert(node_id, pos);
                self.velocities.insert(node_id, egui::Vec2::ZERO);
            }
        }
    }

    /// 力模拟迭代（每帧在渲染前调用）
    pub fn step(&mut self, genome: &Genome, iterations: usize) {
        if self.settled || self.positions.is_empty() {
            return;
        }

        let k = 120.0_f64; // 理想边长

        for _ in 0..iterations {
            let node_ids: Vec<usize> = self.positions.keys().copied().collect();

            // 1. 全局斥力（Coulomb）
            for i in 0..node_ids.len() {
                for j in (i + 1)..node_ids.len() {
                    let a = node_ids[i];
                    let b = node_ids[j];
                    let delta = self.positions[&a] - self.positions[&b];
                    let dist = delta.length().max(0.01) as f64;
                    let force_mag = (k * k / dist) * self.temperature;
                    let capped = force_mag.min(200.0);
                    let f_vec = delta / dist as f32 * capped as f32;
                    *self.velocities.get_mut(&a).unwrap() += f_vec;
                    *self.velocities.get_mut(&b).unwrap() -= f_vec;
                }
            }

            // 2. 边吸引力（Hooke）
            for conn in &genome.connections {
                if !conn.enabled {
                    continue;
                }
                if let (Some(pos_a), Some(pos_b)) = (
                    self.positions.get(&conn.in_node),
                    self.positions.get(&conn.out_node),
                ) {
                    let delta = *pos_b - *pos_a;
                    let dist = delta.length().max(0.01) as f64;
                    let force_mag = (dist * dist / k) * self.temperature;
                    let capped = force_mag.min(50.0);
                    let f_vec = delta / dist as f32 * capped as f32;
                    *self.velocities.get_mut(&conn.in_node).unwrap() += f_vec;
                    *self.velocities.get_mut(&conn.out_node).unwrap() -= f_vec;
                }
            }

            // 3. 区块向心力（弱，让同 block 节点靠近但不挤成一团）
            let mut block_centers: FxHashMap<i8, (egui::Vec2, usize)> = FxHashMap::default();
            for node in &genome.nodes {
                let blk = blk_key(node);
                if let Some(pos) = self.positions.get(&node.id) {
                    let entry = block_centers.entry(blk).or_insert((egui::Vec2::ZERO, 0));
                    entry.0 += pos.to_vec2();
                    entry.1 += 1;
                }
            }
            for &(sum_pos, count) in block_centers.values() {
                if count < 2 {
                    continue;
                }
                let center = sum_pos / count as f32;
                let alpha = 0.04;
                for node in &genome.nodes {
                    if let Some(pos) = self.positions.get(&node.id) {
                        let delta = center - pos.to_vec2();
                        let dist = delta.length().max(0.01);
                        let force = dist * alpha * self.temperature as f32;
                        let f_vec = delta / dist * force;
                        *self.velocities.get_mut(&node.id).unwrap() += f_vec;
                    }
                }
            }

            // 4. 应用速度 + 阻尼
            let max_vel = k as f32 * 0.4;
            for id in &node_ids {
                let vel = *self.velocities.get(id).unwrap();
                let len = vel.length();
                let vel = if len > max_vel {
                    vel / len * max_vel
                } else {
                    vel
                };
                *self.positions.get_mut(id).unwrap() += vel * 0.25;
                *self.velocities.get_mut(id).unwrap() = vel * 0.55;
            }
        }

        self.temperature *= 0.93;
        if self.temperature < 0.008 {
            self.settled = true;
        }
    }

    /// 重置力模拟（重新收敛）
    pub fn reset(&mut self) {
        self.temperature = 1.0;
        self.settled = false;
    }
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

/// 在 egui::Window 内渲染分组力导图
///
/// 返回 `true` 表示用户关闭窗口。
pub fn render_force_graph_window(
    ui: &mut egui::Ui,
    genome: &Genome,
    state: &mut ForceGraphState,
    label: &str,
) -> bool {
    let fixed_w = 620.0;
    let mut close = false;

    egui::Window::new(format!("脑拓扑: {}", label))
        .resizable(true)
        .title_bar(true)
        .default_width(fixed_w)
        .min_width(400.0)
        .min_height(320.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            close = ui.button("× 关闭").clicked();

            let available = ui.available_size();
            let canvas_w = available.x.max(200.0);
            let canvas_h = available.y.max(200.0);

            let (response, painter) = ui.allocate_painter(
                egui::vec2(canvas_w, canvas_h),
                egui::Sense::click_and_drag(),
            );
            let rect = response.rect;

            // 背景
            painter.rect_filled(rect, 4.0, egui::Color32::from_gray(22));

            // 处理交互
            handle_interaction(state, &response, rect);

            // 首次初始化或 rect 尺寸变化时重新布局
            if !state.settled && state.positions.is_empty() {
                state.init_from_genome(genome);
            }

            // 力模拟步进
            if !state.settled {
                // 动态迭代次数：节点多则减少
                let iters = (10.0_f64 * (36.0 / state.positions.len().max(1) as f64).sqrt())
                    .clamp(2.0, 12.0) as usize;
                state.step(genome, iters);
            }

            // 坐标系转换：画布坐标 → 屏幕坐标
            let canvas_center = rect.center() + state.offset;
            let to_screen =
                |p: egui::Pos2| -> egui::Pos2 { canvas_center + (p.to_vec2() * state.scale) };

            // ── 绘制区块包围盒 ──
            draw_block_groups(genome, state, &painter, to_screen);

            // ── 绘制连接 ──
            draw_connections(genome, state, &painter, to_screen);

            // ── 绘制节点 ──
            draw_nodes(genome, state, &painter, to_screen);

            // ── hover 提示 ──
            draw_hover_tooltip(genome, state, &response, rect, &painter, to_screen);

            // 图例
            draw_legend(&painter, rect);
        });

    close
}

// ---------------------------------------------------------------------------
// 交互
// ---------------------------------------------------------------------------

fn handle_interaction(state: &mut ForceGraphState, response: &egui::Response, rect: egui::Rect) {
    let cc = rect.center();

    // 左键拖拽平移
    if response.dragged_by(egui::PointerButton::Primary) {
        state.offset += response.drag_delta();
        state.reset();
    }

    // 滚轮缩放（围绕鼠标位置）
    if let Some(hover) = response.hover_pos() {
        if rect.contains(hover) {
            let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let old_scale = state.scale;
                let new_scale = (old_scale * (1.0 + scroll * 0.15)).clamp(0.15, 4.0);
                // 坐标变换: screen = cc + offset + world * scale
                // 保持鼠标下的世界点不动:
                //   world = (hover - cc - offset) / old_scale
                //   new_offset = hover - cc - world * new_scale
                let to_cc = hover - cc;
                let ratio = new_scale / old_scale;
                state.offset = to_cc - (to_cc - state.offset) * ratio;
                state.scale = new_scale;
                state.reset();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 绘制区块包围盒
// ---------------------------------------------------------------------------

fn draw_block_groups(
    genome: &Genome,
    state: &ForceGraphState,
    painter: &egui::Painter,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) {
    let mut block_nodes: HashMap<i8, Vec<&NodeGene>> = HashMap::new();
    for node in &genome.nodes {
        let blk = blk_key(node);
        if state.positions.contains_key(&node.id) {
            block_nodes.entry(blk).or_default().push(node);
        }
    }

    for (&blk, nodes) in &block_nodes {
        if nodes.is_empty() {
            continue;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &node in nodes {
            if let Some(p) = state.positions.get(&node.id) {
                let sp = to_screen(*p);
                let r = node_radius(node);
                min_x = min_x.min(sp.x - r - 6.0);
                min_y = min_y.min(sp.y - r - 6.0);
                max_x = max_x.max(sp.x + r + 6.0);
                max_y = max_y.max(sp.y + r + 6.0);
            }
        }

        if min_x >= max_x || min_y >= max_y {
            continue;
        }

        let bbox = egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y));
        let pad = 14.0;
        let bbox = bbox.expand(pad);

        let color = block_color(blk);
        // 半透明填充
        painter.rect_filled(bbox, 6.0, color.gamma_multiply(0.12));
        // 边框
        painter.rect_stroke(bbox, 6.0, egui::Stroke::new(1.5, color.gamma_multiply(0.6)));

        // 标签
        let label = block_label(blk, nodes.len());
        painter.text(
            egui::pos2(bbox.left() + 4.0, bbox.top() + 2.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(10.0),
            color.gamma_multiply(0.9),
        );
    }
}

// ---------------------------------------------------------------------------
// 绘制连接
// ---------------------------------------------------------------------------

fn draw_connections(
    genome: &Genome,
    state: &ForceGraphState,
    painter: &egui::Painter,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) {
    for conn in &genome.connections {
        if !conn.enabled {
            continue;
        }
        if let (Some(pa), Some(pb)) = (
            state.positions.get(&conn.in_node),
            state.positions.get(&conn.out_node),
        ) {
            let a = to_screen(*pa);
            let b = to_screen(*pb);
            let width = (conn.weight.abs() as f32 * 2.5).clamp(0.8, 4.0);
            let alpha = (conn.weight.abs() as f32 * 0.6).clamp(0.15, 0.9);
            let color = if conn.weight > 0.0 {
                egui::Color32::from_rgba_premultiplied(100, 200, 255, (alpha * 255.0) as u8)
            } else {
                egui::Color32::from_rgba_premultiplied(255, 120, 100, (alpha * 255.0) as u8)
            };
            painter.line_segment([a, b], egui::Stroke::new(width, color));
        }
    }
}

// ---------------------------------------------------------------------------
// 绘制节点
// ---------------------------------------------------------------------------

fn draw_nodes(
    genome: &Genome,
    state: &ForceGraphState,
    painter: &egui::Painter,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) {
    for node in &genome.nodes {
        if let Some(p) = state.positions.get(&node.id) {
            let sp = to_screen(*p);
            let r = node_radius(node);
            let color = node_color(node);

            // 投影到视口
            painter.circle_filled(sp, r, color);
            // 边框
            painter.circle_stroke(sp, r, egui::Stroke::new(1.0, color.gamma_multiply(0.7)));
        }
    }
}

// ---------------------------------------------------------------------------
// Hover 提示
// ---------------------------------------------------------------------------

fn draw_hover_tooltip(
    genome: &Genome,
    state: &ForceGraphState,
    response: &egui::Response,
    _rect: egui::Rect,
    painter: &egui::Painter,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) {
    let hover_pos = match response.hover_pos() {
        Some(p) => p,
        None => return,
    };

    let threshold = 12.0_f32; // hover 检测半径

    for node in &genome.nodes {
        if let Some(p) = state.positions.get(&node.id) {
            let sp = to_screen(*p);
            let dist = (hover_pos - sp).length();
            if dist < threshold {
                // 高亮节点
                painter.circle_filled(sp, node_radius(node) + 2.0, egui::Color32::WHITE);

                // tooltip 文本
                let tip = node_tooltip(node);
                let tip_pos = sp + egui::vec2(14.0, -8.0);
                painter.text(
                    tip_pos,
                    egui::Align2::LEFT_BOTTOM,
                    tip,
                    egui::FontId::proportional(11.0),
                    egui::Color32::WHITE,
                );
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 图例
// ---------------------------------------------------------------------------

fn draw_legend(painter: &egui::Painter, rect: egui::Rect) {
    let x = rect.left() + 6.0;
    let y = rect.bottom() - 14.0;

    let items = [
        (egui::Color32::from_rgb(100, 220, 130), "感官"),
        (egui::Color32::from_rgb(180, 180, 200), "隐层"),
        (egui::Color32::from_rgb(255, 180, 80), "运动"),
        (egui::Color32::from_rgb(100, 200, 255), "兴奋+"),
        (egui::Color32::from_rgb(255, 120, 100), "抑制-"),
    ];

    let mut x_off = x;
    for (color, label) in &items {
        painter.circle_filled(egui::pos2(x_off, y), 4.0, *color);
        painter.text(
            egui::pos2(x_off + 7.0, y),
            egui::Align2::LEFT_CENTER,
            *label,
            egui::FontId::proportional(9.0),
            egui::Color32::from_gray(160),
        );
        x_off += 60.0;
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 节点的 block 键值（Input/Output 用特殊标记）
fn blk_key(node: &NodeGene) -> i8 {
    match node.node_type {
        NodeType::Input => -100,
        NodeType::Output => 100,
        NodeType::Block(b) => b,
    }
}

fn node_radius(node: &NodeGene) -> f32 {
    match node.node_type {
        NodeType::Input | NodeType::Output => 6.5,
        NodeType::Block(_) => 5.0,
    }
}

fn node_color(node: &NodeGene) -> egui::Color32 {
    match node.node_type {
        NodeType::Input => egui::Color32::from_rgb(100, 220, 130), // 感官绿
        NodeType::Output => egui::Color32::from_rgb(255, 180, 80), // 运动橙
        NodeType::Block(b) => block_color(b),
    }
}

fn block_color(blk: i8) -> egui::Color32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    blk.hash(&mut h);
    let v = h.finish();
    let r = ((v >> 16) & 0xFF) as u8;
    let g = ((v >> 8) & 0xFF) as u8;
    let b = (v & 0xFF) as u8;
    // 保底亮度
    let brighten = |c: u8| -> u8 { c.clamp(60, 220) };
    egui::Color32::from_rgb(brighten(r), brighten(g), brighten(b))
}

fn block_label(blk: i8, count: usize) -> String {
    let role = match blk {
        -100 => "感官输入",
        100 => "运动输出",
        0 => "体感",
        b if b.abs() <= 3 => {
            if b < 0 {
                "左感官"
            } else {
                "右感官"
            }
        }
        b if b.abs() <= 7 => {
            if b < 0 {
                "左初级"
            } else {
                "右初级"
            }
        }
        b if b.abs() <= 24 => {
            if b < 0 {
                "左联合"
            } else {
                "右联合"
            }
        }
        b => {
            if b < 0 {
                "左运动"
            } else {
                "右运动"
            }
        }
    };
    format!("B{} {}  {}n", blk, role, count)
}

fn node_tooltip(node: &NodeGene) -> String {
    let type_str = match node.node_type {
        NodeType::Input => "Input".to_string(),
        NodeType::Output => "Output".to_string(),
        NodeType::Block(b) => format!("Block {}", b),
    };
    format!(
        "id:{} {}\ndecay:{:.3} thr:{:.2} ref:{}",
        node.id, type_str, node.decay, node.threshold, node.refractory_period
    )
}
