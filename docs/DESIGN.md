# Cell World - 神经网络涌现系统设计文档

## 一、项目概述

基于神经网络的二维生态演化模拟器，所有生物行为（集群、种群分化、捕食、合作等）都是自然进化的结果，而非预设编程。

### 技术栈

- **语言**：Rust
- **渲染**：egui + eframe
- **哈希**：FxHashMap（rustc-hash）
- **序列化**：serde + serde_json（可选 persistence feature）

### 世界特性

- **无限世界**：没有边界，生物可自由移动
- **动态视窗**：支持拖拽平移、鼠标滚轮缩放（以鼠标位置为中心）
- **火山能量源**：火山定期喷发，陨石随机降落，粒子随时间衰减

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
1. 存在消耗：基础代谢 + 年龄倍率消耗 + 体温逸散
2. 繁殖成本：繁殖时分走 30% 能量给子代
3. 死亡条件：能量 ≤ 0 → 死亡
```

---

## 四、神经网络设计

### 3眼感知系统

生物拥有三只眼睛，分别朝向不同方向：

```
左眼(-45°)    中眼(0°)    右眼(+45°)
    \          |          /
     \  FOV   |   FOV   /  （半角 = 30°）
      \30°    |   30°  /
       \      |      /
        \     |     /
         \    |    /
          \   |   /
           \  |  /  ← 视觉半径（150单位）
            \ | /
             \|/
              ○ (生物)
```

- **左眼**：朝向 heading - 45°，FOV 半角 30°
- **中眼**：朝向 heading，FOV 半角 30°
- **右眼**：朝向 heading + 45°，FOV 半角 30°
- **视觉半径**：固定 150 单位
- **距离编码**：接近度 = `1.0 - dist / vision_range`（越近越大）

### 输入（11维）

```
[0-2]   左眼: 食物距离, 同族距离, 异族距离
[3-5]   中眼: 食物距离, 同族距离, 异族距离
[6-8]   右眼: 食物距离, 同族距离, 异族距离
[9]     自身能量 (0~1, energy / 200)
[10]    体温状态 (0~1, 冷却时长 / 100，越冷越高)
```

每只眼跟踪最近的食物/同族/异族目标，值为接近度（0=无目标，1=紧贴）。

### 输出（4维，固定）

| 编号 | 功能 | 输出范围 | 说明 |
|------|------|---------|------|
| 0 | 转向角 | tanh(-1~1) | × π/2 = 每秒转向弧度 |
| 1 | 速度 | tanh(-1~1) | abs × 50 = 每秒移动距离 |
| 2 | 嘴 | tanh(-1~1) | <-0.1 咬（捕食）；>+0.1 喂（哺育）；接触食物自动吸收 |
| 3 | 繁殖 | tanh(-1~1) | >0.2 且能量≥阈值时触发 |

### 激活函数

- 隐藏层 + 输出层：tanh（范围 -1 ~ +1）

---

## 五、NEAT 算法

### 结构进化

传统神经网络只进化权重，NEAT 同时进化：
- 权重（微调或重置）
- 连接（可新增、可禁用）
- 节点（可新增隐藏层）

### 基因组结构

```rust
struct Genome {
    nodes: Vec<NodeGene>,        // 节点基因
    connections: Vec<ConnectionGene>, // 连接基因
    next_node_id: usize,
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

所有变异使用统一的 `mutation_rate`（默认 15%）：

| 变异 | 触发概率 | 说明 |
|------|----------|------|
| 权重变异 | 15% 每条连接 | 90% 微调 ±0.5，10% 重置 [-1,1] |
| 新增连接 | 15% | 随机连接两个节点 |
| 新增节点 | 15% | 拆分现有连接，插入隐藏节点 |
| 开关连接 | 15% | 启用/禁用随机连接 |

### 繁殖方式

- **无性繁殖**：基因组带变异复制
- **有性繁殖**：寻找接触范围内同种配偶（相似度≥阈值），`crossover()` 权重平均取交集

### 相似度计算

使用排序归并算法，零分配 O(n log n)：

```rust
fn similarity(&self, other: &Genome) -> f64 {
    // 提取启用连接的 (in_node, out_node, weight)
    // 排序后双指针归并
    // 匹配连接：1.0 - |w1-w2|/4.0
    // 不匹配连接：0分
    // 返回 similarity_sum / total
}
```

相似度 ≥ 0.9 视为同一种族（同类）。

---

## 六、生物结构

```rust
struct Creature {
    id: u64,
    x: f64, y: f64,              // 位置
    energy: f64,                  // 能量
    age: f64,                     // 年龄（秒）
    alive: bool,
    heading: f64,                 // 朝向（弧度）

    genome: Genome,               // 基因组
    brain: Network,               // 神经网络（从 Genome 构建）
    generation: usize,            // 世代数
    parent_id: Option<u64>,       // 父代ID（None = 自然生成）

    genome_hash: u64,             // 基因哈希（快速比较）
    last_warm_time: f64,          // 上次感温时间
    perception_cache: [f64; 11],  // 感知缓存
}
```

---

## 七、世界结构

```rust
struct World {
    creatures: Vec<Creature>,
    energy_particles: Vec<EnergyParticle>,

    creature_grid: SpatialGrid,   // 生物空间索引
    energy_grid: SpatialGrid,     // 粒子空间索引

    time: f64,                    // 世界时间

    // 能量生成计时器
    volcano_timer: f64,
    meteorite_timer: f64,

    // ID 计数器
    next_creature_id: u64,
    next_energy_id: u64,

    // 视窗范围
    viewport_min_x: f64, viewport_min_y: f64,
    viewport_max_x: f64, viewport_max_y: f64,

    // 缓存
    similarity_cache: FxHashMap<(u64, u64), f64>,  // 相似度缓存
    clan_cache: Option<ClanCache>,                  // 种族聚类缓存（每秒更新）

    // 统计
    action_counts: [usize; 5],     // 行为计数：移动/吸收/咬/喂/繁殖
    death_ages: Vec<f64>,          // 死亡年龄记录
    death_age_stats: DeathAgeStats,
}

struct EnergyParticle {
    id: u64,
    x: f64, y: f64,
    energy: f64,
    initial_energy: f64,   // 初始能量（用于透明度计算）
    lifetime: f64,         // 存活时间上限
    age: f64,
    alive: bool,
}
```

---

## 八、交互规则

### 吸收环境能量

```
条件：接触能量粒子（距离 < contact_range）
效果：自动吸收，获得粒子剩余能量，粒子消失
```

### 咬（捕食）

```
条件：嘴输出 < -0.1，且接触到其他生物
效果：转移目标 20% 能量 × 相似度补偿（异类更好咬）
```

### 喂（哺育）

```
条件：嘴输出 > +0.1，且接触到其他生物
效果：转移自身 20% 能量给目标
      体型差 > feed_size_ratio_threshold 时无损耗
```

### 繁殖

```
条件：繁殖输出 > 0.2 且 能量 ≥ reproduce_threshold (60)
效果：
  - 优先有性繁殖（寻找接触范围内同种配偶，crossover + 变异）
  - 无配偶时无性繁殖（变异复制）
  - 子代位置在父代附近
  - 子代获得父代 30% 能量
  - 子代 generation = 父代 + 1
  - 子代 parent_id = 父代 id
```

---

## 九、能量系统

### 消耗

| 行为 | 消耗公式 |
|------|----------|
| 基础代谢 | 0.07 / 秒 |
| 年龄倍率 | base × (1 + age × 0.04)，年龄越大消耗越高 |
| 移动 | 0.001 × 距离 |
| 体温逸散 | 系数 × 冷却时长 × 体表面积 / 秒 |
| 繁殖 | 分 30% 能量给子代 |

### 来源

#### 火山喷发
- 位置：原点 (0, 0)
- 间隔：30 秒
- 每次：60 个粒子，每个 30 能量
- 分布：三角分布，半径 600 内（内密外疏）

#### 陨石降落
- 位置：视窗范围内随机
- 间隔：12 秒
- 每次：18 个粒子，每个 40 能量
- 分布：沿随机方向线段散布（长度 180）

#### 粒子衰减
- 粒子能量随时间衰减：`energy *= (1 - 0.005)` 每秒
- 粒子有生命周期上限，超时消失

---

## 十、种族系统

### 祖先追溯聚类

不使用固定 family_id，而是动态计算种族归属：

1. 沿 `parent_id` 向上追溯，找到最老的活祖先（族长）
2. 检查与族长的基因相似度 ≥ 0.9
3. 满足则归入该族群，否则自立门户

### 优势种检测

满足以下条件时标记为优势种：
- 种群人口占比 ≥ 30%
- 族长年龄 ≥ `dominant_min_age`（500秒）
- 平均年龄 ≥ 全局死亡年龄中位数

优势种自动保存到 `store/` 目录。

---

## 十一、渲染

### 使用 egui + eframe

- 主画布：显示世界、生物、能量粒子、火山标记
- 侧边栏：统计信息、速度控制、模板管理
- 控制：暂停、加速/减速

### 生物渲染

```rust
fn creature_color(creature: &Creature, clan_leader_hash: Option<u64>) -> Color32 {
    let hue = (hash % 360) as f32;      // 基因/族长决定色相
    let saturation = ...;                 // 年龄决定饱和度
    let lightness = ...;                  // 能量决定亮度
}
```

- 大小：∝ √能量
- 朝向：五分之一圆弧标识
- 选中：白色高亮
- 同族生物共享族长色相

### 侧边栏面板

- 速度控制：⏪ 减速 / 滑块 / ⏩ 加速 / ▶ 暂停
- 统计：生物数、粒子数、总能量、平均能量、世代数、种群数
- 火山倒计时
- 行为计数：移动/吸收/咬/喂/繁殖
- 寿命统计：中位数/均值/最大/最小
- 种群排行：Top种族及占比
- 优势种候选
- 生物模板管理：选择/添加/删除

---

## 十二、存储系统

### 生物模板（store/）

```rust
struct CreatureTemplate {
    name: String,
    genome: Genome,
    initial_energy: f64,
    version: Option<String>,
    score: Option<f64>,
    population_ratio: Option<f64>,  // 保存时的人口占比
    avg_energy: Option<f64>,
    avg_age: Option<f64>,
    max_generation: Option<usize>,
    recorded_at: Option<f64>,       // 保存时的世界时间
    auto_recorded: Option<bool>,    // 是否自动保存的优势种
}
```

- 存储目录：`store/`
- 格式：JSON
- 自动保存：优势种检测到时自动存档
- 手动操作：通过侧边栏选择模板，点击 "添加" 投放到世界

---

## 十三、项目结构

```
cell-world/
├── Cargo.toml
├── CLAUDE.md              # Claude Code 开发指南
├── docs/
│   ├── DESIGN.md          # 本文档
│   ├── LOG.md             # 统计日志（自动生成）
│   └── run.log            # 性能日志（自动生成）
├── store/                 # 生物模板存档（JSON）
└── src/
    ├── main.rs            # 入口
    ├── app.rs             # egui 应用主循环、日志、选中
    ├── config.rs          # 配置参数
    ├── store.rs           # 生物模板存储
    ├── world/
    │   ├── mod.rs
    │   ├── world.rs       # 世界管理、感知、动作执行
    │   ├── creature.rs    # 生物结构
    │   ├── energy.rs      # 能量粒子
    │   └── spatial.rs     # 空间索引（FxHashMap 网格）
    ├── neural/
    │   ├── mod.rs
    │   ├── network.rs     # 神经网络前向传播
    │   └── genome.rs      # 基因组 + NEAT 变异
    └── render/
        ├── mod.rs
        ├── canvas.rs      # 主画布（拖拽缩放、实体渲染）
        └── panel.rs       # 侧边栏统计面板
```

---

## 十四、配置参数

```rust
pub struct Config {
    // 速度与初始化
    pub initial_speed: f64,            // 2.0 (初始倍速)
    pub initial_scale: f32,            // 0.6 (世界缩放)
    pub min_creatures: usize,          // 20 (最小生物数)
    pub initial_energy: f64,           // 50.0

    // 火山
    pub volcano_x: f64,                // 0.0
    pub volcano_y: f64,                // 0.0
    pub volcano_interval: f64,         // 30.0 秒
    pub volcano_radius: f64,           // 600.0
    pub volcano_count: usize,          // 60 粒子/次
    pub volcano_particle_energy: f64,  // 30.0

    // 陨石
    pub meteorite_interval: f64,       // 12.0 秒
    pub meteorite_count: usize,        // 18 粒子/次
    pub meteorite_length: f64,         // 180.0 散布长度
    pub meteorite_particle_energy: f64, // 40.0
    pub particle_decay_rate: f64,      // 0.005

    // 代谢
    pub base_metabolism: f64,          // 0.07 / 秒
    pub age_metabolism_factor: f64,    // 0.04 (age × 此值 = 倍率)
    pub move_cost: f64,                // 0.001 / 距离
    pub heat_dissipation_coefficient: f64, // 0.002
    pub feed_size_ratio_threshold: f64,    // 1.5

    // 繁殖
    pub reproduce_threshold: f64,      // 60.0
    pub reproduce_energy_ratio: f64,   // 0.3 (30%)

    // 感知
    pub vision_range: f64,             // 150.0
    pub contact_range: f64,            // 15.0

    // 进化
    pub mutation_rate: f64,            // 0.15 (15%)
    pub initial_connections_min: usize, // 6
    pub initial_connections_max: usize, // 12
    pub species_similarity_threshold: f64, // 0.9

    // 优势种
    pub dominant_min_age: f64,         // 500.0 秒
}
```

---

## 十五、性能优化

| 优化 | 位置 | 效果 |
|------|------|------|
| FxHashMap | 全项目 | 比标准 HashMap 快 2-3 倍 |
| 空间索引网格 | `spatial.rs` | O(1) 邻居查询 |
| 相似度缓存 | `world.rs` | 避免重复计算基因相似度 |
| 种族缓存 | `world.rs` | 每秒更新一次聚类结果 |
| 拓扑排序 | `network.rs` | 一次性排序，前向传播线性评估 |
| 零分配相似度 | `genome.rs` | 排序归并算法，无 HashMap 分配 |
| 缓冲区复用 | `world.rs` | 空间查询复用 Vec |
| 渲染上下文缓存 | `app.rs` | 1 秒更新一次族群颜色 |
| Release 优化 | `Cargo.toml` | LTO + opt-level=3 |

---

## 十六、日志系统

### 统计日志（docs/LOG.md）

每 10 秒输出一行：

```
| 时间 | 生物 | 粒子 | 总能 | 代 | 种群 | 寿命(中/均/大/小/数) | 行为(移/吸/咬/喂/殖) |
```

### 性能日志（docs/run.log）

```
| 时间 | FPS | 生物 | 世界ms | 面板ms | 聚类ms | 渲染ms | egui | 帧总ms |
```

---

## 十七、预期进化路径

```
第 1 代：随机移动，靠运气碰到能量粒子

第 N 代：网络变异出更有效的转向/速度组合
        → 能朝食物方向移动的存活率更高

第 M 代：繁殖信号开始被利用
        → 能主动繁殖的家族扩张

第 K 代：利用同族/异族距离输入
        → 对同类和异类有不同行为
        → 种群分化开始

更后期：咬（捕食）、喂（哺育）、集群、领地...
        → 复杂生态涌现
```

---

## 十八、里程碑

### v0.1 - 基础框架 ✅
- [x] Rust 项目结构
- [x] egui 窗口和基本渲染
- [x] 世界、生物、能量粒子基础类

### v0.2 - 神经网络 + NEAT ✅
- [x] NEAT 基因组（节点 + 连接）
- [x] 结构进化（新增节点、连接、权重变异）
- [x] 3眼感知系统（11维输入）
- [x] 4输出动作（转向、速度、嘴、繁殖）
- [x] 有性/无性繁殖
- [x] 火山 + 陨石能量系统
- [x] 祖先追溯种族聚类
- [x] 优势种自动保存
- [x] 空间索引（FxHashMap 网格）
- [x] 相似度/聚类缓存
- [x] 侧边栏统计面板
- [x] 生物模板存储系统
- [x] 日志系统（统计 + 性能）
- [x] 性能优化（零分配、缓冲区复用、LTO）

### 未来方向
- [ ] 并行计算（rayon）
- [ ] 参数调优与进化实验
- [ ] 长时间运行稳定性验证
- [ ] 可视化增强（进化树、基因拓扑）
