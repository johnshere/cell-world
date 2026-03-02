use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 器官基因
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct OrganGenes {
    pub nose: bool,   // 鼻子
    pub eyes: bool,   // 双眼（一个基因控制左右两只）
    pub mouth: bool,  // 嘴巴
    #[cfg_attr(feature = "persistence", serde(default = "default_organ_power"))]
    pub nose_power: f64,   // 鼻子功率 0~1
    #[cfg_attr(feature = "persistence", serde(default = "default_organ_power"))]
    pub eye_power: f64,    // 眼睛功率 0~1
    #[cfg_attr(feature = "persistence", serde(default = "default_organ_power"))]
    pub mouth_power: f64,  // 嘴巴功率 0~1
}

#[cfg(feature = "persistence")]
fn default_organ_power() -> f64 { 0.5 }

impl Default for OrganGenes {
    fn default() -> Self {
        Self {
            nose: true,
            eyes: true,
            mouth: true,
            nose_power: 0.5,
            eye_power: 0.5,
            mouth_power: 0.5,
        }
    }
}

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
    next_node_id: usize,
    pub organ_genes: OrganGenes,
}

impl Genome {
    /// 输入维度 = 14（鼻子5 + 左眼3 + 右眼3 + 自身3）
    /// 鼻子 [0..4]: 能量粒子强度, 同族强度, 异族强度, 痕迹强度, 痕迹基因相似度
    /// 左眼 [5..7]: 能量粒子接近度, 同族接近度, 异族接近度
    /// 右眼 [8..10]: 能量粒子接近度, 同族接近度, 异族接近度
    /// 自身 [11..13]: 自身能量, 体温状态, 环境温度
    pub const INPUT_SIZE: usize = 14;
    /// 输出维度（固定6个）
    /// [0] 转向角  tanh(-1~1)
    /// [1] 速度    tanh(-1~1) → abs后映射
    /// [2] 嘴      tanh(-1~1)  负=咬, 正=喂, 接触食物自动吸收
    /// [3] 繁殖    tanh(-1~1)  >0.2时触发
    /// [4] 繁殖阈值 sigmoid(0~1) → 映射到 20~200 能量
    /// [5] 子代能量比例 sigmoid(0~1) → 映射到 0.1~0.5
    pub const OUTPUT_SIZE: usize = 6;

    /// 创建最小基因组（只有输入输出，无隐藏层）
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

        // 创建4个固定输出节点
        for i in 0..Self::OUTPUT_SIZE {
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

        let next_node_id = Self::INPUT_SIZE + Self::OUTPUT_SIZE;
        Self {
            nodes,
            connections,
            next_node_id,
            organ_genes: OrganGenes::default(),
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

        // 禁用/启用连接变异
        if rng.gen::<f64>() < rate {
            if let Some(conn) = child.connections.choose_mut(&mut rng) {
                conn.enabled = !conn.enabled;
            }
        }

        // 器官变异（概率 rate * 0.1，约1.5%）
        if rng.gen::<f64>() < rate * 0.1 {
            match rng.gen_range(0..3) {
                0 => child.organ_genes.nose = !child.organ_genes.nose,
                1 => child.organ_genes.eyes = !child.organ_genes.eyes,
                _ => child.organ_genes.mouth = !child.organ_genes.mouth,
            }
        }

        // 器官功率变异（概率 rate * 0.1，与器官布尔变异同频）
        if rng.gen::<f64>() < rate * 0.1 {
            let delta = rng.gen_range(-0.1..0.1);
            match rng.gen_range(0..3) {
                0 => child.organ_genes.nose_power = (child.organ_genes.nose_power + delta).clamp(0.0, 1.0),
                1 => child.organ_genes.eye_power = (child.organ_genes.eye_power + delta).clamp(0.0, 1.0),
                _ => child.organ_genes.mouth_power = (child.organ_genes.mouth_power + delta).clamp(0.0, 1.0),
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

    /// 计算基因组哈希
    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for conn in &self.connections {
            conn.in_node.hash(&mut hasher);
            conn.out_node.hash(&mut hasher);
            ((conn.weight * 1000.0) as i64).hash(&mut hasher);
        }
        self.organ_genes.nose.hash(&mut hasher);
        self.organ_genes.eyes.hash(&mut hasher);
        self.organ_genes.mouth.hash(&mut hasher);
        ((self.organ_genes.nose_power * 100.0) as i32).hash(&mut hasher);
        ((self.organ_genes.eye_power * 100.0) as i32).hash(&mut hasher);
        ((self.organ_genes.mouth_power * 100.0) as i32).hash(&mut hasher);
        hasher.finish()
    }

    /// 计算结构哈希（只看连接拓扑，忽略权重）
    /// 用于种群聚类的快速分桶预过滤
    pub fn structural_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut keys: Vec<(usize, usize)> = self
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node))
            .collect();
        keys.sort();
        keys.hash(&mut hasher);
        hasher.finish()
    }

    /// 计算与另一个基因组的相似度（排序归并，零 HashMap 分配）
    pub fn similarity(&self, other: &Genome) -> f64 {
        // 收集并排序启用的连接
        let mut self_conns: Vec<(usize, usize, f64)> = self
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node, c.weight))
            .collect();
        self_conns.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

        let mut other_conns: Vec<(usize, usize, f64)> = other
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node, c.weight))
            .collect();
        other_conns.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

        if self_conns.is_empty() && other_conns.is_empty() {
            return 1.0;
        }

        // 归并比较
        let mut i = 0;
        let mut j = 0;
        let mut total = 0usize;
        let mut similarity_sum = 0.0;

        while i < self_conns.len() && j < other_conns.len() {
            let k1 = (self_conns[i].0, self_conns[i].1);
            let k2 = (other_conns[j].0, other_conns[j].1);
            match k1.cmp(&k2) {
                std::cmp::Ordering::Equal => {
                    similarity_sum += 1.0 - (self_conns[i].2 - other_conns[j].2).abs() / 4.0;
                    total += 1;
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => {
                    total += 1;
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    total += 1;
                    j += 1;
                }
            }
        }
        total += (self_conns.len() - i) + (other_conns.len() - j);

        let base_sim = if total == 0 { 1.0 } else { similarity_sum / total as f64 };

        // 器官差异惩罚：每个不同器官扣 0.05，功率差异额外扣分
        let mut organ_penalty = 0.0;
        if self.organ_genes.nose != other.organ_genes.nose { organ_penalty += 0.05; }
        if self.organ_genes.eyes != other.organ_genes.eyes { organ_penalty += 0.05; }
        if self.organ_genes.mouth != other.organ_genes.mouth { organ_penalty += 0.05; }
        organ_penalty += (self.organ_genes.nose_power - other.organ_genes.nose_power).abs() * 0.02;
        organ_penalty += (self.organ_genes.eye_power - other.organ_genes.eye_power).abs() * 0.02;
        organ_penalty += (self.organ_genes.mouth_power - other.organ_genes.mouth_power).abs() * 0.02;

        (base_sim - organ_penalty).max(0.0)
    }

    /// NEAT 有性繁殖：两个父代基因交叉产生子代
    pub fn crossover(parent_a: &Genome, parent_b: &Genome, a_is_fitter: bool) -> Genome {
        let (fitter, weaker) = if a_is_fitter { (parent_a, parent_b) } else { (parent_b, parent_a) };

        // 构建 weaker 的连接映射
        let weaker_conns: std::collections::HashMap<(usize, usize), &ConnectionGene> = weaker
            .connections
            .iter()
            .map(|c| ((c.in_node, c.out_node), c))
            .collect();

        // 保守交叉：共享连接取权重平均值，独有连接继承自强者
        let mut child_connections = Vec::new();
        for conn in &fitter.connections {
            let key = (conn.in_node, conn.out_node);
            if let Some(&weaker_conn) = weaker_conns.get(&key) {
                // 双方共有的连接：权重取平均，保留功能共识
                let mut blended = conn.clone();
                blended.weight = (conn.weight + weaker_conn.weight) / 2.0;
                blended.enabled = conn.enabled || weaker_conn.enabled;
                child_connections.push(blended);
            } else {
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

        let next_node_id = fitter.next_node_id.max(weaker.next_node_id);

        let mut child_organs = fitter.organ_genes.clone();
        child_organs.nose_power = (fitter.organ_genes.nose_power + weaker.organ_genes.nose_power) / 2.0;
        child_organs.eye_power = (fitter.organ_genes.eye_power + weaker.organ_genes.eye_power) / 2.0;
        child_organs.mouth_power = (fitter.organ_genes.mouth_power + weaker.organ_genes.mouth_power) / 2.0;

        Genome {
            nodes: child_nodes,
            connections: child_connections,
            next_node_id,
            organ_genes: child_organs,
        }
    }
}
