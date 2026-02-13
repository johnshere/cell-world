# Cell World - 神经网络涌现系统设计文档

## 一、项目概述

基于神经网络的二维生态演化模拟器，所有生物行为（集群、种群分化、捕食、合作等）都是自然进化的结果，而非预设编程。

### 技术栈

- **语言**：Rust
- **渲染**：egui + eframe
- **并行**：rayon（可选）

---

## 二、设计理念

### 核心原则

1. **最小预设**：只定义物理约束，不定义行为
2. **行为涌现**：所有复杂行为是进化的副产品
3. **开放进化**：神经网络结构本身可进化（NEAT）

### 什么是预设 vs 涌现

| 类型 | 示例 | 性质 |
|------|------|------|
| 物理约束 | 能量守恒、死亡条件 | 不可避免，类似自然法则 |
| 编程行为 | if 饥饿 then 找食物 | 应避免 |
| 涌现行为 | 集群、捕食、合作 | 进化的结果 |

---

## 三、物理约束（仅此三条）

```
1. 存在消耗：活着每秒消耗能量
2. 繁殖成本：繁殖时分走能量给子代
3. 死亡条件：能量 ≤ 0 → 死亡
```

---

## 四、功能池

生物通过神经网络输出控制的功能。初始时只有基础功能，其他功能通过进化"解锁"。

| 编号 | 功能 | 输出范围 | 说明 |
|------|------|---------|------|
| 0 | 移动X | -1 ~ +1 | 水平位移量 |
| 1 | 移动Y | -1 ~ +1 | 垂直位移量 |
| 2 | 吸收 | 0 ~ 1 | >0.5 时吸收当前位置能量 |
| 3 | 释放 | 0 ~ 1 | 释放自身能量到环境 |
| 4 | 繁殖 | 0 ~ 1 | >0.5 且能量足够时分裂 |
| 5 | 能量转移 | -1 ~ +1 | 负=给予邻居，正=掠夺邻居 |

### 初始状态

所有生物只有：
- 输出[0] → 移动X
- 输出[1] → 移动Y

其他功能需要通过变异获得。

---

## 五、神经网络设计

### 输入（17维）

```
8方向能量感知      [8个]  周围的阳光能量（归一化）
8方向邻居相似度    [8个]  0=无邻居，>0=有邻居且为平均相似度
自身能量           [1个]  归一化到 0~1
```

> 注：邻居存在信息已合并到相似度中，0 表示无邻居，减少了 8 维冗余输入。

### 输出（可变维度）

初始：2维 [移动X, 移动Y]

进化后可能扩展到 3~6 维，每个输出映射到功能池中的一个功能。

### 激活函数

- 隐藏层：ReLU 或 tanh
- 输出层：tanh（范围 -1 ~ +1）

---

## 六、NEAT 算法

### 结构进化

传统神经网络只进化权重，NEAT 同时进化：
- 权重
- 连接（可新增、可禁用）
- 节点（可新增）
- 输出维度（可扩展）

### 基因组结构

```rust
struct Genome {
    /// 节点基因
    nodes: Vec<NodeGene>,

    /// 连接基因
    connections: Vec<ConnectionGene>,

    /// 输出映射：output_map[i] = 功能编号
    output_map: Vec<usize>,
}

struct NodeGene {
    id: usize,
    node_type: NodeType,  // Input, Hidden, Output
}

struct ConnectionGene {
    in_node: usize,
    out_node: usize,
    weight: f64,
    enabled: bool,
    innovation: usize,  // 创新编号，用于交叉对齐
}
```

### 变异类型

| 变异 | 概率 | 说明 |
|------|------|------|
| 权重微调 | 80% | weight ± 0.2 |
| 权重重置 | 10% | 随机新值 |
| 新增连接 | 5% | 两个现有节点间 |
| 新增节点 | 3% | 在现有连接中插入 |
| 新增输出 | 1% | 扩展输出维度 |
| 改变映射 | 1% | 某输出改为其他功能 |

---

## 七、生物结构

```rust
struct Creature {
    // 位置
    x: f64,
    y: f64,

    // 状态
    energy: f64,
    age: f64,
    alive: bool,

    // 遗传
    genome: Genome,
    brain: Network,  // 从 Genome 构建
    family_id: usize,

    // 缓存
    genome_hash: u64,  // 用于快速比较
}
```

---

## 八、世界结构

```rust
struct World {
    // 尺寸
    width: f64,
    height: f64,

    // 实体
    creatures: Vec<Creature>,
    energy_particles: Vec<EnergyParticle>,

    // 空间索引（加速邻居查询）
    grid: SpatialGrid,

    // 统计
    time: f64,
    family_stats: HashMap<usize, usize>,
}

struct EnergyParticle {
    x: f64,
    y: f64,
    energy: f64,
    lifetime: f64,
}
```

---

## 九、交互规则

### 吸收环境能量

```
条件：当前位置有能量粒子 且 输出"吸收" > 0.5
效果：获得能量，粒子消失
```

### 释放能量

```
条件：输出"释放" > 0
效果：释放 (输出值 × 系数) 能量到当前位置，生成能量粒子
```

### 邻居能量转移

```
条件：与邻居接触（距离 < 阈值）
规则：
  - 我方输出 +0.8（想掠夺），对方 -0.3（想给予）
    → 能量从对方流向我方
  - 双方都想掠夺
    → 输出值更大的一方获得能量
  - 双方都想给予
    → 互相给予（能量交换）
```

### 繁殖

```
条件：输出"繁殖" > 0.5 且 能量 > 繁殖阈值
效果：
  - 创建子代，位置在父代附近
  - 子代继承父代基因组（带变异）
  - 子代获得父代 50% 能量
  - 子代继承父代 family_id
```

---

## 十、能量系统

### 消耗

| 行为 | 消耗 |
|------|------|
| 存在（基础代谢） | 0.1 / 秒 |
| 移动 | 0.5 × 距离 |
| 繁殖 | 分 50% 给子代 |
| 吸收/释放/转移 | 无额外消耗 |

### 来源

- 阳光：世界定期生成能量粒子
- 掠夺：从其他生物获取

---

## 十一、感知系统

### 八方向定义

```
  7   0   1
   \  |  /
    \ | /
  6 --+-- 2
    / | \
   /  |  \
  5   4   3
```

### 感知范围

以生物为中心，感知半径内的：
- 能量粒子总量
- 邻居存在与否
- 邻居基因相似度（余弦相似度）

### 基因相似度计算

```rust
fn similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    (dot / (norm_a * norm_b) + 1.0) / 2.0  // 映射到 0~1
}
```

---

## 十二、渲染

### 使用 egui + eframe

- 主画布：显示世界、生物、能量粒子
- 侧边栏：统计信息、参数调节
- 控制：暂停、加速、重置

### 生物颜色

```rust
fn creature_color(creature: &Creature) -> Color32 {
    let hue = (creature.genome_hash % 360) as f32;  // 基因决定色相
    let saturation = 0.3 + (creature.age / 10000.0).min(1.0) * 0.7;  // 年龄决定饱和度
    let lightness = 0.2 + (creature.energy / 200.0).min(1.0) * 0.5;  // 能量决定亮度
    hsv_to_rgb(hue, saturation, lightness)
}
```

---

## 十三、项目结构

```
cell-world/
├── Cargo.toml
├── README.md
├── docs/
│   └── DESIGN.md          # 本文档
└── src/
    ├── main.rs            # 入口
    ├── app.rs             # egui 应用
    ├── world/
    │   ├── mod.rs
    │   ├── world.rs       # 世界管理
    │   ├── creature.rs    # 生物
    │   ├── energy.rs      # 能量粒子
    │   └── spatial.rs     # 空间索引
    ├── neural/
    │   ├── mod.rs
    │   ├── network.rs     # 神经网络
    │   ├── genome.rs      # 基因组
    │   └── neat.rs        # NEAT 算法
    ├── render/
    │   ├── mod.rs
    │   ├── canvas.rs      # 主画布
    │   └── panel.rs       # 信息面板
    └── config.rs          # 配置参数
```

---

## 十四、配置参数

```rust
pub struct Config {
    // 世界
    pub world_width: f64,
    pub world_height: f64,

    // 初始化
    pub initial_creatures: usize,
    pub initial_energy: f64,

    // 能量
    pub energy_spawn_interval: f64,
    pub energy_spawn_count: usize,
    pub energy_particle_value: f64,
    pub energy_particle_lifetime: f64,

    // 生物
    pub base_metabolism: f64,
    pub move_cost: f64,
    pub reproduce_threshold: f64,
    pub reproduce_energy_ratio: f64,

    // 感知
    pub sense_range: f64,
    pub contact_range: f64,

    // 进化
    pub mutation_rate: f64,
    pub weight_mutation_range: f64,
}
```

---

## 十五、预期进化路径

```
第 1 代：随机移动，靠运气碰到阳光

第 N 代：某些网络变异出"吸收"功能
        → 能主动吸收的存活率更高

第 M 代：某些网络变异出"繁殖"功能
        → 能主动繁殖的家族扩张

第 K 代：某些网络利用"基因相似度"输入
        → 对同类和异类有不同行为
        → 种群分化开始

更后期：掠夺、给予、集群、领地...
        → 复杂生态涌现
```

---

## 十六、里程碑

### v0.1 - 基础框架
- [ ] Rust 项目结构
- [ ] egui 窗口和基本渲染
- [ ] 世界、生物、能量粒子基础类

### v0.2 - 简单神经网络
- [ ] 固定结构神经网络（17→8→2）
- [ ] 基因组 = 权重
- [ ] 基础进化（权重变异）

### v0.3 - NEAT
- [ ] 结构进化（新增节点、连接）
- [ ] 输出维度扩展
- [ ] 功能池映射

### v0.4 - 完整系统
- [ ] 所有功能池实现
- [ ] 交互规则完善
- [ ] 统计和可视化

### v1.0 - 优化
- [ ] 性能优化（空间索引、并行）
- [ ] 参数调优
- [ ] 长时间运行稳定性
