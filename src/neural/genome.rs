use crate::config::{self, Config};
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
/// 分区分层索引（输入，输出，其他中间层）
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub enum TierType {
    Input,
    Output,
    Other(u8),
}

fn default_decay() -> f64 {
    0.0
}
fn default_threshold() -> f64 {
    0.0
}
fn default_partition() -> TierType {
    TierType::Input
}
fn default_layer() -> TierType {
    TierType::Input
}

/// 节点基因
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct NodeGene {
    pub id: usize,
    pub node_type: NodeType,
    /// 分区，0-max，0-输入区，1-输出区，其他中间区（用于引导分区结构进化）
    #[cfg_attr(feature = "persistence", serde(default = "default_partition"))]
    pub partition: TierType,
    /// 分层，0-max，输入层0，1-输出层，其他中间层（用于引导分层结构进化）
    #[cfg_attr(feature = "persistence", serde(default = "default_layer"))]
    pub layer: TierType,
    /// 连接偏好分区，默认是本区，变异时有一定概率改变
    #[cfg_attr(feature = "persistence", serde(default = "default_partition"))]
    pub preferred_partition: TierType,
    /// 连接偏好分层，默认是本层，变异时有一定概率改变
    #[cfg_attr(feature = "persistence", serde(default = "default_layer"))]
    pub preferred_layer: TierType,
    /// 膜电位衰减 (0.0~0.99)
    #[cfg_attr(feature = "persistence", serde(default = "default_decay"))]
    pub decay: f64,
    /// 发放阈值 (0.0~2.0)，0=直读
    #[cfg_attr(feature = "persistence", serde(default = "default_threshold"))]
    pub threshold: f64,
    /// 不应期 ticks
    #[cfg_attr(feature = "persistence", serde(default))]
    pub refractory_period: u8,
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
    /// 预排序的启用连接缓存（用于快速 similarity 比较，避免每次重复排序+分配）
    #[cfg_attr(feature = "persistence", serde(skip))]
    sorted_conns_cache: Vec<(usize, usize, i64)>,
}

impl Genome {
    /// 输入维度 = 17
    /// 左眼 [0..7]: 扫描角归一化, 目标接近度, 目标能量, 实体类型(0/0.33/0.67/1.0), 基因相似度(生物)/同族(痕迹), 能量密度, 朝向差, 速度差
    /// 右眼 [8..15]: 扫描角归一化, 目标接近度, 目标能量, 实体类型, 基因相似度/同族, 能量密度, 朝向差, 速度差
    /// 自身 [16]: 能量(/2000)
    pub const INPUT_SIZE: usize = 17;
    /// 输出维度（固定7个）
    /// [0] 转向角  tanh(-1~1)
    /// [1] 速度    tanh(-1~1) → abs后映射
    /// [2] 嘴      tanh(-1~1)  负=咬, 接触食物自动吸收
    /// [3] 繁殖    tanh(-1~1)  >0.2时触发
    /// [4] 繁殖阈值 sigmoid(0~1) → 映射到 20~200 能量
    /// [5] 子代能量比例 sigmoid(0~1) → 映射到 0.1~0.5
    /// [6] 痕迹强度 正半轴(0~1) → 映射到 0~0.3 (叠加于移动消耗的额外能量投放)
    pub const OUTPUT_SIZE: usize = 7;

    /// 创建最小基因组（只有输入输出，无隐藏层）
    pub fn random_minimal(min_connections: usize, max_connections: usize) -> Self {
        let mut rng = rand::thread_rng();
        let mut nodes = Vec::new();
        let mut connections = Vec::new();

        // 创建输入节点（直通：decay=0, threshold=0, refractory=0）
        for i in 0..Self::INPUT_SIZE {
            nodes.push(NodeGene {
                id: i,
                node_type: NodeType::Input,
                partition: TierType::Input,
                layer: TierType::Input,
                preferred_partition: TierType::Output, // 初始设定连接输出区
                preferred_layer: TierType::Output,
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
        }

        // 创建输出节点（全部直读模式，冷却由 execute_actions 控制）
        for i in 0..Self::OUTPUT_SIZE {
            let output_id = Self::INPUT_SIZE + i;
            let (decay, threshold, refractory) = (0.0, 0.0, 0);
            nodes.push(NodeGene {
                id: output_id,
                node_type: NodeType::Output,
                partition: TierType::Output, // 输出区
                layer: TierType::Output,
                preferred_partition: TierType::Input, // 仅占位
                preferred_layer: TierType::Input,     // 仅占位
                decay,
                threshold,
                refractory_period: refractory,
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
        let mut genome = Self {
            nodes,
            connections,
            next_node_id,
            sorted_conns_cache: Vec::new(),
        };
        genome.rebuild_sorted_cache();
        genome
    }

    /// 确保排序缓存已初始化（反序列化后缓存为空，需要重建）
    pub fn ensure_sorted_cache(&mut self) {
        if self.sorted_conns_cache.is_empty() && !self.connections.is_empty() {
            self.rebuild_sorted_cache();
        }
    }

    /// 重建预排序连接缓存（创建/变异/交叉后调用）
    fn rebuild_sorted_cache(&mut self) {
        self.sorted_conns_cache = self
            .connections
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.in_node, c.out_node, (c.weight * 1000.0) as i64))
            .collect();
        self.sorted_conns_cache
            .sort_unstable_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    }

    /// 变异（所有变异逻辑使用同一个概率）
    pub fn mutate(&self, conf: &Config) -> Self {
        let rate = conf.mutation_rate;
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
            child.mutate_add_connection(conf);
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

        // SNN 参数变异（decay / threshold / refractory_period）
        for node in &mut child.nodes {
            if node.node_type == NodeType::Input {
                continue; // 输入节点始终直通
            }
            if rng.gen::<f64>() < rate {
                node.decay = (node.decay + rng.gen_range(-0.1..0.1)).clamp(0.0, 0.99);
            }
            if rng.gen::<f64>() < rate {
                node.threshold = (node.threshold + rng.gen_range(-0.15..0.15)).clamp(0.0, 2.0);
            }
            if rng.gen::<f64>() < rate * 0.5 {
                let delta: i8 = if rng.gen_bool(0.5) { 1 } else { -1 };
                node.refractory_period = (node.refractory_period as i8 + delta).clamp(0, 5) as u8;
            }
        }

        child.rebuild_sorted_cache();
        child
    }

    /// 分区分层偏好确定连接方位
    fn random_partition_layer(&mut self, node: &NodeGene, conf: &Config) -> (usize, usize) {
        let rate = conf.mutation_rate;
        let mut rng = rand::thread_rng();
        let mut partition = node.partition;
        let mut layer = node.layer;

        if node.node_type == NodeType::Output {
            // 输出分区不做连接
            return (partition, layer);
        }

        if rng.gen::<f64>() < rate {
            // 排除输入输出 以外的分区随机改变连接偏好分区
            partition = TierType::Other(rng.gen_range(0..conf.max_partitions));
            if partition != node.partition {
                layer = rng.gen_range(0..conf.max_layers); // 分区改变时随机分层
            } else {
                // 同一分区大概率向下，小概率随机
                if node.layer == 1 {
                    // 输出层大概率连接到输入层，少部分连接到其他层
                    if rng.gen::<f64>() < rate {
                        layer = rng.gen_range(2..=conf.max_layers);
                    } else {
                        layer = 0;
                    }
                } else {
                    if rng.gen::<f64>() < rate {
                        // 10% 机会随机改变连接偏好分层
                        layer = rng.gen_range(0..=conf.max_layers);
                    } else {
                        layer = rng.gen_range(layer..=conf.max_layers);
                    }
                }
            }
        } else {
            partition = node.preferred_partition; // 否则使用节点的连接偏好分区
        }

        (partition, layer)
    }

    /// 添加连接变异
    fn mutate_add_connection(&mut self, conf: &Config) {
        let rate = conf.mutation_rate;
        let mut rng = rand::thread_rng();

        // 收集有效的输入节点（输入和隐藏）
        let from_candidates: Vec<NodeGene> = self
            .nodes
            .iter()
            .filter(|n| n.node_type != NodeType::Output)
            .cloned()
            .collect();

        // 收集有效的输出节点（隐藏和输出）
        let to_candidates: Vec<NodeGene> = self
            .nodes
            .iter()
            .filter(|n| n.node_type != NodeType::Input)
            .cloned()
            .collect();

        if from_candidates.is_empty() || to_candidates.is_empty() {
            return;
        }

        // 尝试找到一个不存在的连接
        for _ in 0..10 {
            let from_node = from_candidates[rng.gen_range(0..from_candidates.len())];
            // 分区分层偏好确定连接方位
            let (to_partition, to_layer) = self.random_partition_layer(&from_node, conf);

            let to_node = to_candidates[rng.gen_range(0..to_candidates.len())];

            if from_node.id == to_node.id {
                continue; // 不允许自连接
            }

            // 检查连接是否已存在
            let exists = self
                .connections
                .iter()
                .any(|c| c.in_node == from_node && c.out_node == to_node);

            if !exists {
                self.connections.push(ConnectionGene {
                    in_node: from_node,
                    out_node: to_node,
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

        // 创建新的隐藏节点（随机 SNN 参数）
        let new_node_id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Hidden,
            partition: 0,
            layer: 0,
            preferred_partition: 0,
            preferred_layer: 0,
            decay: rng.gen_range(0.5..0.95),
            threshold: rng.gen_range(0.3..1.0),
            refractory_period: rng.gen_range(1..=3),
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
        // 使用预排序缓存，零分配零排序
        let self_conns = &self.sorted_conns_cache;
        let other_conns = &other.sorted_conns_cache;

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
                    // weight 已乘1000存为 i64，差值/4000 等价于原来的 /4.0
                    let diff = (self_conns[i].2 - other_conns[j].2).unsigned_abs();
                    similarity_sum += 1.0 - diff as f64 / 4000.0;
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

        if total == 0 {
            1.0
        } else {
            (similarity_sum / total as f64).max(0.0)
        }
    }

    /// NEAT 有性繁殖：两个父代基因交叉产生子代
    pub fn crossover(parent_a: &Genome, parent_b: &Genome, a_is_fitter: bool) -> Genome {
        let (fitter, weaker) = if a_is_fitter {
            (parent_a, parent_b)
        } else {
            (parent_b, parent_a)
        };

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

        // 节点：取两方并集，共有节点 SNN 参数取平均
        let weaker_nodes: std::collections::HashMap<usize, &NodeGene> =
            weaker.nodes.iter().map(|n| (n.id, n)).collect();

        let mut node_ids: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut child_nodes = Vec::new();
        for node in &fitter.nodes {
            node_ids.insert(node.id);
            let mut blended = node.clone();
            if let Some(&weaker_node) = weaker_nodes.get(&node.id) {
                blended.decay = (node.decay + weaker_node.decay) / 2.0;
                blended.threshold = (node.threshold + weaker_node.threshold) / 2.0;
                blended.refractory_period = ((node.refractory_period as u16
                    + weaker_node.refractory_period as u16)
                    / 2) as u8;
            }
            child_nodes.push(blended);
        }
        for node in &weaker.nodes {
            if !node_ids.contains(&node.id) {
                node_ids.insert(node.id);
                child_nodes.push(node.clone());
            }
        }

        let next_node_id = fitter.next_node_id.max(weaker.next_node_id);

        let mut genome = Genome {
            nodes: child_nodes,
            connections: child_connections,
            next_node_id,
            sorted_conns_cache: Vec::new(),
        };
        genome.rebuild_sorted_cache();
        genome
    }
}
