use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::Config;

/// 累计触发的 add_node 变异次数（含未存活到下一代的子代）
pub static ADD_NODE_TRIGGERS: AtomicU64 = AtomicU64::new(0);
/// 累计触发的 add_connection 变异次数
pub static ADD_CONN_TRIGGERS: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

/// 发育时间默认值（用于反序列化旧存档兼容）
#[cfg(feature = "persistence")]
fn default_maturation_time() -> f64 {
    5000.0
}

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
            eligibility_decay: 0.8,
            reinforcement_rate: 0.001,
            metabolic_penalty: 0.0,
        }
    }
}

// ============================================================================
/// 奖励基因（控制奖励信号如何产生和处理）
// ============================================================================

/// 生理基因（控制各生理信号通道的敏感度，可演化）
/// 框架设计：每新增一种生理通道只需加一个 sensitivity 字段
#[derive(Clone, Debug)]
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct PhysioGene {
    /// 能量吸收敏感度 [0.0~2.0]
    pub pleasure_energy_sensitivity: f64,
    /// 痕迹吸收敏感度 [0.0~2.0]
    pub pleasure_trail_sensitivity: f64,
    /// 集体行为敏感度 [0.0~2.0]（跟随+群居散热）
    pub pleasure_group_sensitivity: f64,
}

impl Default for PhysioGene {
    fn default() -> Self {
        Self {
            pleasure_energy_sensitivity: 1.0,
            pleasure_trail_sensitivity: 1.0,
            pleasure_group_sensitivity: 1.0,
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
    /// 生理基因（各通道敏感度）
    #[cfg_attr(feature = "persistence", serde(default))]
    pub physio: PhysioGene,

    /// 发育时间基因（模拟秒），控制结构变异的活跃窗口
    #[cfg_attr(feature = "persistence", serde(default = "default_maturation_time"))]
    pub maturation_time: f64,

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
    /// 输入维度 = 20，每种感官单侧投射，靠同源跨连让对侧使用：
    /// 左眼 [0..7]: 扫描角, 接近度, 能量, 实体类型, 基因相似度/同族, 能量密度, 朝向差, 速度差   → block -1
    /// 右眼 [8..15]: 扫描角, 接近度, 能量, 实体类型, 基因相似度/同族, 能量密度, 朝向差, 速度差   → block +1
    /// 自身 [16]: 能量(/2000)                                                                  → block -3（内省）
    /// 地形 [17]: 前方坡度方向                                                                 → block +3（外感觉）
    /// 光语言 [18]: 扫描角归一化(-1~1)                                                          → block -2（"光耳"）
    /// 光语言 [19]: 发光强度(0~1)                                                               → block -2（"光耳"）
    pub const INPUT_SIZE: usize = 20;
    /// 输出维度（固定8个）
    /// [0] 转向角  tanh(-1~1)                              block 25
    /// [1] 速度    tanh(-1~1) → abs后映射                   block 25
    /// [2] 嘴      tanh(-1~1)  负=咬, 接触食物自动吸收       block 25
    /// [3] 繁殖    tanh(-1~1)  >0.2时触发                   block -25
    /// [4] 繁殖阈值 sigmoid(0~1) → 映射到 20~200 能量        block -25
    /// [5] 子代能量比例 sigmoid(0~1) → 映射到 0.1~0.5        block -25
    /// [6] 痕迹强度 正半轴(0~1) → 映射到 0~0.3               block 25
    /// [7] 发光强度 tanh→(v+1)/2 映射到 0~1, 量化一位小数     block -26（光嘴/语言生成）
    pub const OUTPUT_SIZE: usize = 8;

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

        // 3. 创建感官区 Block 节点（每种感官占一对镜像 abs 中的某一侧，靠同源跨连传到对侧）
        let sensory_blocks: [(i8, std::ops::Range<usize>); 5] = [
            (-1, 0..8),   // 左眼 → Input 0~7
            (1, 8..16),   // 右眼 → Input 8~15
            (-3, 16..17), // 自身能量（内省）→ Input 16
            (3, 17..18),  // 地形感知（外感觉）→ Input 17
            (-2, 18..20), // 光语言（"光耳"）→ Input 18(方位) + 19(强度)
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

        // 4. 创建运动区 Block 节点：Block(25)运动, Block(-25)繁殖, Block(-26)光嘴
        // 光嘴放 -26 跟光耳 -2 同侧，闭合语言通路（同侧前馈概率远高于跨半球）
        let motor_blocks: [i8; 3] = [25, -25, -26];
        let mut motor_node_ids: Vec<(i8, usize)> = Vec::new();
        for &blk in &motor_blocks {
            let node_id = next_id;
            nodes.push(NodeGene {
                id: node_id,
                node_type: NodeType::Block(blk),
                layer: LayerType::Output,
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
            motor_node_ids.push((blk, node_id));
        }

        // 5. 感官区 Block → 各运动区 Block
        for &(_sblk, sensory_id) in &sensory_node_ids {
            for &(_mblk, motor_id) in &motor_node_ids {
                connections.push(ConnectionGene {
                    in_node: sensory_id,
                    out_node: motor_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        // 6. 运动区 Block → Output（按 block 归属连接）
        for i in 0..Self::OUTPUT_SIZE {
            let output_id = output_start + i;
            let output_blk = block::motor_block_for_output(i);
            // 找到对应的运动区 Block 节点
            let motor_id = motor_node_ids
                .iter()
                .find(|(blk, _)| *blk == output_blk)
                .map(|(_, id)| *id)
                .unwrap();
            let connect_count = rng.gen_range(min_connections..=max_connections);
            for _ in 0..connect_count {
                connections.push(ConnectionGene {
                    in_node: motor_id,
                    out_node: output_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        let mut block_probs = HashMap::new();
        for blk in -26..=26 {
            block_probs.insert(blk, ConnProbsGene::default());
        }

        let mut genome = Self {
            nodes,
            connections,
            conn_probs: block_probs,
            learning: LearningGene::default(),
            physio: PhysioGene::default(),
            maturation_time: 5000.0,
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

    /// 变异（base/block 两类共享 config.mutation_rate）
    /// parent_age: 父代繁殖时的年龄，用于发育期调制结构变异概率
    /// is_sexual: 是否为有性繁殖。无性繁殖只允许权重微调（rate × asexual_mutation_scale），
    ///   不进行 add_connection / add_node / enable 切换 / SNN 参数 / Layer / Block 迁移 /
    ///   block_probs / learning / physio 这 7 类"创新型"变异——演化创新带宽收归有性繁殖
    pub fn mutate(&self, conf: &Config, parent_age: f64, is_sexual: bool) -> Self {
        let base_rate = conf.mutation_rate;
        let block_rate = conf.mutation_rate;
        let weight_rate = if is_sexual {
            base_rate
        } else {
            base_rate * conf.asexual_mutation_scale
        };
        let mut rng = rand::thread_rng();
        let mut child = self.clone();

        // 发育进度：parent_age / maturation_time
        let p = if self.maturation_time > 0.0 {
            parent_age / self.maturation_time
        } else {
            1.0
        };
        // 结构变异调制因子：幼年活跃(2x)，成年后衰减
        let structure_factor = 2.0 * (-p).exp();

        // 权重变异（不受发育期影响）；无性繁殖时 rate 按 asexual_mutation_scale 缩放
        for conn in &mut child.connections {
            if rng.gen::<f64>() < weight_rate {
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

        // 以下 7 类"创新型"变异仅在有性繁殖时发生
        if is_sexual {
            // 添加连接变异（受发育期调制）
            if rng.gen::<f64>() < base_rate * structure_factor {
                ADD_CONN_TRIGGERS.fetch_add(1, Ordering::Relaxed);
                child.mutate_add_connection(conf);
            }

            // 添加节点变异（受发育期调制）
            if rng.gen::<f64>() < base_rate * structure_factor {
                ADD_NODE_TRIGGERS.fetch_add(1, Ordering::Relaxed);
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
                    node.refractory_period =
                        (node.refractory_period as i8 + delta).clamp(0, 5) as u8;
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

            // 生理基因变异（使用 base_rate）
            child.mutate_physio_gene(base_rate);
        }

        child.rebuild_sorted_cache();
        child
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

    /// target_pref 变异：遗忘 + 扰动现有条目 + 添加新条目 + 删除冷条目
    fn mutate_target_pref(probs: &mut ConnProbsGene, rate: f64, rng: &mut impl Rng) {
        const MAX_ENTRIES: usize = 16;
        // 遗忘率：所有条目每代朝中性 1.0 衰减，半衰期约 17 代
        // 选择压平衡点：≥4% 才能撑到 cap，2~3% 稳在 2~4，<1% 仅是软提示
        const FORGET_RATE: f32 = 0.04;

        // 操作0：遗忘——所有条目朝中性 1.0 衰减（无条件每代）
        for w in probs.target_pref.values_mut() {
            *w = *w * (1.0 - FORGET_RATE) + FORGET_RATE;
        }

        // 操作1：扰动现有条目（对数空间，每条目独立判定）
        for w in probs.target_pref.values_mut() {
            if rng.gen::<f64>() < rate {
                let delta: f32 = rng.gen_range(-0.2..0.2);
                *w *= delta.exp();
                *w = w.clamp(0.1, 10.0);
            }
        }

        // 操作2：添加新条目（与删除等速，消除膨胀偏置）
        if probs.target_pref.len() < MAX_ENTRIES && rng.gen::<f64>() < rate * 0.3 {
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

        // 操作3：删除最接近 1.0 的冷条目（与添加等速）
        if !probs.target_pref.is_empty() && rng.gen::<f64>() < rate * 0.3 {
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

    /// PhysioGene变异
    fn mutate_physio_gene(&mut self, rate: f64) {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() < rate {
            self.physio.pleasure_energy_sensitivity = (self.physio.pleasure_energy_sensitivity
                + rng.gen_range(-0.1..0.1))
            .clamp(0.0, 2.0);
        }
        if rng.gen::<f64>() < rate {
            self.physio.pleasure_trail_sensitivity =
                (self.physio.pleasure_trail_sensitivity + rng.gen_range(-0.1..0.1)).clamp(0.0, 2.0);
        }
        if rng.gen::<f64>() < rate {
            self.physio.pleasure_group_sensitivity =
                (self.physio.pleasure_group_sensitivity + rng.gen_range(-0.1..0.1)).clamp(0.0, 2.0);
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
    pub fn node_block(node: &NodeGene) -> i8 {
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
    ///
    /// 跨半球前馈（CrossForwardOther）受同源约束：仅允许 |from|==|to| 的镜像点连接，
    /// 仿胼胝体拓扑（左 V1↔右 V1，左 12↔右 12），禁止跨级跨半球（如 -3→+12）。
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
            ConnTarget::CrossForwardOther => {
                !same_block && forward && !same_side && block::is_homotopic(from_blk, to_blk)
            }
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
                    weight: rng.gen_range(-0.1..0.1),
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
                    weight: rng.gen_range(-0.1..0.1),
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
        // 直读 pass-through 初始化：与基础大脑节点参数一致，确保 add_node 是中性变异
        // （连接拓扑已按 NEAT 经典做法做中性插入：A→B=1, B→C=原A→C 权重）
        // threshold/decay/refractory 仍是基因，由后续 mutate_snn_params 演化决定是否走脉冲
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Block(new_block),
            layer: new_layer,
            decay: 0.0,
            threshold: 0.0,
            refractory_period: 0,
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
    /// Crossover（全原子孟德尔遗传）
    ///
    /// 所有连续参数按"原子"（不可分割的功能单元）从某一父代整取，
    /// crossover 不创造新值，只做重组。新值由 mutation 提供。
    /// 这保证 crossover 算子不会主动收缩群体方差。
    ///
    /// 原子粒度：
    /// - 每条共享连接（整个 ConnectionGene）
    /// - 每个共享节点（整个 NodeGene，含 SNN 参数和 layer）
    /// - 每个 block 的 ConnProbsGene（整块）
    /// - LearningGene（整块）
    /// - PhysioGene（整块）
    ///
    /// 独有连接/节点来自 fitter（标准 NEAT excess/disjoint），
    /// 保留 weaker 独有节点以防基因流失（和旧代码一致）。
    pub fn crossover(parent_a: &Genome, parent_b: &Genome, a_is_fitter: bool) -> Genome {
        let (fitter, weaker) = if a_is_fitter {
            (parent_a, parent_b)
        } else {
            (parent_b, parent_a)
        };

        let mut rng = rand::thread_rng();

        // === 连接：按位孟德尔 ===
        let weaker_conns: std::collections::HashMap<(usize, usize), &ConnectionGene> = weaker
            .connections
            .iter()
            .map(|c| ((c.in_node, c.out_node), c))
            .collect();

        let mut child_connections = Vec::new();
        for conn in &fitter.connections {
            let key = (conn.in_node, conn.out_node);
            if let Some(&weaker_conn) = weaker_conns.get(&key) {
                // 共有连接：整条 50/50 选一方
                let picked = if rng.gen_bool(0.5) { conn } else { weaker_conn };
                child_connections.push(picked.clone());
            } else {
                // fitter 独有：继承 fitter
                child_connections.push(conn.clone());
            }
        }

        // === 节点：按 id 孟德尔（与连接对称：disjoint/excess 只从 fitter 取） ===
        // 注意：weaker 独有节点不保留，避免成为孤儿节点
        // （add_node 创新需要节点+连接共同传递；只有 fitter 当 self 时整套拓扑才完整继承）
        let weaker_nodes: std::collections::HashMap<usize, &NodeGene> =
            weaker.nodes.iter().map(|n| (n.id, n)).collect();

        let mut child_nodes = Vec::new();
        for node in &fitter.nodes {
            let picked = if let Some(&weaker_node) = weaker_nodes.get(&node.id) {
                // 共有节点：整个 NodeGene 50/50 选一方
                if rng.gen_bool(0.5) {
                    node.clone()
                } else {
                    weaker_node.clone()
                }
            } else {
                node.clone()
            };
            child_nodes.push(picked);
        }

        let next_node_id = fitter.next_node_id.max(weaker.next_node_id);

        // === conn_probs：每个 block 作为原子 ===
        let child_block_probs = {
            let mut block_probs = HashMap::new();
            let all_blocks: std::collections::HashSet<i8> = fitter
                .conn_probs
                .keys()
                .chain(weaker.conn_probs.keys())
                .cloned()
                .collect();
            for blk in all_blocks {
                let fp = fitter.conn_probs.get(&blk);
                let wp = weaker.conn_probs.get(&blk);
                let merged = match (fp, wp) {
                    (Some(fp), Some(wp)) => {
                        // 共有 block：整个 ConnProbsGene 50/50 选一方
                        if rng.gen_bool(0.5) {
                            fp.clone()
                        } else {
                            wp.clone()
                        }
                    }
                    (Some(p), None) | (None, Some(p)) => p.clone(),
                    (None, None) => ConnProbsGene::default(),
                };
                block_probs.insert(blk, merged);
            }
            block_probs
        };

        // === LearningGene：整块原子 ===
        let child_learning = if rng.gen_bool(0.5) {
            fitter.learning.clone()
        } else {
            weaker.learning.clone()
        };

        // === PhysioGene：整块原子 ===
        let child_physio = if rng.gen_bool(0.5) {
            fitter.physio.clone()
        } else {
            weaker.physio.clone()
        };

        // === maturation_time：原子选取 ===
        let child_maturation = if rng.gen_bool(0.5) {
            fitter.maturation_time
        } else {
            weaker.maturation_time
        };

        let mut genome = Genome {
            nodes: child_nodes,
            connections: child_connections,
            conn_probs: child_block_probs,
            learning: child_learning,
            physio: child_physio,
            maturation_time: child_maturation,
            next_node_id,
            sorted_conns_cache: Vec::new(),
        };
        genome.rebuild_sorted_cache();
        genome
    }
}
