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
use std::collections::VecDeque;

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
    /// 每个 block 节点的锚定目标位置（半球横向 + I/O 纵向）
    anchor_targets: FxHashMap<usize, egui::Vec2>,
    /// 水平锚定强度（左右拉扯），打开弹框时从 config 更新
    pub h_anchor: f64,
    /// 垂直锚定强度（上下拉扯），打开弹框时从 config 更新
    pub v_anchor: f64,
    /// 当前 genome 指纹（检测变更后自动重置布局）
    genome_fingerprint: u64,
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
            anchor_targets: FxHashMap::default(),
            h_anchor: 3.0,
            v_anchor: 3.0,
            genome_fingerprint: 0,
        }
    }

    /// 从基因组初始化：跳过 Input/Output 桩节点，
    /// 根据左/右半球分配横向锚定、BFS 深度分配纵向锚定。
    /// `canvas_size` 用于自适应布局跨度。
    pub fn init_from_genome(&mut self, genome: &Genome, canvas_size: egui::Vec2) {
        self.positions.clear();
        self.velocities.clear();
        self.anchor_targets.clear();
        self.temperature = 1.0;
        self.settled = false;

        let radius = canvas_size.x.min(canvas_size.y).max(100.0) * 0.42;
        let h_base = radius * 0.5; // 水平半球跨度
        let v_base = radius * 0.65; // 垂直 I/O 跨度

        // 快速节点类型查询
        let node_types: FxHashMap<usize, &NodeType> =
            genome.nodes.iter().map(|n| (n.id, &n.node_type)).collect();

        // 构建出入邻接表（仅 block ↔ block，跳过桩节点）
        let mut outgoing: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        let mut incoming: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        for conn in &genome.connections {
            if !conn.enabled {
                continue;
            }
            let in_is_block = matches!(node_types.get(&conn.in_node), Some(NodeType::Block(_)));
            let out_is_block = matches!(node_types.get(&conn.out_node), Some(NodeType::Block(_)));
            if in_is_block && out_is_block {
                outgoing
                    .entry(conn.in_node)
                    .or_default()
                    .push(conn.out_node);
                incoming
                    .entry(conn.out_node)
                    .or_default()
                    .push(conn.in_node);
            }
        }

        // BFS 前向：每个 block 节点离 Input 的最短距离
        let mut in_dist: FxHashMap<usize, u32> = FxHashMap::default();
        {
            let mut q: VecDeque<usize> = VecDeque::new();
            // 从直连 Input 的 block 节点开始
            for conn in &genome.connections {
                if !conn.enabled {
                    continue;
                }
                if matches!(node_types.get(&conn.in_node), Some(NodeType::Input)) {
                    if !in_dist.contains_key(&conn.out_node) {
                        in_dist.insert(conn.out_node, 1);
                        q.push_back(conn.out_node);
                    }
                }
            }
            while let Some(cur) = q.pop_front() {
                let d = in_dist[&cur];
                if let Some(outs) = outgoing.get(&cur) {
                    for &next in outs {
                        if !in_dist.contains_key(&next) {
                            in_dist.insert(next, d + 1);
                            q.push_back(next);
                        }
                    }
                }
            }
        }

        // BFS 反向：每个 block 节点离 Output 的最短距离
        let mut out_dist: FxHashMap<usize, u32> = FxHashMap::default();
        {
            let mut q: VecDeque<usize> = VecDeque::new();
            for conn in &genome.connections {
                if !conn.enabled {
                    continue;
                }
                if matches!(node_types.get(&conn.out_node), Some(NodeType::Output)) {
                    if !out_dist.contains_key(&conn.in_node) {
                        out_dist.insert(conn.in_node, 1);
                        q.push_back(conn.in_node);
                    }
                }
            }
            while let Some(cur) = q.pop_front() {
                let d = out_dist[&cur];
                if let Some(ins) = incoming.get(&cur) {
                    for &prev in ins {
                        if !out_dist.contains_key(&prev) {
                            out_dist.insert(prev, d + 1);
                            q.push_back(prev);
                        }
                    }
                }
            }
        }

        // 统计各 block 节点数（用于块内初始散布）
        let mut block_counts: HashMap<i8, usize> = HashMap::new();
        for node in &genome.nodes {
            if let NodeType::Block(b) = node.node_type {
                *block_counts.entry(b).or_insert(0) += 1;
            }
        }

        // 仅处理 block 节点，跳过 Input/Output 桩
        let mut block_idx: HashMap<i8, usize> = HashMap::new();
        for node in &genome.nodes {
            let b = match node.node_type {
                NodeType::Block(b) => b,
                _ => continue,
            };
            let id = node.id;
            let idx = block_idx.entry(b).or_insert(0);
            let blk_count = block_counts.get(&b).copied().unwrap_or(1);

            // 横向：左半脑左拉，右半脑右拉，|blk| 深度越大略拉开
            let sign = if b < 0 {
                -1.0
            } else if b > 0 {
                1.0
            } else {
                0.0
            };
            let depth = b.unsigned_abs() as f32 / 26.0;
            let tx = sign * h_base * (1.0 + depth * 0.3);

            // 纵向：BFS 深度比率 → 离 Input 近则向上，离 Output 近则向下
            // 使用 sqrt 非线性映射将中等深度的节点推向两端
            let ty = match (in_dist.get(&id), out_dist.get(&id)) {
                (Some(d_in), Some(d_out)) => {
                    let bias = (*d_in as f32 - *d_out as f32) / (*d_in + *d_out) as f32;
                    let pushed = bias.signum() * bias.abs().powf(0.45);
                    pushed * v_base
                }
                (Some(_), None) => -v_base, // 仅可达 Input → 顶部
                (None, Some(_)) => v_base,  // 仅可达 Output → 底部
                (None, None) => 0.0,        // 孤立节点 → 居中
            };

            self.anchor_targets.insert(id, egui::vec2(tx, ty));

            // 初始位置：锚定点附近小范围散布
            let spread = 20.0_f32;
            let angle = *idx as f32 * 2.399;
            let r = if blk_count > 1 {
                spread * (1.0 + (*idx % 3) as f32 * 0.2)
            } else {
                0.0
            };
            let pos = egui::pos2(tx + angle.cos() * r, ty + angle.sin() * r);
            self.positions.insert(id, pos);
            self.velocities.insert(id, egui::Vec2::ZERO);

            *idx += 1;
        }

        self.genome_fingerprint = genome_fingerprint(genome);
    }

    /// 力模拟迭代（每帧在渲染前调用）
    pub fn step(&mut self, genome: &Genome, iterations: usize) {
        if self.settled || self.positions.is_empty() {
            return;
        }

        let k = 120.0_f64;

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

            // 4. 全局锚定力：每个 block 节点拉向其半球×层深锚定目标
            for (&id, &target) in &self.anchor_targets {
                if let Some(pos) = self.positions.get(&id) {
                    if let Some(vel) = self.velocities.get_mut(&id) {
                        let delta = target - pos.to_vec2();
                        vel.x += delta.x * self.h_anchor as f32 * self.temperature as f32;
                        vel.y += delta.y * self.v_anchor as f32 * self.temperature as f32;
                    }
                }
            }

            // 5. 应用速度 + 阻尼
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
    let fixed_w = 1860.0;
    let mut close = false;

    egui::Window::new(format!("脑拓扑: {}", label))
        .resizable(true)
        .title_bar(false)
        .default_width(fixed_w)
        .min_width(1200.0)
        .min_height(1100.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            // 自定义标题栏（参考偏好弹框）
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!("脑拓扑: {}", label)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("x")
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(200)),
                            )
                            .frame(false)
                            .fill(egui::Color32::from_gray(50))
                            .small(),
                        )
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            ui.separator();

            let available = ui.available_size();
            let canvas_w = available.x.max(400.0);
            let canvas_h = available.y.max(400.0);

            let (response, painter) = ui.allocate_painter(
                egui::vec2(canvas_w, canvas_h),
                egui::Sense::click_and_drag(),
            );
            let rect = response.rect;

            // 背景
            painter.rect_filled(rect, 4.0, egui::Color32::from_gray(22));

            // 处理交互
            handle_interaction(state, &response, rect);

            // 首次初始化或 genome 变更时重新布局
            if state.positions.is_empty() || state.genome_fingerprint != genome_fingerprint(genome)
            {
                state.init_from_genome(genome, rect.size());
            }

            // 力模拟步进
            if !state.settled {
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
    }

    // 滚轮缩放（围绕鼠标位置）
    if let Some(hover) = response.hover_pos() {
        if rect.contains(hover) {
            let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let old_scale = state.scale;
                let new_scale = (old_scale * (1.0 + scroll * 0.02)).clamp(0.15, 4.0);
                // 坐标变换: screen = cc + offset + world * scale
                // 保持鼠标下的世界点不动:
                //   world = (hover - cc - offset) / old_scale
                //   new_offset = hover - cc - world * new_scale
                let to_cc = hover - cc;
                let ratio = new_scale / old_scale;
                state.offset = to_cc - (to_cc - state.offset) * ratio;
                state.scale = new_scale;
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
        // 已知功能区块
        -1 => "左眼",
        1 => "右眼",
        -2 => "左光耳",
        2 => "右光耳",
        -3 => "内省",
        3 => "外感",
        -25 => "繁殖",
        25 => "运动",
        -26 => "发光",
        // 通用分类
        b if b.abs() <= 3 => {
            if b < 0 {
                "左感官"
            } else {
                "右感官"
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
    format!("B{} {} {}n", blk, role, count)
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

/// 计算 genome 简易指纹（检测变更后自动重新布局）
fn genome_fingerprint(genome: &Genome) -> u64 {
    let mut fp: u64 = 0;
    for node in &genome.nodes {
        fp = fp.wrapping_mul(31).wrapping_add(node.id as u64);
    }
    for conn in &genome.connections {
        fp = fp.wrapping_mul(31).wrapping_add(conn.in_node as u64);
        fp = fp.wrapping_mul(31).wrapping_add(conn.out_node as u64);
    }
    fp
}
