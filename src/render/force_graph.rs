//! 分组力导图组件（block 锚定 + d3-force 模型）
//!
//! 设计：
//! - 大格局：每个 block 按编号 b 映射到一个连续梯度的目标坐标
//!   - x 按 sign(b) × W/2 × f(|b|/26)，f 从 1/3 起步单调递增（中轴 → 外缘）
//!   - y 按 |b| 分三段：感官在上 1/3、联合在中 1/3、运动在下 1/3
//! - 细节：节点间斥力、边吸引、forceX/forceY 锚定（同 block 节点共享目标）
//! - alpha 只乘在位置更新上，不缩放力 → 平衡点唯一、结果可复现
//! - alpha 留底（alpha_min）→ 系统永不冻结，支持手动拖动后重排

use crate::neural::block::{motor_block_for_output, sensory_block_for_input};
use crate::neural::genome::{Genome, NodeGene, NodeType};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

/// Input 节点作用标签（20 维感知，索引=node.id）
const INPUT_LABELS: [&str; 20] = [
    "L眼角",   // 0
    "L接近",   // 1
    "L能量",   // 2
    "L类型",   // 3
    "L相似",   // 4
    "L密度",   // 5
    "L朝向差", // 6
    "L速度差", // 7
    "R眼角",   // 8
    "R接近",   // 9
    "R能量",   // 10
    "R类型",   // 11
    "R相似",   // 12
    "R密度",   // 13
    "R朝向差", // 14
    "R速度差", // 15
    "自能量",  // 16
    "地坡度",  // 17
    "光耳角",  // 18
    "光耳强",  // 19
];

/// Output 节点作用标签（8 维动作，索引=node.id - INPUT_SIZE）
const OUTPUT_LABELS: [&str; 8] = [
    "转向",   // 0
    "速度",   // 1
    "嘴",     // 2
    "繁殖",   // 3
    "繁阈值", // 4
    "子能比", // 5
    "痕迹",   // 6
    "光嘴",   // 7
];

/// 节点对应的 block 编号（Input/Output 走投射映射，Block 节点直接取自身）
fn node_block(node: &NodeGene) -> i8 {
    match node.node_type {
        NodeType::Input => sensory_block_for_input(node.id),
        NodeType::Output => motor_block_for_output(node.id.saturating_sub(Genome::INPUT_SIZE)),
        NodeType::Block(b) => b,
    }
}

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

const ALPHA_MIN: f64 = 0.05;
const ALPHA_DECAY: f64 = 0.97;
const ALPHA_REHEAT: f64 = 0.3;
const NODE_PICK_RADIUS: f32 = 12.0;
const HASH_JITTER_RANGE: f32 = 60.0;
/// IO 节点与 Block 节点最高/最低点之间的安全距离（世界坐标）
/// 每帧动态计算：input.y = block_min_y - LOCKED_OFFSET，output.y = block_max_y + LOCKED_OFFSET
const LOCKED_OFFSET: f32 = 60.0;

// ---------------------------------------------------------------------------
// 力模拟状态（跨帧持久化）
// ---------------------------------------------------------------------------

pub struct ForceGraphState {
    positions: FxHashMap<usize, egui::Pos2>,
    velocities: FxHashMap<usize, egui::Vec2>,
    /// 每个 block 的目标坐标（由 b 值的连续梯度公式决定，init 时一次性算）
    block_targets: FxHashMap<i8, egui::Vec2>,
    /// 节点是否被钉住（拖动后保持位置）
    pinned: FxHashMap<usize, bool>,
    /// 当前正在拖动的节点
    dragging_node: Option<usize>,
    /// alpha：只乘在位置更新上，留底 ALPHA_MIN 保持系统响应性
    alpha: f64,
    scale: f32,
    offset: egui::Vec2,
    /// 横向锚定强度（forceX strength）
    pub h_anchor: f64,
    /// 纵向锚定强度（forceY strength）
    pub v_anchor: f64,
    /// 单 iter 速度上限。太小会截断强锚定力（让 v_anchor 增大失效），由 config 注入
    pub max_vel: f64,
    /// 硬锁节点（Input/Output 钉死在画布顶/底，不受力影响、不可拖、不可右键解锁）
    locked_nodes: FxHashSet<usize>,
    /// genome 指纹（拓扑变化时自动重新初始化）
    genome_fingerprint: u64,
}

impl ForceGraphState {
    pub fn new() -> Self {
        Self {
            positions: FxHashMap::default(),
            velocities: FxHashMap::default(),
            block_targets: FxHashMap::default(),
            pinned: FxHashMap::default(),
            dragging_node: None,
            alpha: 1.0,
            scale: 1.0,
            offset: egui::Vec2::ZERO,
            h_anchor: 0.08,
            v_anchor: 0.08,
            max_vel: 240.0,
            locked_nodes: FxHashSet::default(),
            genome_fingerprint: 0,
        }
    }

    /// 关弹框时调用，清空所有状态，下次打开重新初始化
    pub fn reset(&mut self) {
        self.positions.clear();
        self.velocities.clear();
        self.block_targets.clear();
        self.pinned.clear();
        self.dragging_node = None;
        self.alpha = 1.0;
        self.scale = 1.0;
        self.offset = egui::Vec2::ZERO;
        self.locked_nodes.clear();
        self.genome_fingerprint = 0;
    }

    /// 计算 block 编号 b 对应的目标坐标
    ///
    /// X：sign(b) × W/2 × f(|b|/26)，f(t) = 1/3 + 2/3·√t
    ///   - |b|=1  → x ≈ ±W/2 × 0.46（最靠中轴）
    ///   - |b|=25 → x ≈ ±W/2 × 0.99
    ///   - |b|=26 → x = ±W/2
    /// Y：分三段连续映射
    ///   - 感官 |b|∈[1,3]   → 上方收紧到 [-H/2, -H/3]，与联合区中线之间留出 H/6 安全带
    ///     避免感官块被边吸引（连到联合/运动）轻易拉过中线
    ///   - 联合 |b|∈[4,24]  → 中 1/3 区，[-H/6, +H/6]，|b| 大者偏下
    ///   - 运动 |b|∈[25,31] → 下 1/3 区，[+H/6, +H/2]，|b| 大者偏下
    fn compute_block_target(b: i8, canvas: egui::Vec2) -> egui::Vec2 {
        let half_w = canvas.x * 0.5;
        let half_h = canvas.y * 0.5;
        let ab = b.unsigned_abs() as f32;

        let t_x = (ab / 26.0).min(1.0);
        let fx = 1.0 / 3.0 + 2.0 / 3.0 * t_x.sqrt();
        let sign = if b < 0 { -1.0 } else { 1.0 };
        let x = sign * half_w * fx;

        let y = if ab <= 3.0 {
            // |b|=1 → -H/2（最上）；|b|=3 → -H/3（仍在中线上方 1/3 屏高处）
            let t = ((ab - 1.0) / 2.0).clamp(0.0, 1.0);
            -half_h * (1.0 - t / 3.0)
        } else if ab <= 24.0 {
            let t = (ab - 4.0) / 20.0;
            half_h * (-1.0 / 3.0 + 2.0 / 3.0 * t)
        } else {
            let t = ((ab - 25.0) / 6.0).clamp(0.0, 1.0);
            half_h * (1.0 / 3.0 + 2.0 / 3.0 * t)
        };

        egui::vec2(x, y)
    }

    /// 从基因组初始化布局
    ///
    /// - Input/Output 节点：等距钉死在画布顶/底排，按 (block, id) 升序左→右排列，加入 locked_nodes
    /// - Block 节点：按 block_target 锚点 + 哈希抖动初始位置，正常受力
    pub fn init_from_genome(&mut self, genome: &Genome, canvas_size: egui::Vec2) {
        self.positions.clear();
        self.velocities.clear();
        self.block_targets.clear();
        self.pinned.clear();
        self.locked_nodes.clear();
        self.dragging_node = None;
        self.alpha = 1.0;

        let canvas = egui::vec2(canvas_size.x.max(400.0), canvas_size.y.max(400.0));
        let half_w = canvas.x * 0.5;
        let half_h = canvas.y * 0.5;

        // 算所有 block 锚点（含 Input/Output 投射的感官/运动 block，用于 Block 节点锚定）
        for node in &genome.nodes {
            let b = node_block(node);
            self.block_targets
                .entry(b)
                .or_insert_with(|| Self::compute_block_target(b, canvas));
        }

        // —— Input/Output 节点：硬锁在顶/底排，按 (block, id) 升序等距排列 ——
        let mut inputs: Vec<&NodeGene> = genome
            .nodes
            .iter()
            .filter(|n| matches!(n.node_type, NodeType::Input))
            .collect();
        let mut outputs: Vec<&NodeGene> = genome
            .nodes
            .iter()
            .filter(|n| matches!(n.node_type, NodeType::Output))
            .collect();
        inputs.sort_by_key(|n| (node_block(n), n.id));
        outputs.sort_by_key(|n| (node_block(n), n.id));

        // y 边距 = 5% 屏高，保证标签不超出画布
        let in_y = -half_h * 0.95;
        let in_n = inputs.len().max(1) as f32;
        for (i, node) in inputs.iter().enumerate() {
            let x = -half_w + (i as f32 + 0.5) / in_n * canvas.x;
            self.positions.insert(node.id, egui::pos2(x, in_y));
            self.velocities.insert(node.id, egui::Vec2::ZERO);
            self.locked_nodes.insert(node.id);
        }
        let out_y = half_h * 0.95;
        let out_n = outputs.len().max(1) as f32;
        for (i, node) in outputs.iter().enumerate() {
            let x = -half_w + (i as f32 + 0.5) / out_n * canvas.x;
            self.positions.insert(node.id, egui::pos2(x, out_y));
            self.velocities.insert(node.id, egui::Vec2::ZERO);
            self.locked_nodes.insert(node.id);
        }

        // —— Block 节点：按 block 锚点 + 哈希抖动放置，参与力学 ——
        for node in &genome.nodes {
            if !matches!(node.node_type, NodeType::Block(_)) {
                continue;
            }
            let b = node_block(node);
            let target = self.block_targets[&b];
            let (dx, dy) = hash_jitter(node.id);
            let pos = egui::pos2(target.x + dx, target.y + dy);
            self.positions.insert(node.id, pos);
            self.velocities.insert(node.id, egui::Vec2::ZERO);
        }

        self.genome_fingerprint = genome_fingerprint(genome);
    }

    /// 力模拟步进：斥力 + 边吸引 + block 锚定（恒定强度），alpha 只乘位置更新
    pub fn step(&mut self, genome: &Genome, iterations: usize) {
        if self.positions.is_empty() {
            return;
        }

        let k = 120.0_f64;

        // 节点 ID 按值排序，消除浮点累加顺序漂移
        let mut node_ids: Vec<usize> = self.positions.keys().copied().collect();
        node_ids.sort_unstable();

        for _ in 0..iterations {
            // 1. 斥力（Coulomb，恒定强度）
            for i in 0..node_ids.len() {
                for j in (i + 1)..node_ids.len() {
                    let a = node_ids[i];
                    let b = node_ids[j];
                    let delta = self.positions[&a] - self.positions[&b];
                    let dist = delta.length().max(0.01) as f64;
                    let force_mag = (k * k / dist).min(200.0);
                    let f_vec = delta / dist as f32 * force_mag as f32;
                    *self.velocities.get_mut(&a).unwrap() += f_vec;
                    *self.velocities.get_mut(&b).unwrap() -= f_vec;
                }
            }

            // 2. 边吸引（Hooke 形式，恒定强度）
            for conn in &genome.connections {
                if !conn.enabled {
                    continue;
                }
                let (pos_a, pos_b) = match (
                    self.positions.get(&conn.in_node),
                    self.positions.get(&conn.out_node),
                ) {
                    (Some(a), Some(b)) => (*a, *b),
                    _ => continue,
                };
                let delta = pos_b - pos_a;
                let dist = delta.length().max(0.01) as f64;
                let force_mag = (dist * dist / k).min(50.0);
                let f_vec = delta / dist as f32 * force_mag as f32;
                *self.velocities.get_mut(&conn.in_node).unwrap() += f_vec;
                *self.velocities.get_mut(&conn.out_node).unwrap() -= f_vec;
            }

            // 3. block 锚定（forceX/forceY，恒定强度，对 Input/Output 也走投射 block 锚定）
            for node in &genome.nodes {
                let b = node_block(node);
                let target = match self.block_targets.get(&b) {
                    Some(t) => *t,
                    None => continue,
                };
                let pos = match self.positions.get(&node.id) {
                    Some(p) => *p,
                    None => continue,
                };
                if let Some(vel) = self.velocities.get_mut(&node.id) {
                    vel.x += (target.x - pos.x) * self.h_anchor as f32;
                    vel.y += (target.y - pos.y) * self.v_anchor as f32;
                }
            }

            // 4. 应用速度 + 阻尼，alpha 只乘位置更新；pinned/dragging 节点速度归零
            let max_vel = self.max_vel as f32;
            for id in &node_ids {
                if Some(*id) == self.dragging_node
                    || self.pinned.get(id).copied().unwrap_or(false)
                    || self.locked_nodes.contains(id)
                {
                    *self.velocities.get_mut(id).unwrap() = egui::Vec2::ZERO;
                    continue;
                }
                let vel = *self.velocities.get(id).unwrap();
                let len = vel.length();
                let vel = if len > max_vel {
                    vel / len * max_vel
                } else {
                    vel
                };
                *self.positions.get_mut(id).unwrap() += vel * self.alpha as f32;
                *self.velocities.get_mut(id).unwrap() = vel * 0.55;
            }
        }

        // —— 动态 IO 行：贴当前 Block 节点 min_y / max_y 的外侧 LOCKED_OFFSET ——
        // 每帧末算一次，让 Input/Output 永远在所有 Block 节点之上/之下 D 距离
        let mut block_min_y = f32::MAX;
        let mut block_max_y = f32::MIN;
        for (id, pos) in &self.positions {
            if !self.locked_nodes.contains(id) {
                block_min_y = block_min_y.min(pos.y);
                block_max_y = block_max_y.max(pos.y);
            }
        }
        if block_min_y < f32::MAX {
            let in_y = block_min_y - LOCKED_OFFSET;
            let out_y = block_max_y + LOCKED_OFFSET;
            for node in &genome.nodes {
                let target_y = match node.node_type {
                    NodeType::Input => in_y,
                    NodeType::Output => out_y,
                    _ => continue,
                };
                if let Some(pos) = self.positions.get_mut(&node.id) {
                    pos.y = target_y;
                }
            }
        }

        // alpha 衰减但留底，永不冻结
        self.alpha = (self.alpha * ALPHA_DECAY).max(ALPHA_MIN);
    }

    fn reheat(&mut self) {
        if self.alpha < ALPHA_REHEAT {
            self.alpha = ALPHA_REHEAT;
        }
    }
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

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

            painter.rect_filled(rect, 4.0, egui::Color32::from_gray(22));

            // 首次或 genome 变更时初始化
            if state.positions.is_empty() || state.genome_fingerprint != genome_fingerprint(genome)
            {
                state.init_from_genome(genome, rect.size());
            }

            // 处理交互（拖动节点 / 平移画布 / 缩放 / 右键 pinned）
            handle_interaction(state, &response, rect);

            // 每帧步进：alpha 留底，永不停止
            let iters = (10.0_f64 * (36.0 / state.positions.len().max(1) as f64).sqrt())
                .clamp(2.0, 12.0) as usize;
            state.step(genome, iters);

            // 坐标变换
            let canvas_center = rect.center() + state.offset;
            let to_screen =
                |p: egui::Pos2| -> egui::Pos2 { canvas_center + (p.to_vec2() * state.scale) };

            draw_block_groups(genome, state, &painter, to_screen);
            draw_connections(genome, state, &painter, to_screen);
            draw_nodes(genome, state, &painter, to_screen);
            draw_io_labels(genome, state, &painter, to_screen);
            draw_hover_tooltip(genome, state, &response, rect, &painter, to_screen);
            draw_legend(&painter, rect);
        });

    close
}

// ---------------------------------------------------------------------------
// 交互
// ---------------------------------------------------------------------------

fn handle_interaction(state: &mut ForceGraphState, response: &egui::Response, rect: egui::Rect) {
    let cc = rect.center();
    let canvas_center = cc + state.offset;
    let scale = state.scale;
    let to_screen = |p: egui::Pos2| -> egui::Pos2 { canvas_center + (p.to_vec2() * scale) };
    let to_world = |sp: egui::Pos2| -> egui::Pos2 {
        egui::pos2(
            (sp.x - canvas_center.x) / scale,
            (sp.y - canvas_center.y) / scale,
        )
    };

    // === 右键节点：切换 pinned（locked 节点不响应：始终硬钉死）===
    if response.clicked_by(egui::PointerButton::Secondary) {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(id) = pick_node(state, pos, to_screen) {
                if !state.locked_nodes.contains(&id) {
                    let was = state.pinned.get(&id).copied().unwrap_or(false);
                    state.pinned.insert(id, !was);
                    state.reheat();
                }
                return;
            }
        }
    }

    // === 拖拽中的节点：跟随鼠标 ===
    if let Some(id) = state.dragging_node {
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(sp) = response.interact_pointer_pos() {
                let wp = to_world(sp);
                if let Some(pos) = state.positions.get_mut(&id) {
                    *pos = wp;
                }
                if let Some(vel) = state.velocities.get_mut(&id) {
                    *vel = egui::Vec2::ZERO;
                }
                state.reheat();
            }
        } else {
            // 拖拽结束：钉住节点（用户可再次右键解锁）
            state.pinned.insert(id, true);
            state.dragging_node = None;
            state.reheat();
        }
        return;
    }

    // === 拖拽开始：在节点上 → 节点拖（locked 节点不可拖）；空白 → 画布平移 ===
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(sp) = response.interact_pointer_pos() {
            if let Some(id) = pick_node(state, sp, to_screen) {
                if !state.locked_nodes.contains(&id) {
                    state.dragging_node = Some(id);
                    state.reheat();
                    return;
                }
            }
        }
    }

    // 左键拖拽（不在节点上）→ 平移画布
    if response.dragged_by(egui::PointerButton::Primary) {
        state.offset += response.drag_delta();
    }

    // 滚轮缩放
    if let Some(hover) = response.hover_pos() {
        if rect.contains(hover) {
            let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let old_scale = state.scale;
                let new_scale = (old_scale * (1.0 + scroll * 0.02)).clamp(0.15, 4.0);
                let to_cc = hover - cc;
                let ratio = new_scale / old_scale;
                state.offset = to_cc - (to_cc - state.offset) * ratio;
                state.scale = new_scale;
            }
        }
    }
}

fn pick_node(
    state: &ForceGraphState,
    screen_pos: egui::Pos2,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (&id, &pos) in &state.positions {
        let sp = to_screen(pos);
        let dist = (screen_pos - sp).length();
        if dist < NODE_PICK_RADIUS {
            match best {
                None => best = Some((id, dist)),
                Some((_, bd)) if dist < bd => best = Some((id, dist)),
                _ => {}
            }
        }
    }
    best.map(|(id, _)| id)
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
    // 仅聚合 Block 节点：Input/Output 钉在画布顶/底，框进来会让 bounding box 贯穿画布
    let mut block_nodes: HashMap<i8, Vec<&NodeGene>> = HashMap::new();
    for node in &genome.nodes {
        if !matches!(node.node_type, NodeType::Block(_)) {
            continue;
        }
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
        let bbox = bbox.expand(14.0);

        let color = block_color(blk);
        painter.rect_filled(bbox, 6.0, color.gamma_multiply(0.12));
        painter.rect_stroke(bbox, 6.0, egui::Stroke::new(1.5, color.gamma_multiply(0.6)));

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

            painter.circle_filled(sp, r, color);
            painter.circle_stroke(sp, r, egui::Stroke::new(1.0, color.gamma_multiply(0.7)));

            // pinned 节点：外圈黄色提示
            if state.pinned.get(&node.id).copied().unwrap_or(false) {
                painter.circle_stroke(
                    sp,
                    r + 3.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 220, 80)),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 绘制 Input/Output 节点的作用标签（顶/底排，与节点保持小间距）
// ---------------------------------------------------------------------------

fn draw_io_labels(
    genome: &Genome,
    state: &ForceGraphState,
    painter: &egui::Painter,
    to_screen: impl Fn(egui::Pos2) -> egui::Pos2,
) {
    let font = egui::FontId::proportional(10.0);
    let color_in = egui::Color32::from_rgb(120, 230, 150);
    let color_out = egui::Color32::from_rgb(255, 195, 100);
    for node in &genome.nodes {
        let (label, color, above) = match node.node_type {
            NodeType::Input => {
                let l = INPUT_LABELS.get(node.id).copied().unwrap_or("?");
                (l, color_in, true)
            }
            NodeType::Output => {
                let idx = node.id.saturating_sub(Genome::INPUT_SIZE);
                let l = OUTPUT_LABELS.get(idx).copied().unwrap_or("?");
                (l, color_out, false)
            }
            _ => continue,
        };
        let p = match state.positions.get(&node.id) {
            Some(p) => *p,
            None => continue,
        };
        let sp = to_screen(p);
        // 标签贴节点：input 在上、output 在下，留 8 px 缝避免重叠
        let (offset_y, align) = if above {
            (-9.0, egui::Align2::CENTER_BOTTOM)
        } else {
            (9.0, egui::Align2::CENTER_TOP)
        };
        painter.text(
            sp + egui::vec2(0.0, offset_y),
            align,
            format!("{}\n{}", node.id, label),
            font.clone(),
            color,
        );
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

    let threshold = 12.0_f32;

    for node in &genome.nodes {
        if let Some(p) = state.positions.get(&node.id) {
            let sp = to_screen(*p);
            let dist = (hover_pos - sp).length();
            if dist < threshold {
                painter.circle_filled(sp, node_radius(node) + 2.0, egui::Color32::WHITE);

                let mut tip = node_tooltip(node);
                if state.pinned.get(&node.id).copied().unwrap_or(false) {
                    tip.push_str("\n[已钉住，右键解锁]");
                }
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
        (egui::Color32::from_rgb(255, 220, 80), "已钉"),
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

    // 操作提示
    painter.text(
        egui::pos2(rect.right() - 6.0, y),
        egui::Align2::RIGHT_CENTER,
        "左键拖节点 / 空白拖画布 / 滚轮缩放 / 右键节点切换钉住",
        egui::FontId::proportional(9.0),
        egui::Color32::from_gray(140),
    );
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn blk_key(node: &NodeGene) -> i8 {
    // Input/Output 都归到对应投射 block，跟同区 Block 节点共用一个分组框
    node_block(node)
}

fn node_radius(node: &NodeGene) -> f32 {
    match node.node_type {
        NodeType::Input | NodeType::Output => 6.5,
        NodeType::Block(_) => 5.0,
    }
}

fn node_color(node: &NodeGene) -> egui::Color32 {
    match node.node_type {
        NodeType::Input => egui::Color32::from_rgb(100, 220, 130),
        NodeType::Output => egui::Color32::from_rgb(255, 180, 80),
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
    let brighten = |c: u8| -> u8 { c.clamp(60, 220) };
    egui::Color32::from_rgb(brighten(r), brighten(g), brighten(b))
}

fn block_label(blk: i8, count: usize) -> String {
    let role = match blk {
        -1 => "左眼",
        1 => "右眼",
        -2 => "左光耳",
        2 => "右光耳",
        -3 => "内省",
        3 => "外感",
        -25 => "繁殖",
        25 => "运动",
        -26 => "发光",
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

/// 节点 ID → 初始位置抖动（确定性，跨次运行一致）
fn hash_jitter(id: usize) -> (f32, f32) {
    let mut h = (id as u64).wrapping_mul(0x9E3779B97F4A7C15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 27;
    let x_raw = ((h & 0xFFFF) as f32) / 65535.0 - 0.5;
    h = h.wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    let y_raw = ((h & 0xFFFF) as f32) / 65535.0 - 0.5;
    (x_raw * HASH_JITTER_RANGE, y_raw * HASH_JITTER_RANGE)
}

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
