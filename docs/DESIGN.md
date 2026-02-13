# Cell World - 神经网络涌现系统设计文档

## 一、项目概述

基于神经网络的二维生态演化模拟器，所有生物行为（集群、种群分化、捕食、合作等）都是自然进化的结果，而非预设编程。

### 技术栈

- **语言**：Rust
- **渲染**：egui + eframe

### 世界特性

- **无限世界**：没有边界，生物可自由移动
- **动态视窗**：支持拖拽平移、鼠标滚轮缩放（以鼠标位置为中心）
- **视窗内生成**：能量粒子仅在当前可见视窗范围内生成

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
1. 存在消耗：活着每秒消耗能量（基础消耗 + 百分比消耗）
2. 繁殖成本：繁殖时分走 45% 能量给子代
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

所有生物初始拥有：
- 输出[0] → 移动X
- 输出[1] → 移动Y
- 输出[2] → 吸收
- 输出[3] → 繁殖

释放(3)和能量转移(5)需要通过变异解锁。

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
}
```

### 变异类型

所有变异使用统一的 `mutation_rate`（默认 5%）：

| 变异 | 触发概率 | 说明 |
|------|----------|------|
| 权重变异 | 5% 每条连接 | 90% 微调 ±0.5，10% 重置 |
| 新增连接 | 5% | 随机连接两个节点 |
| 新增节点 | 5% | 拆分现有连接，插入隐藏节点 |
| 新增输出 | 5% | 解锁新功能（从功能池选择） |
| 开关连接 | 5% | 启用/禁用随机连接 |

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
    // 实体
    creatures: Vec<Creature>,
    energy_particles: Vec<EnergyParticle>,

    // 空间索引（加速邻居查询）
    creature_grid: SpatialGrid,
    energy_grid: SpatialGrid,

    // 视窗范围（世界坐标，用于能量生成）
    viewport_min_x: f64,
    viewport_min_y: f64,
    viewport_max_x: f64,
    viewport_max_y: f64,

    // 统计
    time: f64,
    family_stats: HashMap<usize, usize>,
    extinct_families: usize,
}

struct EnergyParticle {
    id: u64,
    x: f64,
    y: f64,
    energy: f64,
    lifetime: f64,
    age: f64,
    alive: bool,
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
条件：输出"繁殖" > 0.2 且 能量 > 繁殖阈值(35)
效果：
  - 创建子代，位置在父代附近 ±10 像素
  - 子代继承父代基因组（带变异，变异率 5%）
  - 子代获得父代 45% 能量
  - 子代继承父代 family_id
```

---

## 十、能量系统

### 消耗

| 行为 | 消耗 |
|------|------|
| 存在（基础代谢） | 0.1 / 秒 |
| 存在（百分比代谢） | 0.5% × 当前能量 / 秒 |
| 移动 | 0.2 × 距离 |
| 繁殖 | 分 45% 给子代 |
| 吸收/释放/转移 | 无额外消耗 |

> 百分比代谢防止高能量个体过于懒惰

### 来源

- 阳光：视窗内定期生成能量粒子（每 0.3 秒生成 3 个，每个 30 能量）
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
├── CLAUDE.md              # Claude Code 开发指南
├── docs/
│   └── DESIGN.md          # 本文档
└── src/
    ├── main.rs            # 入口
    ├── app.rs             # egui 应用
    ├── config.rs          # 配置参数
    ├── world/
    │   ├── mod.rs
    │   ├── world.rs       # 世界管理
    │   ├── creature.rs    # 生物
    │   ├── energy.rs      # 能量粒子
    │   └── spatial.rs     # 空间索引
    ├── neural/
    │   ├── mod.rs
    │   ├── network.rs     # 神经网络
    │   └── genome.rs      # 基因组 + NEAT 变异
    └── render/
        ├── mod.rs
        ├── canvas.rs      # 主画布（拖拽缩放）
        └── panel.rs       # 信息面板
```

---

## 十四、配置参数

```rust
pub struct Config {
    // 初始化
    pub initial_energy: f64,           // 60.0

    // 能量生成
    pub energy_spawn_interval: f64,    // 0.3 秒
    pub energy_spawn_count: usize,     // 3 个
    pub energy_particle_value: f64,    // 30.0
    pub energy_particle_lifetime: f64, // 25.0 秒

    // 代谢
    pub base_metabolism: f64,          // 0.1 / 秒
    pub percent_metabolism: f64,       // 0.005 (0.5% / 秒)
    pub move_cost: f64,                // 0.2 / 距离

    // 繁殖
    pub reproduce_threshold: f64,      // 35.0
    pub reproduce_energy_ratio: f64,   // 0.45 (45%)

    // 感知
    pub sense_range: f64,              // 50.0
    pub contact_range: f64,            // 8.0

    // 进化（统一变异率）
    pub mutation_rate: f64,            // 0.05 (5%)
    pub initial_connections_min: usize, // 3
    pub initial_connections_max: usize, // 6
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

### v0.1 - 基础框架 ✅
- [x] Rust 项目结构
- [x] egui 窗口和基本渲染
- [x] 世界、生物、能量粒子基础类

### v0.2 - 简单神经网络 ✅
- [x] 固定结构神经网络（17→隐藏→输出）
- [x] 基因组 = 节点 + 连接
- [x] 基础进化（权重变异）

### v0.3 - NEAT ✅
- [x] 结构进化（新增节点、连接）
- [x] 输出维度扩展
- [x] 功能池映射

### v0.4 - 完整系统 ✅
- [x] 所有功能池实现（6 个功能）
- [x] 交互规则完善
- [x] 统计和可视化
- [x] 无限世界 + 视窗缩放

### v1.0 - 优化
- [x] 空间索引（FxHashMap）
- [ ] 并行计算（rayon）
- [ ] 参数调优
- [ ] 长时间运行稳定性
