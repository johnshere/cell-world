use rustc_hash::{FxHashMap, FxHashSet};

use super::genome::{Genome, NodeType};

/// 神经网络（从基因组构建）
pub struct Network {
    /// 节点值
    node_values: FxHashMap<usize, f64>,
    /// 拓扑排序后的节点顺序
    eval_order: Vec<usize>,
    /// 输入节点 ID（用于输出收集）
    input_ids: Vec<usize>,
    /// 输入节点 ID 集合（用于 O(1) 查询）
    input_ids_set: FxHashSet<usize>,
    /// 输出节点 ID
    output_ids: Vec<usize>,
    /// 节点的输入连接：node_id -> [(from_node, weight), ...]
    node_inputs: FxHashMap<usize, Vec<(usize, f64)>>,
}

impl Network {
    /// 从基因组构建网络
    pub fn from_genome(genome: &Genome) -> Self {
        let mut node_values = FxHashMap::default();
        let mut input_ids = Vec::new();
        let mut input_ids_set = FxHashSet::default();
        let mut output_ids = Vec::new();

        // 收集节点
        for node in &genome.nodes {
            node_values.insert(node.id, 0.0);
            match node.node_type {
                NodeType::Input => {
                    input_ids.push(node.id);
                    input_ids_set.insert(node.id);
                }
                NodeType::Output => output_ids.push(node.id),
                NodeType::Block(_) => {}
            }
        }

        // 构建节点输入连接映射（邻接表）
        let mut node_inputs: FxHashMap<usize, Vec<(usize, f64)>> = FxHashMap::default();
        for conn in &genome.connections {
            if conn.enabled {
                node_inputs
                    .entry(conn.out_node)
                    .or_default()
                    .push((conn.in_node, conn.weight));
            }
        }

        // 拓扑排序
        let eval_order = Self::topological_sort(genome);

        Self {
            node_values,
            eval_order,
            input_ids,
            input_ids_set,
            output_ids,
            node_inputs,
        }
    }

    /// 拓扑排序
    fn topological_sort(genome: &Genome) -> Vec<usize> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut temp_visited = std::collections::HashSet::new();

        // 构建邻接表
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

    /// 前向传播
    pub fn forward(&mut self, inputs: &[f64]) -> Vec<f64> {
        // 重置所有节点值
        for value in self.node_values.values_mut() {
            *value = 0.0;
        }

        // 设置输入值
        for (i, &input_id) in self.input_ids.iter().enumerate() {
            if i < inputs.len() {
                self.node_values.insert(input_id, inputs[i]);
            }
        }

        // 按拓扑顺序计算
        for &node_id in &self.eval_order {
            // 跳过输入节点（O(1) 查询）
            if self.input_ids_set.contains(&node_id) {
                continue;
            }

            // 计算输入和（直接从邻接表获取，无需遍历所有连接）
            let mut sum = 0.0;
            if let Some(inputs_list) = self.node_inputs.get(&node_id) {
                for &(in_node, weight) in inputs_list {
                    let in_value = self.node_values.get(&in_node).copied().unwrap_or(0.0);
                    sum += in_value * weight;
                }
            }

            // 激活函数 (tanh)
            let activated = sum.tanh();
            self.node_values.insert(node_id, activated);
        }

        // 收集输出
        self.output_ids
            .iter()
            .map(|&id| self.node_values.get(&id).copied().unwrap_or(0.0))
            .collect()
    }
}
