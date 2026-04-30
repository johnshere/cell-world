use rustc_hash::{FxHashMap, FxHashSet};

use super::genome::{Genome, LearningGene, NodeType};

/// SNN 节点状态
#[derive(Clone)]
struct SnnNode {
    /// 膜电位（有状态，不清零）
    membrane: f64,
    /// 衰减率
    decay: f64,
    /// 发放阈值（0=直读）
    threshold: f64,
    /// 不应期剩余 ticks
    refractory_count: u8,
    /// 不应期总 ticks
    refractory_period: u8,
    /// 本 tick 是否发放
    fired: bool,
}

/// 脉冲神经网络（从基因组构建，支持回环连接）
#[derive(Clone)]
pub struct SpikingNetwork {
    nodes: FxHashMap<usize, SnnNode>,
    /// 节点遍历顺序（水流式语义下顺序无关，保留作为非输入节点的迭代顺序）
    eval_order: Vec<usize>,
    /// 输入节点 ID
    input_ids: Vec<usize>,
    /// 输入节点 ID 集合（用于 O(1) 查询）
    input_ids_set: FxHashSet<usize>,
    /// 输出节点 ID
    output_ids: Vec<usize>,
    /// 输出模式：true=直读(Padé tanh-approx of membrane，与 GPU shader 一致), false=脉冲
    output_modes: Vec<bool>,
    /// 入边表（不区分正向/回环）：out_node -> [(in_node, weight), ...]
    /// 按 genome.connections 物理顺序 push，与 GPU shader 中遍历 connections 数组的累加顺序一致
    /// 水流式语义下所有连接都从 prev_state 读，无需区分 forward/recurrent
    all_inputs: FxHashMap<usize, Vec<(usize, f64)>>,
    /// 全节点上一 tick 状态快照: node_id -> (membrane, fired)
    /// 水流式语义：所有连接都从此快照读取（与 GPU shader 行为一致）
    prev_state: FxHashMap<usize, (f64, bool)>,

    // === Learning ===
    /// 资格迹：(in_node, out_node) -> eligibility_trace
    eligibility_traces: FxHashMap<(usize, usize), f64>,
    /// 学习基因（从基因组复制，运行时只读）
    learning_gene: LearningGene,
}

impl Default for SpikingNetwork {
    fn default() -> Self {
        Self {
            nodes: FxHashMap::default(),
            eval_order: Vec::new(),
            input_ids: Vec::new(),
            input_ids_set: FxHashSet::default(),
            output_ids: Vec::new(),
            output_modes: Vec::new(),
            all_inputs: FxHashMap::default(),
            prev_state: FxHashMap::default(),
            eligibility_traces: FxHashMap::default(),
            learning_gene: LearningGene::default(),
        }
    }
}

impl SpikingNetwork {
    /// 从基因组构建 SNN
    ///
    /// ⚠️ 容量上限三处硬编码同步：
    /// - 本处：`MAX_NODES` / `MAX_CONNS`
    /// - GPU 端：`src/neural/gpu.rs` 的 `MAX_NODES` / `MAX_CONNS`
    /// - Shader：`src/neural/snn_tick.wgsl` 的 `const MAX_NODES` / `MAX_CONNS`
    /// 改任意一处必须同步改另两处，否则 CPU/GPU 行为会偏离。
    pub fn from_genome(genome: &Genome) -> Self {
        // 与 src/neural/gpu.rs 和 src/neural/snn_tick.wgsl 保持一致
        const MAX_NODES: usize = 64;
        const MAX_CONNS: usize = 128;

        let mut nodes = FxHashMap::default();
        let mut input_ids = Vec::new();
        let mut input_ids_set = FxHashSet::default();
        let mut output_ids = Vec::new();
        let mut output_modes = Vec::new();
        // 截断后实际生效的节点 id 集合，用于过滤连接端点（与 GPU 的 id_to_local 等价）
        let mut effective_node_ids: FxHashSet<usize> = FxHashSet::default();

        // 截断到前 MAX_NODES 个节点（与 GPU `upload_genome` 行为一致）
        for (local_idx, node) in genome.nodes.iter().enumerate() {
            if local_idx >= MAX_NODES {
                break;
            }
            let snn_node = SnnNode {
                membrane: 0.0,
                decay: node.decay,
                threshold: node.threshold,
                refractory_count: 0,
                refractory_period: node.refractory_period,
                fired: false,
            };
            nodes.insert(node.id, snn_node);
            effective_node_ids.insert(node.id);

            match node.node_type {
                NodeType::Input => {
                    input_ids.push(node.id);
                    input_ids_set.insert(node.id);
                }
                NodeType::Output => {
                    output_ids.push(node.id);
                    // threshold == 0 → 直读模式
                    output_modes.push(node.threshold == 0.0);
                }
                NodeType::Block(_) => {}
            }
        }

        // 节点遍历顺序（水流式语义下顺序无关，保留拓扑排序作为非输入节点的迭代序）
        let eval_order = Self::topological_sort(genome, &effective_node_ids);

        // 构建入边表（不区分正向/回环）：按 genome.connections 物理顺序 push，
        // 与 GPU shader 中 `for c in 0..conn_count { connections[c] }` 累加顺序一致
        let mut all_inputs: FxHashMap<usize, Vec<(usize, f64)>> = FxHashMap::default();
        let mut conn_count = 0usize;

        for conn in &genome.connections {
            if !conn.enabled {
                continue;
            }
            // 已收满 MAX_CONNS 直接停止后续遍历（与 GPU upload_genome 行为一致）
            if conn_count >= MAX_CONNS {
                break;
            }
            // 端点必须都在截断后的节点集合中（与 GPU 的 id_to_local 检查等价）
            if !effective_node_ids.contains(&conn.in_node)
                || !effective_node_ids.contains(&conn.out_node)
            {
                continue;
            }
            conn_count += 1;

            all_inputs
                .entry(conn.out_node)
                .or_default()
                .push((conn.in_node, conn.weight));
        }

        // 初始化截断后节点的 prev_state（水流式：所有连接都从此快照读）
        let mut prev_state = FxHashMap::default();
        for &node_id in &effective_node_ids {
            prev_state.insert(node_id, (0.0, false));
        }

        Self {
            nodes,
            eval_order,
            input_ids,
            input_ids_set,
            output_ids,
            output_modes,
            all_inputs,
            prev_state,
            eligibility_traces: FxHashMap::default(),
            learning_gene: genome.learning.clone(),
        }
    }

    /// 拓扑排序（只对截断后保留的节点和连接做排序，与 GPU 截断行为对齐）
    fn topological_sort(genome: &Genome, effective: &FxHashSet<usize>) -> Vec<usize> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();

        let mut adj: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        for conn in &genome.connections {
            if conn.enabled
                && effective.contains(&conn.in_node)
                && effective.contains(&conn.out_node)
            {
                adj.entry(conn.in_node).or_default().push(conn.out_node);
            }
        }

        fn visit(
            node: usize,
            adj: &FxHashMap<usize, Vec<usize>>,
            visited: &mut std::collections::HashSet<usize>,
            temp_visited: &mut std::collections::HashSet<usize>,
            result: &mut Vec<usize>,
        ) {
            if visited.contains(&node) {
                return;
            }
            if temp_visited.contains(&node) {
                return; // 回环检测：跳过回环边
            }
            temp_visited.insert(node);

            if let Some(neighbors) = adj.get(&node) {
                for &next in neighbors {
                    visit(next, adj, visited, temp_visited, result);
                }
            }

            temp_visited.remove(&node);
            visited.insert(node);
            result.push(node);
        }

        for node in &genome.nodes {
            if !effective.contains(&node.id) {
                continue;
            }
            visit(node.id, &adj, &mut visited, &mut temp_visited, &mut result);
        }

        result.reverse();
        result
    }

    /// 输出模式（直读/脉冲）
    pub fn output_modes(&self) -> &[bool] {
        &self.output_modes
    }

    /// 执行一个 tick（注入输入）
    pub fn tick(&mut self, inputs: &[f64]) -> Vec<f64> {
        // 先注入输入（覆盖输入节点状态）
        for (i, &input_id) in self.input_ids.iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&input_id) {
                node.membrane = if i < inputs.len() { inputs[i] } else { 0.0 };
                node.fired = true;
            }
        }
        // 再拍全节点快照：含新输入 + 非输入节点的上一 tick 末状态
        // 与 GPU shader 行为对齐：输入立即可见，非输入节点信号每 tick 流动一层
        self.save_state_snapshot();

        let outputs = self.tick_inner();
        self.update_eligibility_traces();
        outputs
    }

    /// 执行一个 tick（不注入新输入，输入节点保持上次状态）
    /// 感知在帧内不变，保持输入信号持续激励脉冲神经元
    pub fn tick_free(&mut self) -> Vec<f64> {
        self.save_state_snapshot();
        // 不修改输入节点，保持上一次 tick() 注入的 membrane 和 fired 状态
        let outputs = self.tick_inner();
        self.update_eligibility_traces();
        outputs
    }

    /// 执行多 tick：首次注入输入，后续 tick_free
    /// 水流式语义：直读输出取末次 tick 值（信号传播 N 层后的稳定状态），脉冲输出取发放率
    pub fn tick_multi(&mut self, inputs: &[f64], ticks: usize) -> Vec<f64> {
        let n = self.output_ids.len().min(8);
        let mut spike_counts = [0u32; 8];

        // 第 1 tick: 注入输入
        let mut last_outputs = self.tick(inputs);
        for (j, (&v, &direct_read)) in last_outputs
            .iter()
            .zip(self.output_modes.iter())
            .enumerate()
            .take(n)
        {
            if !direct_read && v > 0.5 {
                spike_counts[j] += 1;
            }
        }

        // 后续 ticks: tick_free 保持输入信号，让水流继续往下游传播
        for _ in 1..ticks {
            last_outputs = self.tick_free();
            for (j, (&v, &direct_read)) in last_outputs
                .iter()
                .zip(self.output_modes.iter())
                .enumerate()
                .take(n)
            {
                if !direct_read && v > 0.5 {
                    spike_counts[j] += 1;
                }
            }
        }

        // 组合最终输出：直读取末次值（last_outputs 已是 tick N-1 的结果），脉冲取发放率
        for (j, &direct_read) in self.output_modes.iter().enumerate().take(n) {
            if !direct_read && j < last_outputs.len() {
                let rate = spike_counts[j] as f64 / ticks.max(1) as f64;
                last_outputs[j] = rate * 2.0 - 1.0;
            }
        }

        last_outputs
    }

    /// 保存全节点的当前状态快照（水流式语义：下一 tick 所有连接都从此快照读）
    fn save_state_snapshot(&mut self) {
        for (&id, state) in self.prev_state.iter_mut() {
            if let Some(node) = self.nodes.get(&id) {
                *state = (node.membrane, node.fired);
            }
        }
    }

    /// 内部 tick 逻辑（评估非输入节点）
    /// 水流式语义：所有连接都从 prev_state 读取，按 genome.connections 物理顺序累加
    /// 与 GPU shader 累加顺序对齐：每条边引入 1 tick 延迟，信号每 tick 流动一层
    fn tick_inner(&mut self) -> Vec<f64> {
        let eval_order = self.eval_order.clone();
        for &node_id in &eval_order {
            if self.input_ids_set.contains(&node_id) {
                continue;
            }

            let mut weighted_sum = 0.0;

            // 累加所有入边（不区分正向/回环，按 all_inputs 中存储顺序，
            // 即 genome.connections 物理顺序，与 GPU shader 一致）
            if let Some(inputs_list) = self.all_inputs.get(&node_id) {
                for &(in_node, weight) in inputs_list {
                    if let Some(&(prev_membrane, prev_fired)) = self.prev_state.get(&in_node) {
                        if prev_fired {
                            let threshold =
                                self.nodes.get(&in_node).map(|n| n.threshold).unwrap_or(0.0);
                            if threshold == 0.0 {
                                weighted_sum += prev_membrane * weight;
                            } else {
                                weighted_sum += weight;
                            }
                        }
                    }
                }
            }

            if let Some(node) = self.nodes.get_mut(&node_id) {
                // 不应期中
                if node.refractory_count > 0 {
                    node.refractory_count -= 1;
                    node.fired = false;
                    continue;
                }

                // 膜电位衰减 + 累加输入
                node.membrane *= node.decay;
                node.membrane += weighted_sum;

                // 阈值判定
                if node.threshold == 0.0 {
                    // 直读模式：不发放脉冲，membrane 保持为累积值
                    node.fired = true; // 标记为"活跃"以传播信号
                } else if node.membrane.abs() >= node.threshold {
                    // 超阈发放
                    node.fired = true;
                    node.membrane = 0.0; // 重置膜电位
                    node.refractory_count = node.refractory_period;
                } else {
                    node.fired = false;
                }
            }
        }

        // 收集输出
        self.output_ids
            .iter()
            .zip(self.output_modes.iter())
            .map(|(&id, &direct_read)| {
                if let Some(node) = self.nodes.get(&id) {
                    if direct_read {
                        // 直读模式：Padé 近似 of tanh，与 GPU shader 完全一致
                        // 公式：clamp(x*(27+x²)/(27+9x²), -1, 1)
                        // 见 src/neural/snn_tick.wgsl 中相同表达式
                        // WGSL 没有内置 tanh，GPU 用此 Padé 近似；CPU 对齐以保证两端数值一致
                        let x = node.membrane;
                        let x2 = x * x;
                        (x * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0)
                    } else {
                        // 脉冲模式：fired → 1.0, 否则 → 0.0
                        if node.fired {
                            1.0
                        } else {
                            0.0
                        }
                    }
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// 更新资格迹（每 tick 结束时调用）
    /// 水流式语义：所有连接的 pre/post fired 都从 prev_state 读，与 GPU shader 一致
    fn update_eligibility_traces(&mut self) {
        let decay = self.learning_gene.eligibility_decay;

        // 收集所有连接键（避免对 self 的双重借用）
        let pairs: Vec<(usize, usize)> = self
            .all_inputs
            .iter()
            .flat_map(|(&out, list)| list.iter().map(move |(in_n, _)| (*in_n, out)))
            .collect();

        for (in_node, out_node) in pairs {
            let pre_fired = self
                .prev_state
                .get(&in_node)
                .map(|(_, f)| *f)
                .unwrap_or(false);
            let post_fired = self
                .prev_state
                .get(&out_node)
                .map(|(_, f)| *f)
                .unwrap_or(false);

            let key = (in_node, out_node);
            let trace = self.eligibility_traces.entry(key).or_insert(0.0);

            // 资格迹衰减
            *trace *= decay;

            // pre 和 post 在上一 tick 共激活则累积
            if pre_fired && post_fired {
                *trace += 1.0;
            }
        }
    }

    /// 应用生理信号到权重（多通道独立计算后叠加）
    /// 调用方将各通道 × 敏感度后求和，传入最终 total_reward
    pub fn apply_physiology(&mut self, total_reward: f64) {
        if self.learning_gene.learning_on < 0.5 {
            return; // 学习禁用
        }
        if total_reward.abs() < 0.001 {
            return;
        }

        let sign = (self.learning_gene.hebbian_sign - 0.5) * 2.0; // -1 ~ 1
        let rate = self.learning_gene.hebbian_rate;

        // 收集需要更新的连接列表（不区分正向/回环）
        let mut updates: Vec<((usize, usize), f64)> = Vec::new();

        for (&out_node, inputs_list) in &self.all_inputs {
            for &(in_node, _) in inputs_list {
                let key = (in_node, out_node);
                if let Some(&trace) = self.eligibility_traces.get(&key) {
                    if trace.abs() > 0.001 {
                        let delta = rate * trace * total_reward * sign;
                        updates.push((key, delta));
                    }
                }
            }
        }

        // 应用权重更新
        for ((in_node, out_node), delta) in updates {
            if let Some(inputs) = self.all_inputs.get_mut(&out_node) {
                for (src, weight) in inputs.iter_mut() {
                    if *src == in_node {
                        *weight = (*weight + delta).clamp(-2.0, 2.0);
                        continue;
                    }
                }
            }
        }

        // 衰减资格迹（仅对 |trace| >= 0.001 的衰减，与 GPU apply_rewards 行为一致）
        for trace in self.eligibility_traces.values_mut() {
            if trace.abs() >= 0.001 {
                *trace *= 0.1;
            }
        }
    }
}
