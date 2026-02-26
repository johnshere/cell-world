use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 节点类型
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub enum NodeType {
    Input,
    Hidden,
    Output,
}

/// 节点基因
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct NodeGene {
    pub id: usize,
    pub node_type: NodeType,
}

/// 连接基因
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct ConnectionGene {
    pub in_node: usize,
    pub out_node: usize,
    pub weight: f64,
    pub enabled: bool,
}

/// 基因组
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct Genome {
    pub nodes: Vec<NodeGene>,
    pub connections: Vec<ConnectionGene>,
    pub output_map: Vec<usize>, // 输出节点 -> 功能池映射
    next_node_id: usize,
}

impl Genome {
    /// 输入维度（sin/cos角度编码，无跳变，追逐权重≈+1，逃离权重≈-1）
    /// [0-4]   最佳同类: sin(θ), cos(θ), 距离(0~1), 相似度(0~1), 能量(0~1)
    /// [5-9]   最差同类: sin(θ), cos(θ), 距离(0~1), 相似度(0~1), 能量(0~1)
    /// [10-14] 最佳异类: sin(θ), cos(θ), 距离(0~1), 相似度(0~1), 能量(0~1)
    /// [15-19] 最差异类: sin(θ), cos(θ), 距离(0~1), 相似度(0~1), 能量(0~1)
    /// [20-23] 最佳能量粒子: sin(θ), cos(θ), 距离(0~1), 能量(0~1)
    /// [24]    自身能量(0~1)
    pub const INPUT_SIZE: usize = 25;
    /// 功能池大小
    /// [0] 移动方向sin   tanh(-1~1)
    /// [1] 移动方向cos   tanh(-1~1)
    /// [2] 移动速度      0~1
    /// [3] 吸收
    /// [4] 释放
    /// [5] 繁殖
    /// [6] 捕食
    /// [7] 扫描半径      0~1 → 50~200
    /// [8] 扫描角速度    0~1 → 0~max°/s
    /// [9] 哺育
    pub const FUNCTION_POOL_SIZE: usize = 10;

    /// 创建最小基因组（只有输入输出，无隐藏层）
    /// 初始功能：方向sin(0)、方向cos(1)、速度(2)、吸收(3)、繁殖(5)、扫描半径(7)、扫描角速度(8)
    /// 所有连接完全随机，让行为通过进化自然涌现
    pub fn random_minimal(min_connections: usize, max_connections: usize) -> Self {
        let mut rng = rand::thread_rng();
        let mut nodes = Vec::new();
        let mut connections = Vec::new();

        // 创建输入节点
        for i in 0..Self::INPUT_SIZE {
            nodes.push(NodeGene {
                id: i,
                node_type: NodeType::Input,
            });
        }

        // 核心功能：方向sin/cos(0,1)、速度(2)、吸收(3)、繁殖(5)、扫描控制(7,8)
        let output_map = vec![0, 1, 2, 3, 5, 7, 8];

        // 为每个输出创建节点和随机连接
        for (i, &_func_id) in output_map.iter().enumerate() {
            let output_id = Self::INPUT_SIZE + i;
            nodes.push(NodeGene {
                id: output_id,
                node_type: NodeType::Output,
            });

            // 随机连接一些输入到这个输出
            let connect_count = rng.gen_range(min_connections..=max_connections);
            for _ in 0..connect_count {
                let in_node = rng.gen_range(0..Self::INPUT_SIZE);
                connections.push(ConnectionGene {
                    in_node,
                    out_node: output_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        let next_node_id = Self::INPUT_SIZE + output_map.len();
        Self {
            nodes,
            connections,
            output_map,
            next_node_id,
        }
    }

    /// 变异（所有变异逻辑使用同一个概率）
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
        if rng.gen::<f64>() < rate {
            child.mutate_add_connection();
        }

        // 添加节点变异
        if rng.gen::<f64>() < rate {
            child.mutate_add_node();
        }

        // 添加输出变异（解锁新功能）
        if rng.gen::<f64>() < rate {
            child.mutate_add_output();
        }

        // 禁用/启用连接变异
        if rng.gen::<f64>() < rate {
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
        });

        self.connections.push(ConnectionGene {
            in_node: new_node_id,
            out_node: old_conn.out_node,
            weight: old_conn.weight,
            enabled: true,
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
    /// 要求相同基因类型（连接拓扑）且对应权重近似才算同类
    pub fn similarity(&self, other: &Genome) -> f64 {
        // 构建连接映射: (in_node, out_node) -> weight
        let self_conns: std::collections::HashMap<(usize, usize), f64> = self
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| ((c.in_node, c.out_node), c.weight))
            .collect();

        let other_conns: std::collections::HashMap<(usize, usize), f64> = other
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| ((c.in_node, c.out_node), c.weight))
            .collect();

        // 收集所有唯一连接键
        let mut all_keys: std::collections::HashSet<(usize, usize)> =
            self_conns.keys().cloned().collect();
        for key in other_conns.keys() {
            all_keys.insert(*key);
        }

        if all_keys.is_empty() {
            return 1.0;
        }

        // 计算相似度：拓扑匹配 + 权重近似
        let mut similarity_sum = 0.0;
        for key in &all_keys {
            if let (Some(&w1), Some(&w2)) = (self_conns.get(key), other_conns.get(key)) {
                // 双方都有此连接：计算权重相似度
                // 权重范围 [-2, 2]，最大差值 4.0
                similarity_sum += 1.0 - (w1 - w2).abs() / 4.0;
            }
            // 仅一方有此连接：贡献 0（拓扑不匹配）
        }

        similarity_sum / all_keys.len() as f64
    }

    /// NEAT 有性繁殖：两个父代基因交叉产生子代
    /// - 匹配连接（相同 in_node, out_node）：随机从一方继承
    /// - 不匹配连接：从适应度高的一方（fitter）继承
    /// - 节点：取两方并集
    /// - 输出映射：从 fitter 继承
    pub fn crossover(parent_a: &Genome, parent_b: &Genome, a_is_fitter: bool) -> Genome {
        let mut rng = rand::thread_rng();
        let (fitter, weaker) = if a_is_fitter { (parent_a, parent_b) } else { (parent_b, parent_a) };

        // 构建 weaker 的连接映射
        let weaker_conns: std::collections::HashMap<(usize, usize), &ConnectionGene> = weaker
            .connections
            .iter()
            .map(|c| ((c.in_node, c.out_node), c))
            .collect();

        // 交叉连接
        let mut child_connections = Vec::new();
        for conn in &fitter.connections {
            let key = (conn.in_node, conn.out_node);
            if let Some(&weaker_conn) = weaker_conns.get(&key) {
                // 匹配连接：随机从一方继承
                if rng.gen_bool(0.5) {
                    child_connections.push(conn.clone());
                } else {
                    child_connections.push(weaker_conn.clone());
                }
            } else {
                // 不匹配连接：从 fitter 继承
                child_connections.push(conn.clone());
            }
        }

        // 节点：取两方并集
        let mut node_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut child_nodes = Vec::new();
        for node in &fitter.nodes {
            node_ids.insert(node.id);
            child_nodes.push(node.clone());
        }
        for node in &weaker.nodes {
            if !node_ids.contains(&node.id) {
                node_ids.insert(node.id);
                child_nodes.push(node.clone());
            }
        }

        // 输出映射：从 fitter 继承
        let child_output_map = fitter.output_map.clone();

        // next_node_id：取两方最大值
        let next_node_id = fitter.next_node_id.max(weaker.next_node_id);

        Genome {
            nodes: child_nodes,
            connections: child_connections,
            output_map: child_output_map,
            next_node_id,
        }
    }
}
