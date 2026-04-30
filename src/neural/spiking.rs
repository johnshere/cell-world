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
    /// 拓扑排序后的节点顺序
    eval_order: Vec<usize>,
    /// 输入节点 ID
    input_ids: Vec<usize>,
    /// 输入节点 ID 集合（用于 O(1) 查询）
    input_ids_set: FxHashSet<usize>,
    /// 输出节点 ID
    output_ids: Vec<usize>,
    /// 输出模式：true=直读(tanh membrane), false=脉冲
    output_modes: Vec<bool>,
    /// 正向连接：node_id -> [(from_node, weight), ...]
    forward_inputs: FxHashMap<usize, Vec<(usize, f64)>>,
    /// 回环连接：node_id -> [(from_node, weight), ...]
    /// 注：水流式语义下与 forward_inputs 行为一致（都读 prev_state），保留分桶仅为兼容
    recurrent_inputs: FxHashMap<usize, Vec<(usize, f64)>>,
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
            forward_inputs: FxHashMap::default(),
            recurrent_inputs: FxHashMap::default(),
            prev_state: FxHashMap::default(),
            eligibility_traces: FxHashMap::default(),
            learning_gene: LearningGene::default(),
        }
    }
}

impl SpikingNetwork {
    /// 从基因组构建 SNN
    pub fn from_genome(genome: &Genome) -> Self {
        let mut nodes = FxHashMap::default();
        let mut input_ids = Vec::new();
        let mut input_ids_set = FxHashSet::default();
        let mut output_ids = Vec::new();
        let mut output_modes = Vec::new();

        for node in &genome.nodes {
            let snn_node = SnnNode {
                membrane: 0.0,
                decay: node.decay,
                threshold: node.threshold,
                refractory_count: 0,
                refractory_period: node.refractory_period,
                fired: false,
            };
            nodes.insert(node.id, snn_node);

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

        // 拓扑排序
        let eval_order = Self::topological_sort(genome);

        // 构建位置映射，用于区分正向/回环连接
        let mut position: FxHashMap<usize, usize> = FxHashMap::default();
        for (pos, &node_id) in eval_order.iter().enumerate() {
            position.insert(node_id, pos);
        }

        // 分离正向连接和回环连接（水流式语义下两者行为一致，保留分桶仅为兼容）
        let mut forward_inputs: FxHashMap<usize, Vec<(usize, f64)>> = FxHashMap::default();
        let mut recurrent_inputs: FxHashMap<usize, Vec<(usize, f64)>> = FxHashMap::default();

        for conn in &genome.connections {
            if !conn.enabled {
                continue;
            }

            let in_pos = position.get(&conn.in_node).copied();
            let out_pos = position.get(&conn.out_node).copied();

            // 回环判定：源节点在拓扑序中位于目标节点之后（且不是输入节点）
            let is_recurrent = match (in_pos, out_pos) {
                (Some(ip), Some(op)) => !input_ids_set.contains(&conn.in_node) && ip >= op,
                _ => false,
            };

            if is_recurrent {
                recurrent_inputs
                    .entry(conn.out_node)
                    .or_default()
                    .push((conn.in_node, conn.weight));
            } else {
                forward_inputs
                    .entry(conn.out_node)
                    .or_default()
                    .push((conn.in_node, conn.weight));
            }
        }

        // 初始化全节点 prev_state（水流式：所有连接都从此快照读）
        let mut prev_state = FxHashMap::default();
        for node in &genome.nodes {
            prev_state.insert(node.id, (0.0, false));
        }

        Self {
            nodes,
            eval_order,
            input_ids,
            input_ids_set,
            output_ids,
            output_modes,
            forward_inputs,
            recurrent_inputs,
            prev_state,
            eligibility_traces: FxHashMap::default(),
            learning_gene: genome.learning.clone(),
        }
    }

    /// 拓扑排序
    fn topological_sort(genome: &Genome) -> Vec<usize> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();

        let mut adj: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
        for conn in &genome.connections {
            if conn.enabled {
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
    /// 水流式语义：所有连接（正向 + 回环）都从 prev_state 读取
    /// 与 GPU shader 行为对齐：每条边引入 1 tick 延迟，信号每 tick 流动一层
    fn tick_inner(&mut self) -> Vec<f64> {
        let eval_order = self.eval_order.clone();
        for &node_id in &eval_order {
            if self.input_ids_set.contains(&node_id) {
                continue;
            }

            let mut weighted_sum = 0.0;

            // 正向连接：读 prev_state（与回环统一，水流式）
            if let Some(inputs_list) = self.forward_inputs.get(&node_id) {
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

            // 回环连接：同样读 prev_state
            if let Some(recurrent_list) = self.recurrent_inputs.get(&node_id) {
                for &(in_node, weight) in recurrent_list {
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
                        // 直读模式：tanh(membrane)
                        node.membrane.tanh()
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

        // 收集所有连接键，避免对 self 的双重借用
        let pairs: Vec<(usize, usize)> = self
            .forward_inputs
            .iter()
            .flat_map(|(&out, list)| list.iter().map(move |(in_n, _)| (*in_n, out)))
            .chain(
                self.recurrent_inputs
                    .iter()
                    .flat_map(|(&out, list)| list.iter().map(move |(in_n, _)| (*in_n, out))),
            )
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

        // 收集需要更新的连接列表
        let mut updates: Vec<((usize, usize), f64)> = Vec::new();

        // 收集正向连接更新
        for (&out_node, inputs_list) in &self.forward_inputs {
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

        // 收集回环连接更新
        for (&out_node, recurrent_list) in &self.recurrent_inputs {
            for &(in_node, _) in recurrent_list {
                let key = (in_node, out_node);
                if let Some(&trace) = self.eligibility_traces.get(&key) {
                    if trace.abs() > 0.001 {
                        let delta = rate * trace * total_reward * sign;
                        updates.push((key, delta));
                    }
                }
            }
        }

        // 应用更新
        for ((in_node, out_node), delta) in updates {
            if let Some(inputs) = self.forward_inputs.get_mut(&out_node) {
                for (src, weight) in inputs.iter_mut() {
                    if *src == in_node {
                        *weight = (*weight + delta).clamp(-2.0, 2.0);
                        continue;
                    }
                }
            }
            if let Some(inputs) = self.recurrent_inputs.get_mut(&out_node) {
                for (src, weight) in inputs.iter_mut() {
                    if *src == in_node {
                        *weight = (*weight + delta).clamp(-2.0, 2.0);
                        continue;
                    }
                }
            }
        }

        // 清空资格迹（应用后）
        for trace in self.eligibility_traces.values_mut() {
            *trace *= 0.1;
        }
    }

}
