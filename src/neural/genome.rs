use rand::Rng;
use rustc_hash::FxHashMap;
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

/// C1 软上限：当 `cap = Some(n)` 时，若 from_id / to_id 任一端点的活跃连接数 ≥ n 则拒绝
/// `cap = None` 表示无上限（演化中调用）
#[inline]
fn passes_c1_cap(
    connections: &[ConnectionGene],
    from_id: usize,
    to_id: usize,
    cap: Option<usize>,
) -> bool {
    let Some(limit) = cap else {
        return true;
    };
    let mut from_n = 0usize;
    let mut to_n = 0usize;
    for c in connections {
        if !c.enabled {
            continue;
        }
        if c.in_node == from_id || c.out_node == from_id {
            from_n += 1;
        }
        if c.in_node == to_id || c.out_node == to_id {
            to_n += 1;
        }
    }
    from_n < limit && to_n < limit
}

/// Input/Output 节点 ID 判定：id ∈ [0, INPUT_SIZE + OUTPUT_SIZE)
#[inline]
fn is_io_node(node_id: usize) -> bool {
    node_id < Genome::INPUT_SIZE + Genome::OUTPUT_SIZE
}

/// 统计每个节点（仅启用连接）的连接数
fn conn_counts_per_node(connections: &[ConnectionGene]) -> HashMap<usize, usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for c in connections {
        if !c.enabled {
            continue;
        }
        *counts.entry(c.in_node).or_default() += 1;
        *counts.entry(c.out_node).or_default() += 1;
    }
    counts
}

/// 公共校验：连接是否满足 Input/Output 双重硬约束（block + layer）
/// - Input → 仅可连其指定感官 block 的 Processing 层节点
/// - Output → 仅可由其指定运动 block 的 Output 层节点接收
/// - 非 I/O 端点直接放行
/// release 与 debug 均生效
#[inline]
pub(crate) fn validate_io_edge(nodes: &[NodeGene], from_id: usize, to_id: usize) -> bool {
    use super::block;
    let from_node = nodes.iter().find(|n| n.id == from_id);
    let to_node = nodes.iter().find(|n| n.id == to_id);
    let (Some(fnode), Some(tnode)) = (from_node, to_node) else {
        return false;
    };
    // Input 源：目标必须是指定 block + Processing layer
    if matches!(fnode.node_type, NodeType::Input) {
        let designated = block::sensory_block_for_input(from_id);
        if Genome::node_block(tnode) != designated {
            return false;
        }
        if tnode.layer != LayerType::Processing {
            return false;
        }
    }
    // Output 目标：源必须是指定 motor block + Output layer
    if matches!(tnode.node_type, NodeType::Output) {
        let out_idx = to_id.saturating_sub(Genome::INPUT_SIZE);
        let designated = block::motor_block_for_output(out_idx);
        if Genome::node_block(fnode) != designated {
            return false;
        }
        if fnode.layer != LayerType::Output {
            return false;
        }
    }
    true
}

/// 力导图视角：判断连接是否可从基因组中删除
/// - 涉及 Input/Output 且通过硬约束校验的连接不可删除（28 条必要 I/O 边）
/// - 非 I/O 边和非合规 I/O 边均可删除
/// - 孤儿边（端点不在 nodes 中）可删除
#[inline]
pub(crate) fn is_connection_deletable(nodes: &[NodeGene], from_id: usize, to_id: usize) -> bool {
    let from_node = nodes.iter().find(|n| n.id == from_id);
    let to_node = nodes.iter().find(|n| n.id == to_id);
    let (Some(fnode), Some(tnode)) = (from_node, to_node) else {
        return true; // orphan edge
    };
    let involves_io =
        matches!(fnode.node_type, NodeType::Input) || matches!(tnode.node_type, NodeType::Output);
    if involves_io {
        !validate_io_edge(nodes, from_id, to_id) // deletable only if invalid
    } else {
        true // non-I/O edges always deletable
    }
}

/// 反向校验：已有边是否涉及 I/O 端点且违反硬约束（用于清理历史违规边）
/// - 非 I/O 边永远返回 false（不需要清理）
/// - I/O 边反向取 `!validate_io_edge` 结果
#[inline]
fn is_io_edge_invalid(nodes: &[NodeGene], from_id: usize, to_id: usize) -> bool {
    let from_node = nodes.iter().find(|n| n.id == from_id);
    let to_node = nodes.iter().find(|n| n.id == to_id);
    let (Some(fnode), Some(tnode)) = (from_node, to_node) else {
        return false;
    };
    let involves_io =
        matches!(fnode.node_type, NodeType::Input) || matches!(tnode.node_type, NodeType::Output);
    involves_io && !validate_io_edge(nodes, from_id, to_id)
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

    /// 创建最小基因组（v2.4.1 56 节点 + 68 边预制骨架）：
    ///
    /// **节点（56 = 2n）**：
    /// - 20 Input + 8 Output = 28 个固定 I/O 节点
    /// - 每 input 在其指定感官 block 中独占 1 个 Proc 节点（无 Out 配对）
    /// - 每 output 在其指定运动 block 中独占 1 个 Out 节点（无 Proc 配对）
    /// - 合计 28 + 20 + 8 = **56 节点**
    ///
    /// **必要连接（68 条，全部权重 `[-1.0, 1.0]`）**：
    /// 1. input_i → 其独占 Proc_i（20 条，I/O 硬约束）
    /// 2. 独占 Out_j → output_j（8 条，I/O 硬约束）
    /// 3. **C 方案 cross 边**：对每个 motor Out，从每个 sensory block 内随机选 1 个 Proc，
    ///    连一条 Proc → motor Out 的跨 block 边。8 motor Out × 5 sensory block = **40 条**
    ///
    /// 3 跳路径 `input → 独占 Proc → motor Out → output` 在 t=0 就连通，
    /// 0.5³ × max_speed ≈ 2.5 远超 0.05 阈值，初代 100% 能动。
    ///
    /// **C1 ≤10 自检**：每 motor Out 接 5 cross + 1 out = 6；单 Proc block（-3/+3）出 8 + 入 1 = 9 临界；
    /// 双 Proc block（-2）出 ~4 + 入 1 = 5；8 Proc block（-1/+1）出 ~1 + 入 1 = 2。
    ///
    /// **C2（block 内 Proc+Out 共存）由演化补齐**：sensory 只有 Proc / motor 只有 Out，
    /// 故意打破 C2 给演化留扩张空间，C2 只在 `mutate_add_node` 触发时 90% 概率补齐缺失类型。
    pub fn random_minimal() -> Self {
        use super::block;
        let mut rng = rand::thread_rng();
        let mut nodes = Vec::new();
        let mut connections = Vec::new();
        let mut next_id = 0usize;

        // 1. 创建固定 Input 节点
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

        // 2. 创建固定 Output 节点
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

        // 3. 每个 input：独占 Proc（在其指定 sensory block，无 Out 配对）
        // 同时按 block 分组记录 Proc id，供步骤 5 的 C 方案使用
        let mut sensory_proc_by_block: FxHashMap<i8, Vec<usize>> = FxHashMap::default();
        for input_id in 0..Self::INPUT_SIZE {
            let blk = block::sensory_block_for_input(input_id);

            let proc_id = next_id;
            nodes.push(NodeGene {
                id: proc_id,
                node_type: NodeType::Block(blk),
                layer: LayerType::Processing,
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
            sensory_proc_by_block.entry(blk).or_default().push(proc_id);

            // input → 独占 Proc（必要边大权重）
            connections.push(ConnectionGene {
                in_node: input_id,
                out_node: proc_id,
                weight: rng.gen_range(-1.0..1.0),
                enabled: true,
            });
        }

        // 4. 每个 output：独占 Out（在其指定 motor block，无 Proc 配对）
        // 记录每个 output 对应的独占 Out id，供步骤 5 使用
        let mut output_dedicated_out: Vec<usize> = Vec::with_capacity(Self::OUTPUT_SIZE);
        for output_idx in 0..Self::OUTPUT_SIZE {
            let output_id = output_start + output_idx;
            let blk = block::motor_block_for_output(output_idx);

            let out_id = next_id;
            nodes.push(NodeGene {
                id: out_id,
                node_type: NodeType::Block(blk),
                layer: LayerType::Output,
                decay: 0.0,
                threshold: 0.0,
                refractory_period: 0,
            });
            next_id += 1;
            output_dedicated_out.push(out_id);

            // 独占 Out → output（必要边大权重）
            connections.push(ConnectionGene {
                in_node: out_id,
                out_node: output_id,
                weight: rng.gen_range(-1.0..1.0),
                enabled: true,
            });
        }

        // 5. C 方案 cross 边：每 motor Out × 每 sensory block 选 1 随机 Proc 连边
        // 8 motor Out × 5 sensory block = 40 条，input 到任意 output 的 3 跳路径在 t=0 就连通
        for &motor_out_id in &output_dedicated_out {
            for procs in sensory_proc_by_block.values() {
                if procs.is_empty() {
                    continue;
                }
                let proc_id = procs[rng.gen_range(0..procs.len())];
                connections.push(ConnectionGene {
                    in_node: proc_id,
                    out_node: motor_out_id,
                    weight: rng.gen_range(-1.0..1.0),
                    enabled: true,
                });
            }
        }

        // 6. 初始化 block_probs（默认 5 方向概率 + 空 target_pref）
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
        // 约束：连接数 >5 的非 I/O 节点，其连线只能削弱（推向 0），不能增强或重置
        let conn_counts = conn_counts_per_node(&child.connections);
        for conn in &mut child.connections {
            if rng.gen::<f64>() < weight_rate {
                let in_overloaded = !is_io_node(conn.in_node)
                    && conn_counts.get(&conn.in_node).copied().unwrap_or(0) > 5;
                let out_overloaded = !is_io_node(conn.out_node)
                    && conn_counts.get(&conn.out_node).copied().unwrap_or(0) > 5;
                let restricted = in_overloaded || out_overloaded;

                if rng.gen::<f64>() < 0.9 {
                    // 微调
                    if restricted {
                        // 只能削弱：推向 0
                        let decay = rng.gen_range(0.0..0.5);
                        if conn.weight > 0.0 {
                            conn.weight = (conn.weight - decay).max(0.0);
                        } else if conn.weight < 0.0 {
                            conn.weight = (conn.weight + decay).min(0.0);
                        }
                        // weight == 0：已无法削弱，跳过
                    } else {
                        conn.weight += rng.gen_range(-0.5..0.5);
                    }
                    conn.weight = conn.weight.clamp(-2.0, 2.0);
                } else {
                    // 重置：受限连接跳过（重置可能增强）
                    if !restricted {
                        conn.weight = rng.gen_range(-1.0..1.0);
                    }
                }
            }
        }

        // 以下 7 类"创新型"变异仅在有性繁殖时发生
        if is_sexual {
            // 添加连接变异（受发育期调制）
            if rng.gen::<f64>() < base_rate * structure_factor {
                ADD_CONN_TRIGGERS.fetch_add(1, Ordering::Relaxed);
                child.mutate_add_connection(conf, None);
            }

            // 添加节点变异（受发育期调制）
            if rng.gen::<f64>() < base_rate * structure_factor {
                ADD_NODE_TRIGGERS.fetch_add(1, Ordering::Relaxed);
                child.mutate_add_node(base_rate);
            }

            // 禁用/启用连接变异
            // Input/Output 必须保留至少一条启用连接，不可全部禁掉
            if rng.gen::<f64>() < base_rate {
                let idx = rng.gen_range(0..child.connections.len());
                let conn_enabled = child.connections[idx].enabled;
                if conn_enabled {
                    // 即将禁用：检查是否会导致 Input/Output 失去唯一通路
                    let conn = &child.connections[idx];
                    let in_node = child.nodes.iter().find(|n| n.id == conn.in_node);
                    let out_node = child.nodes.iter().find(|n| n.id == conn.out_node);
                    let in_is_input =
                        in_node.map_or(false, |n| matches!(n.node_type, NodeType::Input));
                    let out_is_output =
                        out_node.map_or(false, |n| matches!(n.node_type, NodeType::Output));
                    let would_orphan = (in_is_input
                        && child
                            .connections
                            .iter()
                            .enumerate()
                            .filter(|(i, c)| {
                                *i != idx
                                    && c.enabled
                                    && c.in_node == conn.in_node
                                    && validate_io_edge(&child.nodes, c.in_node, c.out_node)
                            })
                            .count()
                            == 0)
                        || (out_is_output
                            && child
                                .connections
                                .iter()
                                .enumerate()
                                .filter(|(i, c)| {
                                    *i != idx
                                        && c.enabled
                                        && c.out_node == conn.out_node
                                        && validate_io_edge(&child.nodes, c.in_node, c.out_node)
                                })
                                .count()
                                == 0);
                    if !would_orphan {
                        child.connections[idx].enabled = false;
                    }
                } else {
                    // 启用前校验 I/O 硬约束，无效连接直接删除
                    let conn = &child.connections[idx];
                    if validate_io_edge(&child.nodes, conn.in_node, conn.out_node) {
                        child.connections[idx].enabled = true;
                    } else {
                        child.connections.remove(idx);
                    }
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
                // Layer 翻转已删除（v2.6）：翻转会打碎 Input→Proc / Out→Output 硬约束，
                // 且 C2 在 mutate_add_node 中已提供足够的 layer 探索通道。
            }

            // block_probs变异（使用 block_rate）
            child.mutate_block_probs_gene(block_rate);

            // learning基因变异（使用 base_rate）
            child.mutate_learning_gene(base_rate);

            // 生理基因变异（使用 base_rate）
            child.mutate_physio_gene(base_rate);
        }

        // I/O 硬约束全局修复：删违规边 → 清死节点 → 补孤点
        child.repair_invalid_io_edges();

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
    ///
    /// `max_per_node`：可选 C1 软上限——若任一端点的活跃连接数已达该值则跳过 push。
    /// - 演化中调用传 `None`，无任何上限
    /// - 初始化（random_minimal）调用传 `Some(10)`，避免初始拓扑过度集中
    fn mutate_add_connection(&mut self, _conf: &Config, max_per_node: Option<usize>) {
        use super::block;
        let mut rng = rand::thread_rng();

        // 源节点选择：先随机选 block，再在 block 内按 layer 加权选节点
        // Processing 偏向发新连接（60%），Output 偏向向外投射（40%）
        // Input 走独立硬约束路径，参与 block 随机但因只有自身而无加权

        // 按 block 分组非 Output 节点
        let mut from_by_blk: HashMap<i8, Vec<usize>> = HashMap::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if matches!(node.node_type, NodeType::Output) {
                continue;
            }
            let blk = Self::node_block(node);
            from_by_blk.entry(blk).or_default().push(i);
        }
        if from_by_blk.is_empty() {
            return;
        }

        // 随机选一个 block（各 block 等权）
        let blk_keys: Vec<i8> = from_by_blk.keys().copied().collect();
        let chosen_blk = blk_keys[rng.gen_range(0..blk_keys.len())];
        let candidates = &from_by_blk[&chosen_blk];

        // 源节点 layer 加权：Processing 60% / Output 40%
        let mut weights: Vec<f32> = Vec::with_capacity(candidates.len());
        for &ci in candidates {
            let layer = self.nodes[ci].layer;
            weights.push(if layer == LayerType::Processing {
                0.6
            } else {
                0.4
            });
        }

        let total: f32 = weights.iter().sum();
        let from_idx = if total <= 0.0 {
            candidates[rng.gen_range(0..candidates.len())]
        } else {
            let mut r = rng.gen::<f32>() * total;
            let mut chosen = candidates[candidates.len() - 1];
            for (k, &ci) in candidates.iter().enumerate() {
                r -= weights[k];
                if r <= 0.0 {
                    chosen = ci;
                    break;
                }
            }
            chosen
        };
        let from_node = &self.nodes[from_idx];
        let from_blk = Self::node_block(from_node);
        let from_id = from_node.id;

        // === 硬约束路径：Input 只连同 block 感官区的 Processing 层节点 ===
        if matches!(from_node.node_type, NodeType::Input) {
            let targets: Vec<usize> = self
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    matches!(n.node_type, NodeType::Block(b) if b == from_blk)
                        && n.layer == LayerType::Processing
                        && n.id != from_id
                })
                .map(|(i, _)| i)
                .collect();
            if targets.is_empty() {
                return;
            }
            let to_idx = targets[rng.gen_range(0..targets.len())];
            let to_id = self.nodes[to_idx].id;
            let exists = self.connections.iter().any(|c| {
                (c.in_node == from_id && c.out_node == to_id)
                    || (c.in_node == to_id && c.out_node == from_id)
            });
            if !exists
                && passes_c1_cap(&self.connections, from_id, to_id, max_per_node)
                && validate_io_edge(&self.nodes, from_id, to_id)
            {
                // 连接数 >5 的非 I/O 节点不允许新增连接
                let cc = conn_counts_per_node(&self.connections);
                let to_over = !is_io_node(to_id) && cc.get(&to_id).copied().unwrap_or(0) >= 5;
                if !to_over {
                    self.connections.push(ConnectionGene {
                        in_node: from_id,
                        out_node: to_id,
                        weight: rng.gen_range(-0.1..0.1),
                        enabled: true,
                    });
                }
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

        // 连接数 >5 的非 I/O 源节点不允许新增连接
        let cc = conn_counts_per_node(&self.connections);
        let from_overloaded = !is_io_node(from_id) && cc.get(&from_id).copied().unwrap_or(0) >= 5;
        if from_overloaded {
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
                    // 硬约束：Output 只从其指定运动区的 Output 层节点接收
                    if matches!(to_node.node_type, NodeType::Output) {
                        let out_idx = to_node.id.saturating_sub(Self::INPUT_SIZE);
                        return from_blk == block::motor_block_for_output(out_idx)
                            && from_layer == LayerType::Output;
                    }
                    Self::matches_conn_target(from_blk, to_node, target_type)
                })
                .collect();

            if matching.is_empty() {
                continue; // 该类别无候选，重新采样
            }

            // 按源 block 的 target_pref × C3 软偏好做加权采样
            // - target_pref：缺省 1.0=中性
            // - C3：跨 block 时 Proc 目标 ×3（仿生：跨区信号优先进入"前端处理"层）
            let from_pref = self.conn_probs.get(&from_blk);
            let to_idx = {
                let weights: Vec<f32> = matching
                    .iter()
                    .map(|&i| {
                        let to_node = &self.nodes[i];
                        let to_blk = Self::node_block(to_node);
                        let pref = from_pref
                            .and_then(|p| p.target_pref.get(&to_blk).copied())
                            .unwrap_or(1.0);
                        // C3 软偏好：跨 block 的 Block 目标节点，Proc ×3
                        let c3 = if to_blk != from_blk
                            && matches!(to_node.node_type, NodeType::Block(_))
                            && to_node.layer == LayerType::Processing
                        {
                            3.0
                        } else {
                            1.0
                        };
                        pref * c3
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

            let exists = self.connections.iter().any(|c| {
                (c.in_node == from_id && c.out_node == to_id)
                    || (c.in_node == to_id && c.out_node == from_id)
            });

            if !exists
                && passes_c1_cap(&self.connections, from_id, to_id, max_per_node)
                && validate_io_edge(&self.nodes, from_id, to_id)
            {
                // 连接数 >5 的非 I/O 目标节点不允许新增连接
                let to_overloaded = !is_io_node(to_id) && cc.get(&to_id).copied().unwrap_or(0) >= 5;
                if !to_overloaded {
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
    }

    /// 添加节点变异（分裂连接 A→B，插入新节点 X）
    /// - X 的 block/layer 由旧连接 A→B 的拓扑方向类别推导（保留旧连接的语义）
    /// - 选连接时按 1/源block节点数 加权，稀疏 block 优先享用 add_node
    fn mutate_add_node(&mut self, rate: f64) {
        use super::block;
        let mut rng = rand::thread_rng();

        // 选择一个启用的连接，按 1/源 block 节点数加权（稀疏 block 优先）
        let enabled_conns: Vec<(usize, f64)> = self
            .connections
            .iter()
            .enumerate()
            .filter(|(_, c)| c.enabled)
            .map(|(i, c)| {
                let in_blk = self
                    .nodes
                    .iter()
                    .find(|n| n.id == c.in_node)
                    .map(|n| Self::node_block(n))
                    .unwrap_or(0);
                let blk_node_count = self
                    .nodes
                    .iter()
                    .filter(|n| Self::node_block(n) == in_blk)
                    .count()
                    .max(1) as f64;
                (i, 1.0 / blk_node_count)
            })
            .collect();

        if enabled_conns.is_empty() {
            return;
        }

        let total_weight: f64 = enabled_conns.iter().map(|(_, w)| w).sum();
        let mut r = rng.gen::<f64>() * total_weight;
        let mut conn_idx = enabled_conns[enabled_conns.len() - 1].0;
        for &(idx, w) in &enabled_conns {
            r -= w;
            if r <= 0.0 {
                conn_idx = idx;
                break;
            }
        }

        let old_conn = self.connections[conn_idx].clone();

        // 禁用旧连接
        self.connections[conn_idx].enabled = false;

        let in_node = self.nodes.iter().find(|n| n.id == old_conn.in_node);
        let out_node = self.nodes.iter().find(|n| n.id == old_conn.out_node);
        let in_blk = in_node.map(|n| Self::node_block(n)).unwrap_or(0);
        let out_blk = out_node.map(|n| Self::node_block(n)).unwrap_or(0);

        let in_is_input = in_node.map_or(false, |n| matches!(n.node_type, NodeType::Input));
        let out_is_output = out_node.map_or(false, |n| matches!(n.node_type, NodeType::Output));

        // 判定 A→B 的方向类别
        let direction = if in_is_input || out_is_output {
            None // 保护区走硬约束，不参与方向分类
        } else {
            let same_block = in_blk == out_blk;
            let forward = block::is_forward(in_blk, out_blk);
            let same_side = block::is_same_side(in_blk, out_blk);
            let out_is_output_layer = out_node.map_or(false, |n| n.layer == LayerType::Output);
            if same_block && !out_is_output_layer {
                Some(ConnTarget::SameBlockProcessing)
            } else if same_block && out_is_output_layer {
                Some(ConnTarget::SameBlockOutput)
            } else if !same_block && forward && same_side {
                Some(ConnTarget::CrossForwardSame)
            } else if !same_block && forward && !same_side && block::is_homotopic(in_blk, out_blk) {
                Some(ConnTarget::CrossForwardOther)
            } else {
                Some(ConnTarget::CrossFeedback)
            }
        };

        // 确定新节点 X 的 block 和 layer
        let (new_block, new_layer_weight) = if in_is_input {
            (
                block::sensory_block_for_input(old_conn.in_node),
                Some(false), // Input→感官：layer 必须 Processing（硬约束）
            )
        } else if out_is_output {
            let out_idx = old_conn.out_node.saturating_sub(Self::INPUT_SIZE);
            (
                block::motor_block_for_output(out_idx),
                Some(true), // 感官→Output：layer 必须 Output（硬约束）
            )
        } else if let Some(dir) = direction {
            match dir {
                ConnTarget::SameBlockProcessing => {
                    // 同 block 处理：X 留在同 block，偏向 Processing
                    (in_blk, Some(false))
                }
                ConnTarget::SameBlockOutput => {
                    // 同 block 投射：X 留在同 block，偏向 Out
                    (in_blk, Some(true))
                }
                ConnTarget::CrossForwardSame => {
                    // 同侧前馈：X 落在 in_blk~out_blk 中间，偏向 Processing
                    let mid = in_blk + (out_blk - in_blk) / 2;
                    let side: i8 = if in_blk >= 0 { 1 } else { -1 };
                    let abs_mid = mid.unsigned_abs();
                    // 中间点在联合区范围则用中间点，否则取 in_blk
                    if (4..=24).contains(&abs_mid) {
                        (mid.abs() * side.signum(), Some(false))
                    } else {
                        (in_blk, Some(false))
                    }
                }
                ConnTarget::CrossForwardOther => {
                    // 对侧前馈：同上但跨半球，X 取 in_blk 侧
                    (in_blk, Some(false))
                }
                ConnTarget::CrossFeedback => {
                    // 反馈：X 偏向 out_blk 侧，layer 均衡
                    (out_blk, None)
                }
            }
        } else {
            // 兜底：继承 in_blk
            (in_blk, None)
        };

        // target_pref 微调 block：若 in 对 out_blk 有偏好，微调 X 的 block
        let new_block = if !in_is_input && !out_is_output {
            let from_pref = self.conn_probs.get(&in_blk);
            if let Some(pref) = from_pref.and_then(|p| p.target_pref.get(&out_blk).copied()) {
                if pref > 1.1 && block::is_association(out_blk) && (out_blk - new_block).abs() <= 2
                {
                    // in 偏爱 out_blk：X 靠近 out_blk
                    out_blk
                } else if pref < 0.9 && block::is_association(in_blk) {
                    // in 回避 out_blk：X 留在 in_blk
                    in_blk
                } else {
                    new_block
                }
            } else {
                new_block
            }
        } else {
            new_block
        };

        // === C2 永久软约束：每个 block 必须同时拥有 Proc + Out 节点 ===
        // 若 new_block 当前只缺一种类型，本次 add_node 高概率补齐该类型。
        // 仿生意义：感官区缺投射层无法对外发声，运动区缺处理层无法被驱动；
        // C2 保证 block 内部信号能从 Proc → Out 这条链路传递。
        //
        // 注意：Input/Output 边界不受 C2 影响——Input 只能连 Proc，Output 只能由 Out 驱动。
        // 此时 new_layer_weight 已强制为正确方向，C2 不参与。
        let blk_type_hint: Option<LayerType> = if in_is_input || out_is_output {
            None // I/O 边界：layer 由硬约束 new_layer_weight 决定，C2 不插手
        } else {
            let blk_proc_n = self
                .nodes
                .iter()
                .filter(|n| {
                    matches!(n.node_type, NodeType::Block(_))
                        && Self::node_block(n) == new_block
                        && n.layer == LayerType::Processing
                })
                .count();
            let blk_out_n = self
                .nodes
                .iter()
                .filter(|n| {
                    matches!(n.node_type, NodeType::Block(_))
                        && Self::node_block(n) == new_block
                        && n.layer == LayerType::Output
                })
                .count();
            if blk_proc_n == 0 && blk_out_n > 0 {
                // 只有 Out，缺 Proc → 90% 概率补 Proc
                if rng.gen::<f64>() < 0.9 {
                    Some(LayerType::Processing)
                } else {
                    None
                }
            } else if blk_out_n == 0 && blk_proc_n > 0 {
                // 只有 Proc，缺 Out → 90% 概率补 Out
                if rng.gen::<f64>() < 0.9 {
                    Some(LayerType::Output)
                } else {
                    None
                }
            } else {
                None
            }
        };

        // 确定 layer：方向推导 > 补全 > 随机
        let new_layer = if let Some(forced) = blk_type_hint {
            forced
        } else if let Some(prefer_out) = new_layer_weight {
            if prefer_out {
                if rng.gen::<f64>() < 0.6 {
                    LayerType::Output
                } else {
                    LayerType::Processing
                }
            } else {
                // 偏向 Processing
                if rng.gen::<f64>() < 0.1 {
                    LayerType::Output
                } else {
                    LayerType::Processing
                }
            }
        } else {
            if rng.gen::<f64>() < rate {
                LayerType::Output
            } else {
                LayerType::Processing
            }
        };

        let new_node_id = self.next_node_id;
        self.next_node_id += 1;
        // 直读 pass-through 初始化：与基础大脑节点参数一致，确保 add_node 是中性变异
        self.nodes.push(NodeGene {
            id: new_node_id,
            node_type: NodeType::Block(new_block),
            layer: new_layer,
            decay: 0.0,
            threshold: 0.0,
            refractory_period: 0,
        });

        // 创建两个新连接；I/O 硬约束守门
        if validate_io_edge(&self.nodes, old_conn.in_node, new_node_id) {
            self.connections.push(ConnectionGene {
                in_node: old_conn.in_node,
                out_node: new_node_id,
                weight: 1.0,
                enabled: true,
            });
        }

        if validate_io_edge(&self.nodes, new_node_id, old_conn.out_node) {
            self.connections.push(ConnectionGene {
                in_node: new_node_id,
                out_node: old_conn.out_node,
                weight: old_conn.weight,
                enabled: true,
            });
        }
    }

    /// I/O 硬约束全局修复（orchestrator）：
    /// 力导图手动拆线：在指定连接上插入新节点（用户自选 block 和 layer）
    /// - 旧连接 A→B 被禁用
    /// - 创建新节点 X（直读 pass-through：decay=0, threshold=0, refractory=0），block 和 layer 由调用方指定
    /// - 创建 A→X（weight=1.0）和 X→B（weight=旧连接的 weight），通过 I/O 硬约束校验
    /// - 若连接不存在或已禁用，返回 false 不做任何修改
    pub fn split_connection_with_block(
        &mut self,
        in_node: usize,
        out_node: usize,
        block: i8,
        layer: LayerType,
    ) -> bool {
        let idx = match self
            .connections
            .iter()
            .position(|c| c.in_node == in_node && c.out_node == out_node)
        {
            Some(i) => i,
            None => return false,
        };
        if !self.connections[idx].enabled {
            return false;
        }

        let old = self.connections[idx].clone();
        self.connections[idx].enabled = false;

        let new_id = self.next_node_id;
        self.next_node_id += 1;
        self.nodes.push(NodeGene {
            id: new_id,
            node_type: NodeType::Block(block),
            layer,
            decay: 0.0,
            threshold: 0.0,
            refractory_period: 0,
        });

        // A → X, weight=1.0
        if validate_io_edge(&self.nodes, old.in_node, new_id) {
            self.connections.push(ConnectionGene {
                in_node: old.in_node,
                out_node: new_id,
                weight: 1.0,
                enabled: true,
            });
        }
        // X → B, weight=旧权重
        if validate_io_edge(&self.nodes, new_id, old.out_node) {
            self.connections.push(ConnectionGene {
                in_node: new_id,
                out_node: old.out_node,
                weight: old.weight,
                enabled: true,
            });
        }
        true
    }

    /// Phase 1 → 删除所有违规 I/O 边（block/layer 不符）
    /// Phase 2 → 清理死 Block 节点
    /// Phase 3 → 修复 I/O 孤点（无有效启用线则补）
    fn repair_invalid_io_edges(&mut self) {
        // Phase 1: 删除违规 I/O 边
        let mut invalid_indices: Vec<usize> = self
            .connections
            .iter()
            .enumerate()
            .filter(|(_, c)| c.enabled && is_io_edge_invalid(&self.nodes, c.in_node, c.out_node))
            .map(|(i, _)| i)
            .collect();
        // 逆序删除避免索引偏移
        invalid_indices.sort_unstable_by(|a, b| b.cmp(a));
        for i in invalid_indices {
            self.connections.remove(i);
        }

        // Phase 2: 清理死 Block 节点
        self.prune_dead_output_nodes();

        // Phase 3: 修复 I/O 孤点
        self.repair_orphan_io();
    }

    /// 删除无效 Block 节点：出边全部 disabled 或入边全部 disabled
    /// Input/Output 节点不受影响，它们由 toggle 守卫 + 硬约束保护
    fn prune_dead_output_nodes(&mut self) {
        let mut has_enabled_out: FxHashMap<usize, bool> = FxHashMap::default();
        let mut has_enabled_in: FxHashMap<usize, bool> = FxHashMap::default();
        for conn in &self.connections {
            has_enabled_out.entry(conn.in_node).or_insert(false);
            has_enabled_in.entry(conn.out_node).or_insert(false);
            if conn.enabled {
                *has_enabled_out.get_mut(&conn.in_node).unwrap() = true;
                *has_enabled_in.get_mut(&conn.out_node).unwrap() = true;
            }
        }

        let dead_ids: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| {
                matches!(n.node_type, NodeType::Block(_))
                    && (!has_enabled_out.get(&n.id).copied().unwrap_or(false)
                        || !has_enabled_in.get(&n.id).copied().unwrap_or(false))
            })
            .map(|n| n.id)
            .collect();

        if dead_ids.is_empty() {
            return;
        }

        self.connections
            .retain(|c| !dead_ids.contains(&c.in_node) && !dead_ids.contains(&c.out_node));
        self.nodes.retain(|n| !dead_ids.contains(&n.id));
    }

    /// Input/Output 孤点修复：若全部启用线已断，强制重连到指定 block + 正确 layer 的节点
    /// Input 只能连 Processing，Output 只能由 Output 驱动（硬约束）
    fn repair_orphan_io(&mut self) {
        use super::block;
        let mut rng = rand::thread_rng();

        // 收集指定 block 中匹配 layer 的节点索引；若无匹配则 fallback 到任意 layer
        fn collect_candidates(
            nodes: &[NodeGene],
            blk: i8,
            preferred_layer: LayerType,
        ) -> Vec<usize> {
            let mut v: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    matches!(n.node_type, NodeType::Block(b) if b == blk)
                        && n.layer == preferred_layer
                })
                .map(|(i, _)| i)
                .collect();
            if v.is_empty() {
                v = nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| matches!(n.node_type, NodeType::Block(b) if b == blk))
                    .map(|(i, _)| i)
                    .collect();
            }
            v
        }

        for ni in 0..self.nodes.len() {
            let node = &self.nodes[ni];
            match node.node_type {
                NodeType::Input => {
                    let designated = block::sensory_block_for_input(node.id);
                    let has_enabled = self.connections.iter().any(|c| {
                        c.enabled
                            && c.in_node == node.id
                            && self.nodes.iter().any(|n| {
                                n.id == c.out_node
                                    && Self::node_block(n) == designated
                                    && n.layer == LayerType::Processing
                            })
                    });
                    if has_enabled {
                        continue;
                    }
                    let candidates =
                        collect_candidates(&self.nodes, designated, LayerType::Processing);
                    let best_idx = candidates
                        .iter()
                        .min_by_key(|&&i| {
                            let id = self.nodes[i].id;
                            self.connections
                                .iter()
                                .filter(|c| c.in_node == id || c.out_node == id)
                                .count()
                        })
                        .copied();
                    if let Some(best_idx) = best_idx {
                        let to_id = self.nodes[best_idx].id;
                        let exists = self
                            .connections
                            .iter()
                            .any(|c| c.in_node == node.id && c.out_node == to_id);
                        if !exists && validate_io_edge(&self.nodes, node.id, to_id) {
                            self.connections.push(ConnectionGene {
                                in_node: node.id,
                                out_node: to_id,
                                weight: rng.gen_range(-0.1..0.1),
                                enabled: true,
                            });
                        }
                    }
                }
                NodeType::Output => {
                    let out_idx = node.id.saturating_sub(Self::INPUT_SIZE);
                    let designated = block::motor_block_for_output(out_idx);
                    let has_enabled = self.connections.iter().any(|c| {
                        c.enabled
                            && c.out_node == node.id
                            && self.nodes.iter().any(|n| {
                                n.id == c.in_node
                                    && Self::node_block(n) == designated
                                    && n.layer == LayerType::Output
                            })
                    });
                    if has_enabled {
                        continue;
                    }
                    let candidates = collect_candidates(&self.nodes, designated, LayerType::Output);
                    let best_idx = candidates
                        .iter()
                        .min_by_key(|&&i| {
                            let id = self.nodes[i].id;
                            self.connections
                                .iter()
                                .filter(|c| c.in_node == id || c.out_node == id)
                                .count()
                        })
                        .copied();
                    if let Some(best_idx) = best_idx {
                        let from_id = self.nodes[best_idx].id;
                        let exists = self
                            .connections
                            .iter()
                            .any(|c| c.in_node == from_id && c.out_node == node.id);
                        if !exists && validate_io_edge(&self.nodes, from_id, node.id) {
                            self.connections.push(ConnectionGene {
                                in_node: from_id,
                                out_node: node.id,
                                weight: rng.gen_range(-0.1..0.1),
                                enabled: true,
                            });
                        }
                    }
                }
                _ => {}
            }
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
