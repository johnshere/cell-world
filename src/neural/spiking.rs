use rustc_hash::{FxHashMap, FxHashSet};

use super::genome::{Genome, NodeType};

/// SNN 节点状态
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

/// 脉冲神经网络（从基因组构建）
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
    /// 输出模式：true=直读(tanh membrane), false=脉冲(fired?1:-1)
    output_modes: Vec<bool>,
    /// 节点的输入连接：node_id -> [(from_node, weight), ...]
    node_inputs: FxHashMap<usize, Vec<(usize, f64)>>,
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
                NodeType::Hidden => {}
            }
        }

        // 构建邻接表
        let mut node_inputs: FxHashMap<usize, Vec<(usize, f64)>> = FxHashMap::default();
        for conn in &genome.connections {
            if conn.enabled {
                node_inputs
                    .entry(conn.out_node)
                    .or_default()
                    .push((conn.in_node, conn.weight));
            }
        }

        let eval_order = Self::topological_sort(genome);

        Self {
            nodes,
            eval_order,
            input_ids,
            input_ids_set,
            output_ids,
            output_modes,
            node_inputs,
        }
    }

    /// 拓扑排序（复用 network.rs 的逻辑）
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
                return; // 循环检测
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

    /// 执行一个 tick
    ///
    /// 1. 输入节点：membrane = input value, fired = true
    /// 2. 非输入按拓扑序：不应期跳过 → membrane *= decay → 累加 fired 前驱的 weight → 超阈发放/直读
    /// 3. 收集输出
    pub fn tick(&mut self, inputs: &[f64]) -> Vec<f64> {
        // 1. 设置输入节点
        for (i, &input_id) in self.input_ids.iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&input_id) {
                node.membrane = if i < inputs.len() { inputs[i] } else { 0.0 };
                node.fired = true;
            }
        }

        // 2. 按拓扑序计算非输入节点
        // 需要两步拆分以避免同时借用
        let eval_order = self.eval_order.clone();
        for &node_id in &eval_order {
            if self.input_ids_set.contains(&node_id) {
                continue;
            }

            // 收集前驱信号
            // 直读节点(threshold=0): membrane * weight（保留信号幅度，类似传统ANN）
            // 脉冲节点(threshold>0): weight（二值脉冲）
            let weighted_sum = if let Some(inputs_list) = self.node_inputs.get(&node_id) {
                let mut sum = 0.0;
                for &(in_node, weight) in inputs_list {
                    if let Some(src) = self.nodes.get(&in_node) {
                        if src.fired {
                            if src.threshold == 0.0 {
                                sum += src.membrane * weight;
                            } else {
                                sum += weight;
                            }
                        }
                    }
                }
                sum
            } else {
                0.0
            };

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

        // 3. 收集输出
        self.output_ids
            .iter()
            .zip(self.output_modes.iter())
            .map(|(&id, &direct_read)| {
                if let Some(node) = self.nodes.get(&id) {
                    if direct_read {
                        // 直读模式：tanh(membrane)
                        node.membrane.tanh()
                    } else {
                        // 脉冲模式：fired → 1.0, 否则 → -1.0
                        if node.fired {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// 重置所有节点状态
    pub fn reset(&mut self) {
        for node in self.nodes.values_mut() {
            node.membrane = 0.0;
            node.fired = false;
            node.refractory_count = 0;
        }
    }
}
