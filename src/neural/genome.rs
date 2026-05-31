use rand::seq::SliceRandom;
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

        // ====== 临时梳理-4：连接数>7的节点只允许权重削弱，不允许增强 ======
        let conn_threshold: usize = 7;
        let mut node_conn_counts: FxHashMap<usize, usize> = FxHashMap::default();
        for conn in &self.connections {
            *node_conn_counts.entry(conn.in_node).or_default() += 1;
            *node_conn_counts.entry(conn.out_node).or_default() += 1;
        }
        // ====== 临时梳理-4 结束 ======

        // 权重变异（不受发育期影响）；无性繁殖时 rate 按 asexual_mutation_scale 缩放
        for conn in &mut child.connections {
            if rng.gen::<f64>() < weight_rate {
                let in_over =
                    node_conn_counts.get(&conn.in_node).copied().unwrap_or(0) > conn_threshold;
                let out_over =
                    node_conn_counts.get(&conn.out_node).copied().unwrap_or(0) > conn_threshold;
                let restricted = in_over || out_over;

                if rng.gen::<f64>() < 0.9 {
                    // 微调
                    let delta_range = if restricted {
                        (-0.5..0.0) // 受限：仅允许削弱
                    } else {
                        (-0.5..0.5) // 正常：正负均可
                    };
                    conn.weight += rng.gen_range(delta_range);
                    conn.weight = conn.weight.clamp(-2.0, 2.0);
                } else {
                    // 重置
                    let reset_range = if restricted {
                        (-1.0..0.0) // 受限：仅负权重
                    } else {
                        (-1.0..1.0)
                    };
                    conn.weight = rng.gen_range(reset_range);
                }
            }
            // ====== 临时梳理-4：受限连接弱到一定程度 → disabled ======
            let in_over =
                node_conn_counts.get(&conn.in_node).copied().unwrap_or(0) > conn_threshold;
            let out_over =
                node_conn_counts.get(&conn.out_node).copied().unwrap_or(0) > conn_threshold;
            if (in_over || out_over) && conn.enabled && conn.weight.abs() < 0.05 {
                conn.enabled = false;
            }
            // ====== 临时梳理-4 结束 ======
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
                            .filter(|c| c.enabled && c.in_node == conn.in_node)
                            .count()
                            == 1)
                        || (out_is_output
                            && child
                                .connections
                                .iter()
                                .filter(|c| c.enabled && c.out_node == conn.out_node)
                                .count()
                                == 1);
                    if !would_orphan {
                        child.connections[idx].enabled = false;
                    }
                } else {
                    child.connections[idx].enabled = true;
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

            // block_probs变异（使用 block_rate）
            child.mutate_block_probs_gene(block_rate);

            // learning基因变异（使用 base_rate）
            child.mutate_learning_gene(base_rate);

            // 生理基因变异（使用 base_rate）
            child.mutate_physio_gene(base_rate);
        }

        // ====== 临时修复：Input/Output 连接合规性（修复已完成，暂停） ======
        // 注：mutate_add_node 的 Input/Output 约束 + toggle 守卫已永久防止复发
        // 若有历史遗留再次出现，取消此行注释：
        // if rng.gen::<f64>() < base_rate * 0.5 { child.repair_io_connections(); }
        // ====== 临时修复结束 ======

        // ====== 临时梳理-2：渐进惩罚跨 block 违规连接（Proc→外界 / 外界→Out） ======
        if is_sexual && rng.gen::<f64>() < base_rate * 0.3 {
            child.rectify_cross_block_io();
        }
        // ====== 临时梳理-2 结束 ======

        // Input/Output 孤点修复：若全部启用线已断，强制重连到指定 block 中最少线的节点
        child.repair_orphan_io();

        // 删除无有效输出的 Block 节点（所有出边均已 disabled 或从无出边）
        child.prune_dead_output_nodes();

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

    // ====== 临时梳理-2：渐进惩罚跨 block 违规连接 ======
    /// 扫描全脑跨 block 连接，对违规连接（Proc→外界 / 外界→Out）渐进惩罚：
    ///   |weight| > 0.1 → weight *= 0.5（削弱）
    ///   |weight| ≤ 0.1 → enabled = false（退役）
    /// 每次随机挑最多 2 条违规连接处理
    fn rectify_cross_block_io(&mut self) {
        use super::block;
        let mut rng = rand::thread_rng();

        // 统计每个 block 是否有 Out / Proc 节点
        let blk_has: FxHashMap<i8, (bool, bool)> = {
            let mut m: FxHashMap<i8, (bool, bool)> = FxHashMap::default();
            for node in &self.nodes {
                if let NodeType::Block(_) = node.node_type {
                    let entry = m.entry(Self::node_block(node)).or_insert((false, false));
                    if node.layer == LayerType::Output {
                        entry.0 = true;
                    } else {
                        entry.1 = true;
                    }
                }
            }
            m
        };

        // 收集所有跨 block 违规连接（含索引）
        let mut violations: Vec<usize> = Vec::new();
        for (i, conn) in self.connections.iter().enumerate() {
            if !conn.enabled {
                continue;
            }
            let in_blk = self
                .nodes
                .iter()
                .find(|n| n.id == conn.in_node)
                .map(|n| Self::node_block(n))
                .unwrap_or(0);
            let out_blk = self
                .nodes
                .iter()
                .find(|n| n.id == conn.out_node)
                .map(|n| Self::node_block(n))
                .unwrap_or(0);
            // 仅跨 block 连接
            if in_blk == out_blk {
                continue;
            }
            // 忽略 Input/Output 保护区
            let in_node = self.nodes.iter().find(|n| n.id == conn.in_node);
            let out_node = self.nodes.iter().find(|n| n.id == conn.out_node);
            let in_is_input = in_node.map_or(false, |n| matches!(n.node_type, NodeType::Input));
            let out_is_output = out_node.map_or(false, |n| matches!(n.node_type, NodeType::Output));
            if in_is_input || out_is_output {
                continue;
            }

            let in_layer_ok = in_node.map_or(true, |n| {
                if n.layer == LayerType::Processing {
                    // Proc 当发射器：检查该 block 是否有 Out
                    !blk_has.get(&in_blk).map_or(false, |(has_out, _)| *has_out)
                } else {
                    // Out 当发射器：合规
                    true
                }
            });
            let out_layer_ok = out_node.map_or(true, |n| {
                if n.layer == LayerType::Output {
                    // Out 当接收器：检查该 block 是否有 Proc
                    !blk_has
                        .get(&out_blk)
                        .map_or(false, |(_, has_proc)| *has_proc)
                } else {
                    true
                }
            });
            // 两边都违规只算一条，优先算发射端违规
            if !in_layer_ok || (!in_layer_ok && !out_layer_ok) || !out_layer_ok {
                // 但确保 block 自身已配齐类型（有 Out/Proc 才判断违规）
                let in_has = blk_has.get(&in_blk).map_or((false, false), |v| *v);
                let out_has = blk_has.get(&out_blk).map_or((false, false), |v| *v);
                let in_violation = !in_layer_ok && in_has.0;
                let out_violation = !out_layer_ok && out_has.1;
                if in_violation || out_violation {
                    violations.push(i);
                }
            }
        }

        if violations.is_empty() {
            return;
        }

        // 随机挑最多 2 条处理
        let count = 2.min(violations.len());
        for _ in 0..count {
            let idx = violations[rng.gen_range(0..violations.len())];
            let conn = &mut self.connections[idx];
            if conn.weight.abs() > 0.1 {
                conn.weight *= 0.5;
            } else {
                conn.enabled = false;
            }
        }
    }
    // ====== 临时梳理-2 结束 ======

    /// 添加连接变异（分区分层感知，加权概率采样）
    fn mutate_add_connection(&mut self, _conf: &Config) {
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

        // ====== 临时梳理-1：若 block 有 Out，源极偏 Out（~95%）；Proc 不充当跨块发射器 ======
        // 临时完工后删除本段，恢复原有 Proc60%/Out40% 逻辑
        let blk_has_out_temp = self.nodes.iter().any(|n| {
            matches!(n.node_type, NodeType::Block(_))
                && Self::node_block(n) == chosen_blk
                && n.layer == LayerType::Output
        });
        let temp_out_boost = if blk_has_out_temp { 20.0_f32 } else { 1.0_f32 };
        let mut weights: Vec<f32> = Vec::with_capacity(candidates.len());
        for &ci in candidates {
            let layer = self.nodes[ci].layer;
            let base = if layer == LayerType::Processing {
                0.6
            } else {
                0.4
            };
            let w = if layer == LayerType::Output {
                base * temp_out_boost
            } else {
                base
            };
            weights.push(w);
        }
        // ====== 临时梳理-1 结束 ======

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

            // ====== 临时梳理-1：跨 block 目标若其 block 有 Proc，目标必须是 Proc ======
            let matching: Vec<usize> = matching
                .into_iter()
                .filter(|&i| {
                    let to_blk = Self::node_block(&self.nodes[i]);
                    if to_blk == from_blk {
                        return true; // 同 block 不限制
                    }
                    let to_layer = self.nodes[i].layer;
                    if to_layer == LayerType::Output {
                        // 目标 block 有 Proc 吗？
                        let blk_has_proc = self.nodes.iter().any(|n| {
                            matches!(n.node_type, NodeType::Block(_))
                                && Self::node_block(n) == to_blk
                                && n.layer == LayerType::Processing
                        });
                        // 有 Proc 但目标是 Out → 拒绝
                        !blk_has_proc
                    } else {
                        true // 目标是 Proc，总是合规
                    }
                })
                .collect();
            if matching.is_empty() {
                continue;
            }
            // ====== 临时梳理-1 结束 ======

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
                None, // Input→感官：layer 随机
            )
        } else if out_is_output {
            let out_idx = old_conn.out_node.saturating_sub(Self::INPUT_SIZE);
            (block::motor_block_for_output(out_idx), None)
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

        // ====== 临时梳理-3：补全 block 缺失的节点类型 ======
        // 统计 new_block 内 Proc/Out 节点数
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
        let blk_type_hint: Option<LayerType> = if blk_proc_n == 0 && blk_out_n > 0 {
            // 只有 Out，缺 Proc → 强制补 Proc（概率 90%）
            if rng.gen::<f64>() < 0.9 {
                Some(LayerType::Processing)
            } else {
                None
            }
        } else if blk_out_n == 0 && blk_proc_n > 0 {
            // 只有 Proc，缺 Out → 强制补 Out（概率 90%）
            if rng.gen::<f64>() < 0.9 {
                Some(LayerType::Output)
            } else {
                None
            }
        } else {
            None
        };
        // ====== 临时梳理-3 结束 ======

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

    // ====== 临时修复：Input/Output 连接合规性 ======
    /// 每次调用同时做三件事：
    ///   1. Input/Output 若已无一条指向指定 block 的启用连接 → 恢复一条 disabled
    ///   2. Input→非指定 block / 非指定 block→Output 的违规连接 → 删除
    ///   3. 删除单线孤点（Block 节点总连接 ≤1）
    /// 调用点见 mutate() 末尾注释，需要时取消注释即可恢复
    #[allow(dead_code)]
    fn repair_io_connections(&mut self) {
        use super::block;
        let input_size = Self::INPUT_SIZE;
        let output_start = input_size;

        // —— 1. 恢复：Input/Output 必须至少有一条指向指定 block 的启用连接 ——
        for node in &self.nodes {
            match node.node_type {
                NodeType::Input => {
                    let designated = block::sensory_block_for_input(node.id);
                    let designated_nodes: Vec<usize> = self
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(
                            |(_, n)| matches!(n.node_type, NodeType::Block(b) if b == designated),
                        )
                        .map(|(i, _)| i)
                        .collect();
                    if designated_nodes.is_empty() {
                        continue;
                    }
                    let has_enabled = self.connections.iter().any(|c| {
                        c.enabled
                            && c.in_node == node.id
                            && designated_nodes
                                .iter()
                                .any(|&di| self.nodes[di].id == c.out_node)
                    });
                    if !has_enabled {
                        // 找一条指向指定 block 的 disabled 连接恢复
                        if let Some(conn) = self.connections.iter_mut().find(|c| {
                            !c.enabled
                                && c.in_node == node.id
                                && designated_nodes
                                    .iter()
                                    .any(|&di| self.nodes[di].id == c.out_node)
                        }) {
                            conn.enabled = true;
                        }
                    }
                }
                NodeType::Output => {
                    let out_idx = node.id.saturating_sub(output_start);
                    let designated = block::motor_block_for_output(out_idx);
                    let designated_nodes: Vec<usize> = self
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(
                            |(_, n)| matches!(n.node_type, NodeType::Block(b) if b == designated),
                        )
                        .map(|(i, _)| i)
                        .collect();
                    if designated_nodes.is_empty() {
                        continue;
                    }
                    let has_enabled = self.connections.iter().any(|c| {
                        c.enabled
                            && c.out_node == node.id
                            && designated_nodes
                                .iter()
                                .any(|&di| self.nodes[di].id == c.in_node)
                    });
                    if !has_enabled {
                        if let Some(conn) = self.connections.iter_mut().find(|c| {
                            !c.enabled
                                && c.out_node == node.id
                                && designated_nodes
                                    .iter()
                                    .any(|&di| self.nodes[di].id == c.in_node)
                        }) {
                            conn.enabled = true;
                        }
                    }
                }
                _ => {}
            }
        }

        // —— 2. 删除：Input→非指定 block / 非指定 block→Output 的违规连接 ——
        // 收集要删除的索引，从后往前删避免索引漂移
        // 守卫：至少保留一条正确的启用连接，避免孤儿 Input/Output
        let mut to_remove: Vec<usize> = Vec::new();
        for (i, conn) in self.connections.iter().enumerate() {
            // Input → 非指定感官区
            if conn.in_node < input_size {
                let designated = block::sensory_block_for_input(conn.in_node);
                let out_node = self.nodes.iter().find(|n| n.id == conn.out_node);
                let out_blk = out_node.map(|n| Self::node_block(n)).unwrap_or(0);
                if out_blk != designated {
                    let same_in_correct = self
                        .connections
                        .iter()
                        .enumerate()
                        .filter(|(j, c)| {
                            *j != i && c.enabled && c.in_node == conn.in_node && {
                                let out_n = self.nodes.iter().find(|n| n.id == c.out_node);
                                out_n.map_or(false, |n| Self::node_block(n) == designated)
                            }
                        })
                        .count();
                    if same_in_correct > 0 {
                        to_remove.push(i);
                    }
                    continue;
                }
            }
            // 非指定运动区 → Output
            if conn.out_node >= output_start && conn.out_node < output_start + Self::OUTPUT_SIZE {
                let out_idx = conn.out_node.saturating_sub(output_start);
                let designated = block::motor_block_for_output(out_idx);
                let in_node = self.nodes.iter().find(|n| n.id == conn.in_node);
                let in_blk = in_node.map(|n| Self::node_block(n)).unwrap_or(0);
                if in_blk != designated {
                    // 保留至少一条正确的启用连接
                    let same_out_correct = self
                        .connections
                        .iter()
                        .enumerate()
                        .filter(|(j, c)| {
                            *j != i && c.enabled && c.out_node == conn.out_node && {
                                let in_n = self.nodes.iter().find(|n| n.id == c.in_node);
                                in_n.map_or(false, |n| Self::node_block(n) == designated)
                            }
                        })
                        .count();
                    if same_out_correct > 0 {
                        to_remove.push(i);
                    }
                }
            }
        }
        // 从后往前删
        to_remove.sort_unstable();
        for &idx in to_remove.iter().rev() {
            self.connections.remove(idx);
        }

        // —— 3. 删除单线孤点：Block 节点仅有一条连接（进或出）→ 无回路，纯代谢累赘 ——
        // disabled 线也算正常连接，只清理真正孤立（总连接数=1）的节点
        let mut orphan_node_ids: Vec<usize> = Vec::new();
        for node in &self.nodes {
            if !matches!(node.node_type, NodeType::Block(_)) {
                continue;
            }
            let total_conns = self
                .connections
                .iter()
                .filter(|c| c.in_node == node.id || c.out_node == node.id)
                .count();
            if total_conns <= 1 {
                orphan_node_ids.push(node.id);
            }
        }
        if !orphan_node_ids.is_empty() {
            // 删除这些节点的全部连接
            self.connections.retain(|c| {
                !orphan_node_ids.contains(&c.in_node) && !orphan_node_ids.contains(&c.out_node)
            });
            // 删除节点本身
            self.nodes.retain(|n| !orphan_node_ids.contains(&n.id));
        }
    }
    // ====== 临时修复结束 ======

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

    /// Input/Output 孤点修复：若全部启用线已断，强制生成新连接到指定 block
    /// 目标：指定 block 中当前连接数（含 disabled）最少的节点
    fn repair_orphan_io(&mut self) {
        use super::block;
        let mut rng = rand::thread_rng();

        for node in &self.nodes {
            match node.node_type {
                NodeType::Input => {
                    let designated = block::sensory_block_for_input(node.id);
                    let has_enabled = self.connections.iter().any(|c| {
                        c.enabled
                            && c.in_node == node.id
                            && self
                                .nodes
                                .iter()
                                .any(|n| n.id == c.out_node && Self::node_block(n) == designated)
                    });
                    if has_enabled {
                        continue;
                    }
                    // 找到指定 block 中总连接数最少的节点
                    let designated_nodes: Vec<usize> = self
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(
                            |(_, n)| matches!(n.node_type, NodeType::Block(b) if b == designated),
                        )
                        .map(|(i, _)| i)
                        .collect();
                    // 按总连接数（含 disabled）排序，取最少
                    let best = designated_nodes.iter().min_by_key(|&&i| {
                        let id = self.nodes[i].id;
                        self.connections
                            .iter()
                            .filter(|c| c.in_node == id || c.out_node == id)
                            .count()
                    });
                    if let Some(&best_idx) = best {
                        let to_id = self.nodes[best_idx].id;
                        let exists = self
                            .connections
                            .iter()
                            .any(|c| c.in_node == node.id && c.out_node == to_id);
                        if !exists {
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
                            && self
                                .nodes
                                .iter()
                                .any(|n| n.id == c.in_node && Self::node_block(n) == designated)
                    });
                    if has_enabled {
                        continue;
                    }
                    let designated_nodes: Vec<usize> = self
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(
                            |(_, n)| matches!(n.node_type, NodeType::Block(b) if b == designated),
                        )
                        .map(|(i, _)| i)
                        .collect();
                    let best = designated_nodes.iter().min_by_key(|&&i| {
                        let id = self.nodes[i].id;
                        self.connections
                            .iter()
                            .filter(|c| c.in_node == id || c.out_node == id)
                            .count()
                    });
                    if let Some(&best_idx) = best {
                        let from_id = self.nodes[best_idx].id;
                        let exists = self
                            .connections
                            .iter()
                            .any(|c| c.in_node == from_id && c.out_node == node.id);
                        if !exists {
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
