use rustc_hash::FxHashMap;

use super::genome::{Genome, NodeType};

/// 神经网络（从基因组构建）
pub struct Network {
    /// 节点值
    node_values: FxHashMap<usize, f64>,
    /// 拓扑排序后的节点顺序
    eval_order: Vec<usize>,
    /// 连接列表 (in_node, out_node, weight)
    connections: Vec<(usize, usize, f64)>,
    /// 输入节点 ID
    input_ids: Vec<usize>,
    /// 输出节点 ID
    output_ids: Vec<usize>,
}

impl Network {
    /// 从基因组构建网络
    pub fn from_genome(genome: &Genome) -> Self {
        let mut node_values = FxHashMap::default();
        let mut input_ids = Vec::new();
        let mut output_ids = Vec::new();

        // 收集节点
        for node in &genome.nodes {
            node_values.insert(node.id, 0.0);
            match node.node_type {
                NodeType::Input => input_ids.push(node.id),
                NodeType::Output => output_ids.push(node.id),
                NodeType::Hidden => {}
            }
        }

        // 收集启用的连接
        let connections: Vec<(usize, usize, f64)> = genome
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node, c.weight))
            .collect();

        // 拓扑排序
        let eval_order = Self::topological_sort(genome);

        Self {
            node_values,
            eval_order,
            connections,
            input_ids,
            output_ids,
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
            // 跳过输入节点
            if self.input_ids.contains(&node_id) {
                continue;
            }

            // 计算输入和
            let mut sum = 0.0;
            for &(in_node, out_node, weight) in &self.connections {
                if out_node == node_id {
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
