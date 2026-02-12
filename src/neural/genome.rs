use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 节点类型
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NodeType {
    Input,
    Hidden,
    Output,
}

/// 节点基因
#[derive(Clone, Debug)]
pub struct NodeGene {
    pub id: usize,
    pub node_type: NodeType,
}

/// 连接基因
#[derive(Clone, Debug)]
pub struct ConnectionGene {
    pub in_node: usize,
    pub out_node: usize,
    pub weight: f64,
    pub enabled: bool,
    pub innovation: usize,
}

/// 基因组
#[derive(Clone, Debug)]
pub struct Genome {
    pub nodes: Vec<NodeGene>,
    pub connections: Vec<ConnectionGene>,
    pub output_map: Vec<usize>, // 输出节点 -> 功能池映射
    next_node_id: usize,
}

impl Genome {
    /// 输入维度
    pub const INPUT_SIZE: usize = 25;
    /// 功能池大小
    pub const FUNCTION_POOL_SIZE: usize = 6;

    /// 创建最小基因组（只有输入输出，无隐藏层）
    pub fn random_minimal() -> Self {
        let mut rng = rand::thread_rng();
        let mut nodes = Vec::new();
        let mut connections = Vec::new();

        // 创建输入节点 (0-24)
        for i in 0..Self::INPUT_SIZE {
            nodes.push(NodeGene {
                id: i,
                node_type: NodeType::Input,
            });
        }

        // 随机选择初始输出数量 (2-4个)
        let initial_outputs = rng.gen_range(2..=4);
        let mut output_map = Vec::new();

        // 从功能池中随机选择功能
        let mut available_functions: Vec<usize> = (0..Self::FUNCTION_POOL_SIZE).collect();
        for i in 0..initial_outputs {
            let func_idx = rng.gen_range(0..available_functions.len());
            let func_id = available_functions.remove(func_idx);
            output_map.push(func_id);

            let output_id = Self::INPUT_SIZE + i;
            nodes.push(NodeGene {
                id: output_id,
                node_type: NodeType::Output,
            });

            // 随机连接一些输入到这个输出
            let connect_count = rng.gen_range(2..=5);
            for _ in 0..connect_count {
                let in_node = rng.gen_range(0..Self::INPUT_SIZE);
                connections.push(ConnectionGene {
                    in_node,
                    out_node: output_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                    innovation: connections.len(),
                });
            }
        }

        Self {
            nodes,
            connections,
            output_map,
            next_node_id: Self::INPUT_SIZE + initial_outputs,
        }
    }

    /// 变异
    pub fn mutate(&self, rate: f64) -> Self {
        let mut rng = rand::thread_rng();
        let mut child = self.clone();

        // 权重变异
        for conn in &mut child.connections {
            if rng.gen::<f64>() < rate {
                if rng.gen::<f64>() < 0.9 {
                    // 微调
                    conn.weight += rng.gen_range(-0.5..0.5);
                    conn.weight = conn.weight.clamp(-2.0, 2.0);
                } else {
                    // 重置
                    conn.weight = rng.gen_range(-1.0..1.0);
                }
            }
        }

        // 添加连接变异
        if rng.gen::<f64>() < rate * 0.3 {
            child.mutate_add_connection();
        }

        // 添加节点变异
        if rng.gen::<f64>() < rate * 0.1 {
            child.mutate_add_node();
        }

        // 添加输出变异（解锁新功能）
        if rng.gen::<f64>() < rate * 0.05 {
            child.mutate_add_output();
        }

        // 禁用/启用连接变异
        if rng.gen::<f64>() < rate * 0.1 {
            if let Some(conn) = child.connections.choose_mut(&mut rng) {
                conn.enabled = !conn.enabled;
            }
        }

        child
    }

    /// 添加连接变异
    fn mutate_add_connection(&mut self) {
        let mut rng = rand::thread_rng();

        // 收集有效的输入节点（输入和隐藏）
        let in_candidates: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.node_type != NodeType::Output)
            .map(|n| n.id)
            .collect();

        // 收集有效的输出节点（隐藏和输出）
        let out_candidates: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.node_type != NodeType::Input)
            .map(|n| n.id)
            .collect();

        if in_candidates.is_empty() || out_candidates.is_empty() {
            return;
        }

        // 尝试找到一个不存在的连接
        for _ in 0..10 {
            let in_node = in_candidates[rng.gen_range(0..in_candidates.len())];
            let out_node = out_candidates[rng.gen_range(0..out_candidates.len())];

            // 检查连接是否已存在
            let exists = self
                .connections
                .iter()
                .any(|c| c.in_node == in_node && c.out_node == out_node);

            if !exists && in_node != out_node {
                self.connections.push(ConnectionGene {
                    in_node,
                    out_node,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                    innovation: self.connections.len(),
                });
                break;
            }
        }
    }

    /// 添加节点变异
    fn mutate_add_node(&mut self) {
        let mut rng = rand::thread_rng();

        // 选择一个启用的连接
        let enabled_conns: Vec<usize> = self
            .connections
            .iter()
            .enumerate()
            .filter(|(_, c)| c.enabled)
            .map(|(i, _)| i)
            .collect();

        if enabled_conns.is_empty() {
            return;
        }

        let conn_idx = enabled_conns[rng.gen_range(0..enabled_conns.len())];
        let old_conn = self.connections[conn_idx].clone();

        // 禁用旧连接
        self.connections[conn_idx].enabled = false;

        // 创建新的隐藏节点
        let new_node_id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Hidden,
        });

        // 创建两个新连接
        self.connections.push(ConnectionGene {
            in_node: old_conn.in_node,
            out_node: new_node_id,
            weight: 1.0, // 保持原信号
            enabled: true,
            innovation: self.connections.len(),
        });

        self.connections.push(ConnectionGene {
            in_node: new_node_id,
            out_node: old_conn.out_node,
            weight: old_conn.weight,
            enabled: true,
            innovation: self.connections.len(),
        });
    }

    /// 添加输出变异（解锁新功能）
    fn mutate_add_output(&mut self) {
        let mut rng = rand::thread_rng();

        // 检查是否还有未解锁的功能
        let used_functions: Vec<usize> = self.output_map.clone();
        let available: Vec<usize> = (0..Self::FUNCTION_POOL_SIZE)
            .filter(|f| !used_functions.contains(f))
            .collect();

        if available.is_empty() {
            return;
        }

        // 随机选择一个新功能
        let new_func = available[rng.gen_range(0..available.len())];
        self.output_map.push(new_func);

        // 创建新的输出节点
        let new_node_id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Output,
        });

        // 随机连接一些输入到新输出
        let connect_count = rng.gen_range(2..=5);
        for _ in 0..connect_count {
            let in_node = rng.gen_range(0..Self::INPUT_SIZE);
            self.connections.push(ConnectionGene {
                in_node,
                out_node: new_node_id,
                weight: rng.gen_range(-1.0..1.0),
                enabled: true,
                innovation: self.connections.len(),
            });
        }
    }

    /// 计算基因组哈希
    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for conn in &self.connections {
            conn.in_node.hash(&mut hasher);
            conn.out_node.hash(&mut hasher);
            ((conn.weight * 1000.0) as i64).hash(&mut hasher);
        }
        for func in &self.output_map {
            func.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// 计算与另一个基因组的相似度
    pub fn similarity(&self, other: &Genome) -> f64 {
        // 简单的 Jaccard 相似度
        let self_conns: std::collections::HashSet<(usize, usize)> = self
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node))
            .collect();

        let other_conns: std::collections::HashSet<(usize, usize)> = other
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node))
            .collect();

        let intersection = self_conns.intersection(&other_conns).count();
        let union = self_conns.union(&other_conns).count();

        if union == 0 {
            1.0
        } else {
            intersection as f64 / union as f64
        }
    }

    /// 获取输出节点数量
    pub fn output_count(&self) -> usize {
        self.output_map.len()
    }
}
