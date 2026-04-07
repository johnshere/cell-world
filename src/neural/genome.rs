use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::config::Config;

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 层类型：Block 节点在区内的角色
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub enum LayerType {
    /// 区内处理（含接收信号）
    Processing,
    /// 对外投射，跨区优先
    Output,
}

/// 节点类型
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub enum NodeType {
    /// 固定感官输入点
    Input,
    /// 处理节点，i8 分区编号（-31~31）
    Block(i8),
    /// 固定动作输出点
    Output,
}

fn default_layer() -> LayerType {
    LayerType::Processing
}
fn default_decay() -> f64 {
    0.0
}
fn default_threshold() -> f64 {
    0.0
}

// ============================================================================
/// 连接概率基因（控制新连接的拓扑方向偏好）
// ============================================================================

/// 单个分区的连接概率（Processing层5方向 + Output层5方向）
/// 5方向: SameBlockProcessing, SameBlockOutput, CrossForwardSame, CrossForwardOther, CrossFeedback
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct ConnProbsGene {
    /// Processing层5方向概率 [同区Proc, 同区Out, 跨区前馈同侧, 跨区前馈对侧, 跨区反馈]
    pub proc: [f64; 5],
    /// Output层5方向概率
    pub out: [f64; 5],
    /// 目标 block 偏好倍率（稀疏存储）
    /// key=目标 block 编号，value=偏好倍率（>1=偏爱，<1=回避，缺省=1.0=中性）
    /// 在 5 方向筛选出候选后，作为二次加权采样的权重
    #[cfg_attr(feature = "persistence", serde(default))]
    pub target_pref: HashMap<i8, f32>,
}

impl Default for ConnProbsGene {
    fn default() -> Self {
        // 对应原有硬编码概率表
        Self {
            proc: [0.70, 0.15, 0.08, 0.02, 0.05],
            out: [0.10, 0.05, 0.55, 0.10, 0.20],
            target_pref: HashMap::new(),
        }
    }
}

// ============================================================================
/// 变异基因（控制各类变异的概率）
// ============================================================================

#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct MutationGene {
    /// 基础变异率：权重、启用/禁用、添加连接、SNN参数、layer切换
    pub base: f64,
    /// 分区变异率：block移动、区块概率基因变异
    pub block: f64,
}

impl Default for MutationGene {
    fn default() -> Self {
        Self {
            base: 0.15,
            block: 0.15,
        }
    }
}

// ============================================================================
/// 学习基因（控制神经网络如何从经验中学习）
// ============================================================================

#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct LearningGene {
    /// 学习开关 [0.0=禁用, 1.0=启用]
    pub learning_on: f64,
    /// Hebbian学习率 [0.0~0.5]
    pub hebbian_rate: f64,
    /// Hebbian符号 [0.0~1.0]，0.5=平衡，<0.5偏弱化，>0.5偏强化
    pub hebbian_sign: f64,
    /// 资格迹衰减率 [0.0~0.99]
    pub eligibility_decay: f64,
    /// TD强化学习率 [0.0~0.1]
    pub reinforcement_rate: f64,
    /// 代谢惩罚系数 [0.0~1.0]
    pub metabolic_penalty: f64,
}

impl Default for LearningGene {
    fn default() -> Self {
        Self {
            learning_on: 0.5,
            hebbian_rate: 0.01,
            hebbian_sign: 0.7,
            eligibility_decay: 0.95,
            reinforcement_rate: 0.001,
            metabolic_penalty: 0.0,
        }
    }
}

// ============================================================================
/// 奖励基因（控制奖励信号如何产生和处理）
// ============================================================================

#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct RewardGene {
    /// 奖励信号缩放 [0.0~2.0]
    pub reward_scale: f64,
    /// TD折扣因子 [0.0~0.99]
    pub td_discount: f64,
    /// 能量敏感度 [0.0~1.0]
    pub energy_sensitivity: f64,
    /// 奖励延迟容忍 [0.0~1.0]
    pub reward_delay_tolerance: f64,
}

impl Default for RewardGene {
    fn default() -> Self {
        Self {
            reward_scale: 1.0,
            td_discount: 0.9,
            energy_sensitivity: 1.0,
            reward_delay_tolerance: 0.5,
        }
    }
}

/// 节点基因
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct NodeGene {
    pub id: usize,
    /// 分区类型（Input/Output/Block(-31~31)）
    pub node_type: NodeType,
    /// 区内层类型（仅 Block 节点有意义）
    #[cfg_attr(feature = "persistence", serde(default = "default_layer"))]
    pub layer: LayerType,
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

    /// 连接概率基因（32分区 × 2层 × 5方向）
    pub conn_probs: HashMap<i8, ConnProbsGene>,
    /// 学习基因
    pub learning: LearningGene,
    /// 奖励基因
    pub reward: RewardGene,

    /// 变异率基因
    pub mutation_rate: MutationGene,

    next_node_id: usize,
    /// 预排序的启用连接缓存（用于快速 similarity 比较，避免每次重复排序+分配）
    #[cfg_attr(feature = "persistence", serde(skip))]
    sorted_conns_cache: Vec<(usize, usize, i64)>,
}

/// 连接目标类别（用于加权概率采样）
#[derive(Clone, Copy)]
enum ConnTarget {
    SameBlockProcessing, // 同区 Processing
    SameBlockOutput,     // 同区 Output
    CrossForwardSame,    // 跨区前馈（同侧）
    CrossForwardOther,   // 跨区前馈（对侧）
    CrossFeedback,       // 跨区反馈
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

    /// 创建最小基因组：Input → 感官区Block → 运动区Block → Output
    pub fn random_minimal(min_connections: usize, max_connections: usize) -> Self {
        use super::block;
        let mut rng = rand::thread_rng();
        let mut nodes = Vec::new();
        let mut connections = Vec::new();
        let mut next_id = 0usize;

        // 1. 创建固定 Input 节点（17个）
        for _i in 0..Self::INPUT_SIZE {
            nodes.push(NodeGene {
                id: next_id,
                node_type: NodeType::Input,
                layer: LayerType::Processing, // Input 节点 layer 无意义
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
        }

        // 2. 创建固定 Output 节点（7个）
        let output_start = next_id;
        for _i in 0..Self::OUTPUT_SIZE {
            nodes.push(NodeGene {
                id: next_id,
                node_type: NodeType::Output,
                layer: LayerType::Processing, // Output 节点 layer 无意义
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
        }

        // 3. 创建感官区 Block 节点：Block(-1)左眼, Block(1)右眼, Block(0)体感
        let sensory_blocks: [(i8, std::ops::Range<usize>); 3] = [
            (-1, 0..8),  // 左眼 → Input 0~7
            (1, 8..16),  // 右眼 → Input 8~15
            (0, 16..17), // 体感 → Input 16
        ];
        let mut sensory_node_ids: Vec<(i8, usize)> = Vec::new(); // (block, node_id)
        for &(blk, ref input_range) in &sensory_blocks {
            let node_id = next_id;
            nodes.push(NodeGene {
                id: node_id,
                node_type: NodeType::Block(blk),
                layer: LayerType::Processing,
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
            sensory_node_ids.push((blk, node_id));

            // Input → 感官区 Block
            for input_id in input_range.clone() {
                connections.push(ConnectionGene {
                    in_node: input_id,
                    out_node: node_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        // 4. 创建运动区 Block 节点：Block(25)
        let motor_blk: i8 = block::motor_block_for_output(0);
        let motor_node_id = next_id;
        nodes.push(NodeGene {
            id: motor_node_id,
            node_type: NodeType::Block(motor_blk),
            layer: LayerType::Output, // 运动区初始为 Output 层，负责对外投射
            decay: 0.0,
            threshold: 0.0,
            refractory_period: 0,
        });
        next_id += 1;

        // 5. 感官区 Block → 运动区 Block
        for &(_blk, sensory_id) in &sensory_node_ids {
            connections.push(ConnectionGene {
                in_node: sensory_id,
                out_node: motor_node_id,
                weight: rng.gen_range(-1.0..1.0),
                enabled: true,
            });
        }

        // 6. 运动区 Block → Output（随机连接）
        for i in 0..Self::OUTPUT_SIZE {
            let output_id = output_start + i;
            let connect_count = rng.gen_range(min_connections..=max_connections);
            for _ in 0..connect_count {
                connections.push(ConnectionGene {
                    in_node: motor_node_id,
                    out_node: output_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        let mut block_probs = HashMap::new();
        for blk in -24..=24 {
            if blk == 0 {
                continue;
            }
            block_probs.insert(blk, ConnProbsGene::default());
        }

        let mut genome = Self {
            nodes,
            connections,
            conn_probs: block_probs,
            learning: LearningGene::default(),
            reward: RewardGene::default(),
            mutation_rate: MutationGene::default(),
            next_node_id: next_id,
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

    /// 变异（分离 base 和 block 变异率）
    pub fn mutate(&self, conf: &Config) -> Self {
        let base_rate = self.mutation_rate.base;
        let block_rate = self.mutation_rate.block;
        let mut rng = rand::thread_rng();
        let mut child = self.clone();

        // 权重变异
        for conn in &mut child.connections {
            if rng.gen::<f64>() < base_rate {
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
        if rng.gen::<f64>() < base_rate {
            child.mutate_add_connection(conf);
        }

        // 添加节点变异（使用 base_rate）
        if rng.gen::<f64>() < base_rate {
            child.mutate_add_node(base_rate);
        }

        // 禁用/启用连接变异
        if rng.gen::<f64>() < base_rate {
            if let Some(conn) = child.connections.choose_mut(&mut rng) {
                conn.enabled = !conn.enabled;
            }
        }

        // SNN 参数变异 + Layer 变异（使用 base_rate）
        for node in &mut child.nodes {
            if !matches!(node.node_type, NodeType::Block(_)) {
                continue;
            }
            // SNN 参数
            if rng.gen::<f64>() < base_rate {
                node.decay = (node.decay + rng.gen_range(-0.1..0.1)).clamp(0.0, 0.99);
            }
            if rng.gen::<f64>() < base_rate {
                node.threshold = (node.threshold + rng.gen_range(-0.15..0.15)).clamp(0.0, 2.0);
            }
            if rng.gen::<f64>() < base_rate * 0.5 {
                let delta: i8 = if rng.gen_bool(0.5) { 1 } else { -1 };
                node.refractory_period = (node.refractory_period as i8 + delta).clamp(0, 5) as u8;
            }
            // Layer 变异：使用 base_rate
            if rng.gen::<f64>() < base_rate {
                node.layer = match node.layer {
                    LayerType::Processing => LayerType::Output,
                    LayerType::Output => LayerType::Processing,
                };
            }
        }

        // Block 变异（仅联合区节点可变，使用 block_rate）
        for node in &mut child.nodes {
            if let NodeType::Block(ref mut blk) = node.node_type {
                if super::block::is_association(*blk) && rng.gen::<f64>() < block_rate {
                    let delta: i8 = rng.gen_range(-2..=2);
                    let new_blk = (*blk + delta).clamp(-24, 24);
                    if super::block::is_association(new_blk) {
                        *blk = new_blk;
                    }
                }
            }
        }

        // block_probs变异（使用 block_rate）
        child.mutate_block_probs_gene(block_rate);

        // learning基因变异（使用 base_rate）
        child.mutate_learning_gene(base_rate);

        // reward基因变异（使用 base_rate）
        child.mutate_reward_gene(base_rate);

        // mutation_rate基因自身变异：base 和 block 独立变异
        child.mutate_mutation_rate_gene();

        child.rebuild_sorted_cache();
        child
    }

    /// mutation_rate基因自身变异（base 和 block 独立变异）
    fn mutate_mutation_rate_gene(&mut self) {
        let mut rng = rand::thread_rng();

        // base 变异
        if rng.gen::<f64>() < self.mutation_rate.base {
            if rng.gen::<f64>() < 0.9 {
                self.mutation_rate.base += rng.gen_range(-0.02..0.02);
            } else {
                self.mutation_rate.base = rng.gen_range(0.01..0.30);
            }
            self.mutation_rate.base = self.mutation_rate.base.clamp(0.01, 0.30);
        }

        // block 变异
        if rng.gen::<f64>() < self.mutation_rate.block {
            if rng.gen::<f64>() < 0.9 {
                self.mutation_rate.block += rng.gen_range(-0.02..0.02);
            } else {
                self.mutation_rate.block = rng.gen_range(0.01..0.30);
            }
            self.mutation_rate.block = self.mutation_rate.block.clamp(0.01, 0.30);
        }
    }

    /// block_probs基因变异（使用统一rate）
    fn mutate_block_probs_gene(&mut self, rate: f64) {
        let mut rng = rand::thread_rng();

        // 每个分区概率基因独立变异
        for (_, probs) in &mut self.conn_probs {
            if rng.gen::<f64>() < rate {
                Self::mutate_single_block_probs(probs, &mut rng);
            }
            // target_pref 三种操作：扰动 / 添加 / 删除
            Self::mutate_target_pref(probs, rate, &mut rng);
        }
    }

    /// target_pref 变异：扰动现有条目 + 添加新条目 + 删除冷条目
    fn mutate_target_pref(probs: &mut ConnProbsGene, rate: f64, rng: &mut impl Rng) {
        const MAX_ENTRIES: usize = 16;

        // 操作1：扰动现有条目（对数空间，每条目独立判定）
        for w in probs.target_pref.values_mut() {
            if rng.gen::<f64>() < rate {
                let delta: f32 = rng.gen_range(-0.2..0.2);
                *w *= delta.exp();
                *w = w.clamp(0.1, 10.0);
            }
        }

        // 操作2：添加新条目（中频）
        if probs.target_pref.len() < MAX_ENTRIES && rng.gen::<f64>() < rate * 0.5 {
            // 在 -31..=31（排除 0）中随机选一个未出现的目标 block
            for _ in 0..8 {
                let candidate: i8 = rng.gen_range(-31..=31);
                if candidate == 0 || probs.target_pref.contains_key(&candidate) {
                    continue;
                }
                let initial: f32 = 1.0 + rng.gen_range(-0.3..0.3);
                probs.target_pref.insert(candidate, initial);
                break;
            }
        }

        // 操作3：删除最接近 1.0 的冷条目（控制熵）
        if !probs.target_pref.is_empty() && rng.gen::<f64>() < rate * 0.2 {
            if let Some((&k, _)) = probs.target_pref.iter().min_by(|a, b| {
                (a.1 - 1.0)
                    .abs()
                    .partial_cmp(&(b.1 - 1.0).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                probs.target_pref.remove(&k);
            }
        }

        // 上限保护：超过 MAX_ENTRIES 时强制删除最冷条目
        while probs.target_pref.len() > MAX_ENTRIES {
            if let Some((&k, _)) = probs.target_pref.iter().min_by(|a, b| {
                (a.1 - 1.0)
                    .abs()
                    .partial_cmp(&(b.1 - 1.0).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                probs.target_pref.remove(&k);
            } else {
                break;
            }
        }
    }

    /// LearningGene变异
    fn mutate_learning_gene(&mut self, rate: f64) {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < rate {
            self.learning.learning_on =
                (self.learning.learning_on + rng.gen_range(-0.1..0.1)).clamp(0.0, 1.0);
        }
        if rng.gen::<f64>() < rate {
            self.learning.hebbian_rate =
                (self.learning.hebbian_rate + rng.gen_range(-0.005..0.005)).clamp(0.0, 0.5);
        }
        if rng.gen::<f64>() < rate {
            self.learning.hebbian_sign =
                (self.learning.hebbian_sign + rng.gen_range(-0.05..0.05)).clamp(0.0, 1.0);
        }
        if rng.gen::<f64>() < rate {
            self.learning.eligibility_decay =
                (self.learning.eligibility_decay + rng.gen_range(-0.02..0.02)).clamp(0.0, 0.99);
        }
        if rng.gen::<f64>() < rate {
            self.learning.reinforcement_rate =
                (self.learning.reinforcement_rate + rng.gen_range(-0.001..0.001)).clamp(0.0, 0.1);
        }
        if rng.gen::<f64>() < rate {
            self.learning.metabolic_penalty =
                (self.learning.metabolic_penalty + rng.gen_range(-0.05..0.05)).clamp(0.0, 1.0);
        }
    }

    /// RewardGene变异
    fn mutate_reward_gene(&mut self, rate: f64) {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < rate {
            self.reward.reward_scale =
                (self.reward.reward_scale + rng.gen_range(-0.1..0.1)).clamp(0.0, 2.0);
        }
        if rng.gen::<f64>() < rate {
            self.reward.td_discount =
                (self.reward.td_discount + rng.gen_range(-0.05..0.05)).clamp(0.0, 0.99);
        }
        if rng.gen::<f64>() < rate {
            self.reward.energy_sensitivity =
                (self.reward.energy_sensitivity + rng.gen_range(-0.05..0.05)).clamp(0.0, 1.0);
        }
        if rng.gen::<f64>() < rate {
            self.reward.reward_delay_tolerance =
                (self.reward.reward_delay_tolerance + rng.gen_range(-0.05..0.05)).clamp(0.0, 1.0);
        }
    }

    /// 单个分区概率基因变异（加性扰动 + 归一化）
    fn mutate_single_block_probs(probs: &mut ConnProbsGene, rng: &mut impl Rng) {
        let epsilon = 0.05;
        for layer_probs in [&mut probs.proc, &mut probs.out] {
            for p in layer_probs.iter_mut() {
                *p += rng.gen_range(-epsilon..epsilon);
                *p = p.clamp(0.01, 1.0); // 避免塌陷到0
            }
            // 归一化和为1
            let sum: f64 = layer_probs.iter().sum();
            if sum > 0.0 {
                for p in layer_probs.iter_mut() {
                    *p /= sum;
                }
            }
        }
    }

    /// 获取节点的 block 编号（Input/Output 通过映射获取，Block 直接读取）
    fn node_block(node: &NodeGene) -> i8 {
        match node.node_type {
            NodeType::Input => super::block::sensory_block_for_input(node.id),
            NodeType::Output => {
                super::block::motor_block_for_output(node.id.saturating_sub(Self::INPUT_SIZE))
            }
            NodeType::Block(b) => b,
        }
    }

    /// 按概率表加权采样连接目标类别（从基因查表）
    fn conn_target(
        block: i8,
        layer: LayerType,
        block_probs: &HashMap<i8, ConnProbsGene>,
        rng: &mut impl Rng,
    ) -> ConnTarget {
        let r = rng.gen::<f64>();

        // 查找该block的概率表；若不存在，使用默认值
        let probs: ConnProbsGene = block_probs
            .get(&block)
            .cloned()
            .unwrap_or_else(ConnProbsGene::default);

        let layer_probs = match layer {
            LayerType::Processing => &probs.proc,
            LayerType::Output => &probs.out,
        };

        // 加权累积采样
        let mut cumsum = 0.0;
        for (i, &p) in layer_probs.iter().enumerate() {
            cumsum += p;
            if r < cumsum {
                return match i {
                    0 => ConnTarget::SameBlockProcessing,
                    1 => ConnTarget::SameBlockOutput,
                    2 => ConnTarget::CrossForwardSame,
                    3 => ConnTarget::CrossForwardOther,
                    4 => ConnTarget::CrossFeedback,
                    _ => ConnTarget::SameBlockProcessing,
                };
            }
        }

        // 防退化兜底
        ConnTarget::SameBlockProcessing
    }

    /// 判断候选目标节点是否匹配指定的连接目标类别
    fn matches_conn_target(from_blk: i8, to_node: &NodeGene, target: ConnTarget) -> bool {
        use super::block;
        let to_blk = Self::node_block(to_node);
        let same_block = from_blk == to_blk;
        let same_side = block::is_same_side(from_blk, to_blk);
        let forward = block::is_forward(from_blk, to_blk);
        let to_is_output_layer =
            matches!(to_node.node_type, NodeType::Block(_)) && to_node.layer == LayerType::Output;

        match target {
            ConnTarget::SameBlockProcessing => same_block && !to_is_output_layer,
            ConnTarget::SameBlockOutput => same_block && to_is_output_layer,
            ConnTarget::CrossForwardSame => !same_block && forward && same_side,
            ConnTarget::CrossForwardOther => !same_block && forward && !same_side,
            ConnTarget::CrossFeedback => !same_block && !forward,
        }
    }

    /// 添加连接变异（分区分层感知，加权概率采样）
    fn mutate_add_connection(&mut self, _conf: &Config) {
        use super::block;
        let mut rng = rand::thread_rng();

        // 随机选源节点（Input 或 Block）
        let from_candidates: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !matches!(n.node_type, NodeType::Output))
            .map(|(i, _)| i)
            .collect();

        if from_candidates.is_empty() {
            return;
        }

        let from_idx = from_candidates[rng.gen_range(0..from_candidates.len())];
        let from_node = &self.nodes[from_idx];
        let from_blk = Self::node_block(from_node);
        let from_id = from_node.id;

        // === 硬约束路径：Input 只连同 block 感官区 ===
        if matches!(from_node.node_type, NodeType::Input) {
            let targets: Vec<usize> = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    matches!(n.node_type, NodeType::Block(b) if b == from_blk) && n.id != from_id
                })
                .map(|(i, _)| i)
                .collect();
            if targets.is_empty() {
                return;
            }
            let to_idx = targets[rng.gen_range(0..targets.len())];
            let to_id = self.nodes[to_idx].id;
            let exists = self
                .connections
                .iter()
                .any(|c| c.in_node == from_id && c.out_node == to_id);
            if !exists {
                self.connections.push(ConnectionGene {
                    in_node: from_id,
                    out_node: to_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
            return;
        }

        // === Block 源节点：按概率表采样目标类别 ===
        let from_layer = from_node.layer;

        // 收集所有可能的目标节点（Block 和 Output）
        let all_targets: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !matches!(n.node_type, NodeType::Input) && n.id != from_id)
            .map(|(i, _)| i)
            .collect();

        if all_targets.is_empty() {
            return;
        }

        for _ in 0..20 {
            // 采样目标类别
            let target_type = Self::conn_target(from_blk, from_layer, &self.conn_probs, &mut rng);

            // 筛选匹配该类别的候选目标
            let matching: Vec<usize> = all_targets
                .iter()
                .copied()
                .filter(|&i| {
                    let to_node = &self.nodes[i];
                    // 硬约束：Output 只从运动区接收
                    if matches!(to_node.node_type, NodeType::Output) {
                        return block::is_motor(from_blk);
                    }
                    Self::matches_conn_target(from_blk, to_node, target_type)
                })
                .collect();

            if matching.is_empty() {
                continue; // 该类别无候选，重新采样
            }

            // 按源 block 的 target_pref 做加权采样：缺省 1.0=中性
            let from_pref = self.conn_probs.get(&from_blk);
            let to_idx = {
                let weights: Vec<f32> = matching
                    .iter()
                    .map(|&i| {
                        let to_blk = Self::node_block(&self.nodes[i]);
                        from_pref
                            .and_then(|p| p.target_pref.get(&to_blk).copied())
                            .unwrap_or(1.0)
                    })
                    .collect();
                let total: f32 = weights.iter().sum();
                if total <= 0.0 {
                    matching[rng.gen_range(0..matching.len())]
                } else {
                    let mut r = rng.gen::<f32>() * total;
                    let mut chosen = matching[matching.len() - 1];
                    for (k, &i) in matching.iter().enumerate() {
                        r -= weights[k];
                        if r <= 0.0 {
                            chosen = i;
                            break;
                        }
                    }
                    chosen
                }
            };
            let to_id = self.nodes[to_idx].id;

            let exists = self
                .connections
                .iter()
                .any(|c| c.in_node == from_id && c.out_node == to_id);

            if !exists {
                self.connections.push(ConnectionGene {
                    in_node: from_id,
                    out_node: to_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
                return;
            }
        }
    }

    /// 添加节点变异（继承源节点分区，小概率变异到新区）
    fn mutate_add_node(&mut self, rate: f64) {
        use super::block;
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

        // 确定新节点的 block：大概率继承源节点，小概率随机联合区
        let in_node = self.nodes.iter().find(|n| n.id == old_conn.in_node);
        let src_block = in_node.map(|n| Self::node_block(n)).unwrap_or(0);
        let src_side: i8 = if src_block >= 0 { 1 } else { -1 };

        let new_block = if rng.gen::<f64>() < rate {
            // 小概率：随机联合区（同侧优先）
            block::random_association_block(src_side, &mut rng)
        } else {
            src_block
        };

        // 确定新节点的 layer
        let new_layer = if rng.gen::<f64>() < rate {
            LayerType::Output
        } else {
            LayerType::Processing
        };

        let new_node_id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Block(new_block),
            layer: new_layer,
            decay: rng.gen_range(0.5..0.95),
            threshold: rng.gen_range(0.3..1.0),
            refractory_period: rng.gen_range(1..=3),
        });

        // 创建两个新连接
        self.connections.push(ConnectionGene {
            in_node: old_conn.in_node,
            out_node: new_node_id,
            weight: 1.0,
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

        // block_probs 交叉：逐block独立交叉
        let child_block_probs = {
            let mut block_probs = HashMap::new();
            let all_blocks: std::collections::HashSet<i8> = fitter
                .conn_probs
                .keys()
                .chain(weaker.conn_probs.keys())
                .cloned()
                .collect();
            for blk in all_blocks {
                let fitter_probs = fitter.conn_probs.get(&blk);
                let weaker_probs = weaker.conn_probs.get(&blk);
                let merged = match (fitter_probs, weaker_probs) {
                    (Some(fp), Some(wp)) => {
                        let mut proc = [0.0; 5];
                        let mut out = [0.0; 5];
                        for i in 0..5 {
                            proc[i] = (fp.proc[i] + wp.proc[i]) / 2.0;
                            out[i] = (fp.out[i] + wp.out[i]) / 2.0;
                        }
                        // 归一化
                        let proc_sum: f64 = proc.iter().sum();
                        if proc_sum > 0.0 {
                            for p in &mut proc {
                                *p /= proc_sum;
                            }
                        }
                        let out_sum: f64 = out.iter().sum();
                        if out_sum > 0.0 {
                            for p in &mut out {
                                *p /= out_sum;
                            }
                        }
                        // target_pref 合并：并集，重叠键取几何平均
                        let mut target_pref: HashMap<i8, f32> = fp.target_pref.clone();
                        for (&k, &wv) in &wp.target_pref {
                            target_pref
                                .entry(k)
                                .and_modify(|fv| *fv = (*fv * wv).sqrt())
                                .or_insert(wv);
                        }
                        ConnProbsGene { proc, out, target_pref }
                    }
                    (Some(p), None) | (None, Some(p)) => p.clone(),
                    (None, None) => ConnProbsGene::default(),
                };
                block_probs.insert(blk, merged);
            }
            block_probs
        };

        // learning基因交叉
        let child_learning = LearningGene {
            learning_on: (fitter.learning.learning_on + weaker.learning.learning_on) / 2.0,
            hebbian_rate: (fitter.learning.hebbian_rate + weaker.learning.hebbian_rate) / 2.0,
            hebbian_sign: (fitter.learning.hebbian_sign + weaker.learning.hebbian_sign) / 2.0,
            eligibility_decay: (fitter.learning.eligibility_decay
                + weaker.learning.eligibility_decay)
                / 2.0,
            reinforcement_rate: (fitter.learning.reinforcement_rate
                + weaker.learning.reinforcement_rate)
                / 2.0,
            metabolic_penalty: (fitter.learning.metabolic_penalty
                + weaker.learning.metabolic_penalty)
                / 2.0,
        };

        // reward基因交叉
        let child_reward = RewardGene {
            reward_scale: (fitter.reward.reward_scale + weaker.reward.reward_scale) / 2.0,
            td_discount: (fitter.reward.td_discount + weaker.reward.td_discount) / 2.0,
            energy_sensitivity: (fitter.reward.energy_sensitivity
                + weaker.reward.energy_sensitivity)
                / 2.0,
            reward_delay_tolerance: (fitter.reward.reward_delay_tolerance
                + weaker.reward.reward_delay_tolerance)
                / 2.0,
        };

        let mut genome = Genome {
            nodes: child_nodes,
            connections: child_connections,
            conn_probs: child_block_probs,
            learning: child_learning,
            reward: child_reward,
            mutation_rate: MutationGene {
                base: (fitter.mutation_rate.base + weaker.mutation_rate.base) / 2.0,
                block: (fitter.mutation_rate.block + weaker.mutation_rate.block) / 2.0,
            },
            next_node_id,
            sorted_conns_cache: Vec::new(),
        };
        genome.rebuild_sorted_cache();
        genome
    }
}
