# Cell World v1.0 - 神经网络涌现系统设计文档

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
- **正弦周期调制**：火山/陨石的间隔和能量均随正弦函数波动，模拟季节性变化
- **环境温度**：火山热（三次方衰减）+ 集体热（附近生物能量贡献），推动集群涌现
- **痕迹点系统**：移动消耗转化为痕迹（能量守恒），可被吸收
- **海底火山地形**：可由用户一次性生成的 fBm 噪声高度图（50×50 chunk，火山圆锥+多倍频噪声+种子），影响生物移动消耗（坡度+海拔阻力），生成后冻结
- **时间模型**：固定步长 `dt = SIM_DT = 1/30` 模拟秒；加速通过每帧多次 `world.update` 实现；SNN 每次 update 固定 10 ticks（`neural_tick_rate = 300`，`300 × 1/30 = 10`）
- **加速语义主旨**：**加速只是更快获得结果，不影响结果**。任意倍速 v 下，给定 (S₀, config)，K 次 update 后的状态 S_K 与 v 二进制一致
- **三线程架构**：
  - UI 主线程：egui 渲染、面板、输入
  - sim 线程：`world.update`（感知并行 + 动作串行）、快照导出，墙钟节拍 1/30s
  - neural 线程（仅 bridge 模式）：SNN 批处理，由 sim 请求驱动，无独立节奏
- **神经后端（两种都保持主旨）**：
  - **Legacy**（`neural_backend="legacy"`）：world.update 直接调用 `brain.tick_multi(perception, 10)`，CPU SpikingNetwork 原子推进
  - **Bridge**（`auto`/`cpu`/`gpu`）：world 每次 update 向 neural 线程发送 `TickRequest { events, inputs, tick_count=10 }` 并**同步阻塞等** `TickResponse`。neural 线程 `req_rx.recv()` 阻塞驱动，不按墙钟
- **GPU 批内单次 readback（C 方案，bridge+gpu 专属优化）**：
  - Shader 新增 binding：`4=spike_counts (atomic<u32>)`、`5=first_outputs (f32)`、`6=tick_params uniform`
  - 批开始：清零 spike_counts + first_outputs GPU 缓冲
  - 批内 10 个 dispatch：tick 0 写 first_outputs，每次 fire 时 `atomicAdd(&spike_counts[out])`
  - 批末尾：一次 `copy_buffer_to_buffer` 把两段连续拷贝到 staging，`map_async` + `Maintain::Wait` 一次拉回
  - 相比历史 per-tick readback + spin_loop，CPU↔GPU 同步开销降低 10×
- **面板速度**：同步批处理下只有单值"FPS: X | 速度: Nx"；历史上的"神经/世界"双值和反压机制均已删除

---

## 二、设计理念

### 核心原则

1. **真实规则**：引入痕迹点、环境温度+集体热，使物理规则更贴近真实生态
2. **行为涌现**：所有复杂行为是进化的副产品
3. **温度驱动集群**：集体热效应使生物聚集→温度升高→散热降低→存活率提升

### 什么是预设 vs 涌现

| 类型 | 示例 | 性质 |
|------|------|------|
| 物理约束 | 能量守恒、死亡条件、体温逸散 | 不可避免，类似自然法则 |
| 编程行为 | if 饥饿 then 找食物 | 应避免 |
| 涌现行为 | 集群、捕食、合作、寄生 | 进化的结果 |

---

## 三、物理约束

```
1. 存在消耗：基础代谢 × 年龄倍率 + 体温逸散（与环境温度相关）
2. 繁殖成本：繁殖时分走 10%~50% 能量给子代（神经网络控制）
3. 死亡条件：能量 ≤ 0 → 死亡
4. 能量守恒：移动消耗 → 痕迹点能量
5. 落地杀伤：火山/陨石粒子下落时砸死附近生物
6. 地形阻力（仅当地形已生成）：移动消耗 ×= slope_factor × altitude_factor
   - slope_factor = 1 + max(dh/distance, 0) × terrain_slope_cost  （上坡加倍开销，下坡不补贴）
   - altitude_factor = 1 + |h - comfort_h| / range × terrain_altitude_cost  （远离舒适带加倍开销）
   - comfort_h 在地形生成时预计算：取 min(volcano_radius, clamp_dist) 的 2/3 处线性圆锥高度
   - 地形未生成或所在 chunk 无数据时因子 = 1.0
```

### 地形系统（海底火山，fBm 噪声）

- **网格**：与渲染网格共用常量 `GRID_WORLD_SIZE = 50.0`，每个 50×50 chunk 一个整数高度
- **覆盖范围**：以原点为中心、`volcano_radius + 一格` 的圆，确保边界平滑
- **高度算法**（`src/world/terrain.rs::chunk_terrain_height`）：
  - **火山圆锥基底**：`base_height - dist / base_falloff`（clamp 到 8~base_height），定大致地势
  - **fBm 多倍频噪声**：`fbm(x/scale, y/scale, octaves, seed) × fbm_amp`，提供自然不规则起伏
  - 内部 `fbm` 由 `value_noise` (4 角双线性 + smoothstep) 多次采样累加而成（lacunarity=2, persistence=0.5），手写无外部依赖
  - 不再有环形/放射状结构，地形完全由噪声塑形
- **可调参数**（`TerrainParams`）：`base_height`、`base_falloff`、`fbm_scale`、`fbm_amp`、`fbm_octaves`、`seed`
- **种子化**：所有随机来自单个 `u32 seed`，相同参数 + 相同 seed → 完全确定性的地形
- **生成时机**：用户点击侧边栏 "⛰" 按钮触发（二次确认），按当前 `config.volcano_radius` 一次性生成；生成后**永久冻结**
- **半径变更解耦**：`TerrainMap` 内保存 `generated_radius` 快照；后续 UI 调整 `volcano_radius` 不影响已生成地形；新扩展区域 `terrain_factor = 1.0`
- **持久化**：地形独立保存为 `terrain.json`，与 `snapshot.json` 解耦。⛰ 生成时立即写盘；启动时无条件加载——无论用户在"发现存档"对话框中选"恢复"或"新游戏"，地形都自动复用上一次的
- **渲染**：`canvas.rs::draw_terrain` 在背景之后、网格之前绘制；颜色按高度归一化，深蓝→青蓝→暖橙的"海底→火山口"渐变（alpha≈110）

---

## 四、神经网络设计

### 感知系统（扫描眼）

```
       左眼FOV(140°)    右眼FOV(140°)
       heading+20°      heading-20°
            \              /
             \            /
              ○ (生物)
```

- **双眼窄波束扫描**：每只眼在 140° FOV 内逐帧扫描一条窄波束
  - 扫描速度 `eye_scan_speed = 280°/s`
  - 左眼起点 `heading + 20°`，逆时针扫；右眼起点 `heading - 20°`，顺时针扫
  - 探测距离 = `vision_range`，从生物身体中心起算
- **波束目标类型**（最近一个）：
  - 能量粒子 → `type = 0.33`
  - 痕迹点（非自身）→ `type = 0.67`
  - 其他生物 → `type = 1.0`
- **全 FOV 能量密度**：对每只眼整个 140° 内所有粒子+痕迹+生物按 `Σ energy / dist²` 累加（与散热公式中 `nearby_energy` 含义一致）
- **基因相似度**：仅当波束最近目标是生物时才计算 similarity，其他类型时复用为"是否同族"标志（痕迹0/1，能量粒子=0）
- **朝向差 / 速度差**：仅当最近目标是生物时有值，否则为 0
- **自身状态**：1 通道，每帧都更新

### 输入（18 维）

```
左眼 [0..7]:
  [0] 扫描角度归一化  (-1 ~ 1)
  [1] 目标接近度       body_radius / (body_radius + dist)
  [2] 目标能量         (energy / 200).min(1)
  [3] 实体类型         {0, 0.33, 0.67, 1.0}
  [4] 同族度/相似度    生物→similarity；痕迹→is_ally(0/1)；粒子→0
  [5] FOV 能量密度     (Σ energy/dist² / energy_denominator).min(1)
  [6] 目标朝向差       仅生物：angle_diff / π；否则 0
  [7] 目标速度差       仅生物：(Δspeed / max_speed).clamp(-1,1)；否则 0

右眼 [8..15]:
  [8..15] 同左眼，结构对称

自身状态 [16]:
  [16] 自身能量        (energy / 2000).min(1)

地形感知 [17]:
  [17] 前方坡度方向    dh.signum()  {-1=下坡, 0=平地/无地形, 1=上坡}
       前瞻 15px，归入 block 0（体感区）
       地形未生成时恒为 0
```

> 常量来源：`Genome::INPUT_SIZE = 18`，写入逻辑见 `src/world/world.rs::compute_perception_pure`。

### 输出（7 维，固定，全部直读 tanh）

| 编号 | 功能         | 输出范围   | 说明 |
|------|--------------|-----------|------|
| 0    | 转向角       | tanh -1~1 | × 2π = 每秒转向弧度（最大每秒一圈） |
| 1    | 速度         | tanh -1~1 | abs × `max_speed` = 每秒移动距离 |
| 2    | 嘴           | tanh -1~1 | < -0.1 咬（攻击），冷却 1s；接触食物自动吸收（不受冷却限制） |
| 3    | 繁殖意愿     | tanh -1~1 | > 0.2 时触发繁殖，冷却 10s |
| 4    | 繁殖阈值     | tanh→sig  | 映射到 20~200 能量阈值 |
| 5    | 子代能量比例 | tanh→sig  | 映射到 0.1~0.5 |
| 6    | 痕迹强度     | tanh -1~1 | 取正半轴 → 0~0.3，作为自身能量比例额外投放痕迹 |

> 常量来源：`Genome::OUTPUT_SIZE = 7`，执行逻辑见 `src/world/world.rs::execute_actions`。
> 喂食机制已移除，能量分享通过痕迹点实现。

### 激活函数

- SNN 节点：膜电位 + 阈值发放（直读模式取膜电位 tanh）

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
    nodes: Vec<NodeGene>,
    connections: Vec<ConnectionGene>,
    next_node_id: usize,
}
```

### 变异率（v2.5 后为全局常量）

变异率由 `config.mutation_rate` 全局控制（默认 0.15），**不再是基因组内可演化基因**。
历史上的 `MutationGene { base, block }` 双速率自适应机制已删除——稳定环境下它必然塌到下界，反而成为演化停滞的放大器。base/block 两类变异目前共享同一个全局 rate。

### 变异类型（触发概率均为 `config.mutation_rate`）

| 变异 | 类别 | 说明 |
|------|------|------|
| 权重变异 | base（每条连接） | 90% 微调 ±0.5，10% 重置 [-1,1] |
| 新增连接 | base | 随机连接两个节点（受 conn_probs 加权） |
| 新增节点 | base | 拆分现有连接，插入隐藏节点 |
| 开关连接 | base | 启用/禁用随机连接 |
| SNN 参数 | base | decay / threshold / refractory_period 抖动 |
| Layer 切换 | base | Block 节点 Processing ↔ Output |
| Learning/Reward | base | 学习/奖励基因抖动 |
| Block 迁移 | block | 联合区节点 block 编号 ±2 移动（限制在 8~24） |
| ConnProbs | block | 区块连接概率基因抖动 |

### Crossover（v2.5 改为全原子孟德尔遗传）

所有连续参数按"原子"（不可分割的功能单元）从某一父代整取，**crossover 不做算术平均**。
原因：算术平均是方差收缩算子，在稳定环境 + 选择压力下加速种群同质化；孟德尔遗传保持方差、保持多峰。

| 原子 | 粒度 | 规则 |
|------|------|------|
| 共享连接 `ConnectionGene` | 整条（weight/enabled 一体） | 50/50 从一方继承 |
| 共享节点 `NodeGene` | 整个 node（SNN 参数+layer 一体） | 50/50 从一方继承 |
| 每个 block 的 `ConnProbsGene` | 整块（proc/out/target_pref 一体） | 50/50 从一方继承 |
| `LearningGene` | 整个 struct | 50/50 从一方继承 |
| `RewardGene` | 整个 struct | 50/50 从一方继承 |
| fitter 独有连接/节点 | — | 标准 NEAT excess/disjoint，继承 fitter |
| weaker 独有节点 | — | 保留（避免基因流失） |

**分工**：crossover 只做重组（零方差贡献），mutation 是唯一的方差注入源。职责单一、行为可预测。

### 交配阈值（v2.5 与聚类阈值解耦）

`find_mate` 的相似度下限 = `species_similarity_threshold × 0.9`（而不是聚类阈值本身）。

- 聚类阈值 0.95：严格，用于面板上种族计数与颜色标识
- 交配阈值 0.855：宽松，允许跨 clan 基因流

目的是打破单一优势种垄断后无法注入新基因的死锁：
稀有变异个体相似度 <0.95 无法被计入主族，但 >0.855 时仍可与主族交配，让有益变异有机会回流基因池。

---

## 六、痕迹点系统

### 数据结构

```rust
struct TrailPoint {
    x: f64, y: f64,
    energy: f64,           // 移动消耗转化的能量（守恒）
    initial_energy: f64,
    genome_hash: u64,      // 创建者基因哈希（种族标识）
    age: f64,
    alive: bool,
}
```

### 生命周期

1. **生成**：每隔 0.25s 在生物位置生成一个痕迹点，能量来自移动消耗
   - 痕迹抑制：附近 10px 内有其他生物痕迹时不生成
2. **衰减**：`energy *= (1 - trail_decay_rate × dt)`，衰减率 0.12/s
3. **吸收**：生物接触范围内自动吸收
4. **死亡**：能量 < 0.05 时消失

---

## 七、体温逸散（指数衰减 + floor）

旧的"火山热 + 集体热 + env_temp"模型已替换为更简洁的**反距离能量密度**模型，核心是 `nearby_energy`。

### 周围能量

```
nearby_energy = Σ energy(particle, in vision_range)
              + Σ energy(creature,  in vision_range)
```

### 散热公式

```
body_radius   = (energy × 1.28)^(1/3)
circumference = body_radius × 2π

heat_factor = heat_floor
            + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)

heat_cost = heat_dissipation_coefficient × circumference × heat_factor × dt
```

- `heat_factor` ∈ `[heat_floor, 1.0]`
- 荒野（`nearby_energy = 0`）：`heat_factor = 1.0`，散热最大
- 能量丰富区：`heat_factor → heat_floor`，散热最小但有上界
- 小体型生物周长/能量比更大，散热更快
- **集群涌现核心**：聚集 → `nearby_energy ↑` → `heat_factor ↓` → 存活率提升

### 全 FOV 能量密度（眼睛通道 5/13）

每只眼整个 140° FOV 内 `Σ energy / dist²`，与散热公式中 `nearby_energy` 同源，只是带反距离²加权。这让生物可以感知不同方向的能量浓度，进化出向暖区/集群移动的能力。

### 落地杀伤

- 火山/陨石粒子落地时砸死半径内生物（火山 20px / 陨石 40px）

---

## 八、交互规则

### 吸收

- 接触能量粒子：自动吸收，获得剩余能量（**不受嘴巴冷却限制**）
- 接触痕迹点（非自身）：自动吸收

### 咬（攻击，受冷却限制）

```
条件：嘴输出 < -0.1，且接触到其他生物
咬合力     = |mouth_output|
攻方战力   = combat_power(energy, speed, 同族援助) × 咬合力
守方战力   = combat_power(...)
伤害       = 战力比 × bite_transfer_rate
无额外咬消耗，冷却 1s
```

> 喂食（哺育）机制已**移除**。能量分享通过痕迹点系统实现：生物投放痕迹（输出 6 控制强度），其他生物可吃到。

### 繁殖

```
条件：繁殖输出[3] > 0.2 且 能量 ≥ 繁殖阈值[4]（20~200，神经控制）
子代能量 = 父代能量 × 子代比例[5]（10%~50%，神经控制）
繁殖冷却 10s
优先有性繁殖（crossover + 变异），无配偶时无性繁殖
```

---

## 九、渲染

### 器官绘制

| 器官 | 形状 | 位置 | 颜色 |
|------|------|------|------|
| 左眼 | 白圆+黑瞳 | heading-50° 圆边缘 | 白底黑瞳 |
| 右眼 | 白圆+黑瞳 | heading+50° 圆边缘 | 白底黑瞳 |
| 嘴巴 | 小弧线 | 正前方 | 粉红 (255,100,100) |

眼睛和嘴巴始终绘制（无开关基因），缩放较小时跳过。

### 痕迹点绘制

- 半透明小圆点，颜色=种族色(genome_hash)
- alpha = 60 × 能量比
- 半径 = 0.5 × scale

### 火山热辐射圈

- 极淡圆环标识温度范围（1300 单位半径）

---

## 十、配置参数

参数定义于 `config.toml`，运行时通过 UI 面板可动态调整并自动保存。
完整字段列表见仓库根目录 `config.toml`，下表为关键分组示例（数值仅供参考）。

```toml
# 初始化
initial_speed = 10.0
min_creatures = 50
max_creatures = 500
initial_energy = 160.0

# 火山
volcano_interval = 30.0
volcano_radius = 2300.0
volcano_count = 35
volcano_particle_energy = 185.0
volcano_decay_rate = 0.001
volcano_kill_radius = 62.0
landing_damage_multiplier = 1.7

# 火山正弦周期调制
volcano_interval_cycle = 1500.0
volcano_interval_amplitude = 0.2
volcano_energy_cycle = 1000.0
volcano_energy_amplitude = 0.1

# 温泉（动态出现/消失的次级能量源）
spring_max_count = 9
spring_spawn_interval = 25.0
spring_lifetime = 901.0
spring_emit_interval = 8.0
spring_emit_count = 1
spring_particle_energy = 200.0
spring_radius = 175.0
spring_decay_rate = 0.002
spring_min_distance = 530.0
spring_max_distance = 670.0

# 感知 / 嘴 / 战斗
eye_scan_speed = 400.0       # 度/秒，扫描眼角速度
mouth_cooldown = 0.5
bite_transfer_rate = 0.4
vision_range = 115.0
contact_range = 15.0
combat_speed_weight = 2.0
combat_ally_weight = 0.2

# 散热（指数衰减 + floor 模型）
energy_denominator = 450.0
heat_floor = 0.05
heat_dissipation_coefficient = 0.001

# 代谢
base_metabolism = 0.025
age_metabolism_factor = 0.025
metabolism_exponent = 2.0
move_cost = 0.0005
follow_cost_discount = 0.6
max_speed = 20.0

# 进化
mutation_rate = 0.15           # 全局变异率（base/block 两类共享）
initial_connections_min = 6
initial_connections_max = 12
species_similarity_threshold = 0.95  # 聚类阈值；交配阈值 = × 0.9
dominant_min_age = 250.0

# 痕迹点
trail_decay_rate = 0.01
trail_suppress_radius = 12.0
trail_emit_interval = 0.4

# 繁殖
reproduce_cooldown = 20.0

# SNN 神经后端
neural_backend = "auto"        # auto | cpu | gpu | legacy
snn_ticks_per_frame = 10
neural_tick_rate = 300.0        # 300 × (1/30) = 10 ticks/update
compute_energy_factor = 10.5   # 算力转能量（除以 1e8，0 = 禁用）

# 灭绝
stop_on_extinction = true
auto_spawn_interval = 45.0
```

---

## 十一、里程碑

### v0.1 - 基础框架 ✅
### v0.2 - 神经网络 + NEAT ✅

### v1.0 - 简化版 ✅
- [x] 删除器官基因系统（鼻子/眼睛/嘴巴开关和功率基因）
- [x] 简化感知为纯双眼系统
- [x] 痕迹点系统保留
- [x] 正弦周期调制保留
- [x] 神经控制繁殖保留
- [x] 落地杀伤保留

### v2.x - SNN + 分区 + 双速率变异 ✅
- [x] 神经后端从前馈 NEAT 切到 **SNN（脉冲神经网络）**，节点带膜电位/阈值/不应期
- [x] 引入 Block 分区编号（感官区/联合区/运动区）+ Layer（Processing/Output）+ ConnProbsGene 区块连接概率基因
- [x] **17 维感知**：双眼扫描波束（左右各 8 通道）+ 自身能量（1 通道）
- [x] **7 维输出**：增加痕迹强度输出（输出 6）；喂食机制移除
- [x] **散热模型重写**：旧 env_temp/group_heat 替换为 `heat_floor + (1-heat_floor)·exp(-nearby_energy/energy_denominator)`
- [x] **变异率拆分**：`MutationGene { base, block }`，由生物自演化
- [x] **温泉系统**：动态生成/消亡的次级能量源
- [x] **算力换能量**：神经计算时长可转化为生物能量（compute_energy_factor）
- [x] 多神经后端：CPU SNN / GPU SNN / legacy 直跑

### v2.5 - 反演化停滞改造 ✅
- [x] **Crossover 改为全原子孟德尔遗传**：删除所有连续参数的算术平均，每个原子（连接/节点/ConnProbs/LearningGene/RewardGene）50/50 从父代整取。Crossover 不再主动收缩群体方差
- [x] **删除 `MutationGene`**：自适应变异率在稳定环境下必然塌到下界，改为 `config.mutation_rate` 单一全局常量（默认 0.15）
- [x] **交配阈值与聚类阈值解耦**：`find_mate` 使用 `species_similarity_threshold × 0.9`（0.855），允许跨 clan 基因流，打破单一优势种垄断
- [x] Panel 选中生物详情删除变异率显示（已无意义，查 config 即可）
- 问题背景：演化 1 天后种群收敛到单峰、突变率塌到 0.05 地板、crossover 算术平均持续消灭多样性。此次改造从算子层面（crossover）+ 参数层面（mutation_rate）+ 种群结构（交配阈值）三处联合解决

### v2.4 - 同步批处理 Bridge + GPU readback 合批 ✅
- [x] Bridge 协议从异步双缓冲改为**请求-响应同步** channel（`TickRequest`/`TickResponse`）
- [x] Neural 线程从墙钟固定频率改为 `req_rx.recv()` 阻塞驱动，完全由 world 驱动
- [x] 删除反压机制（`effective_speed = min(target, neural_smooth × 1.2)` 及相关 EMA 采样）
- [x] 确立"加速只是更快获得结果，不影响结果"主旨：10 ticks/update 固定，任何倍速下结果二进制一致
- [x] GPU 路径批内单次 readback：shader 端 atomic spike 累加 + tick 0 写 first_outputs，批末尾一次性 `copy_buffer_to_buffer` 拉回两段
- [x] TickExecutor trait 新接口 `run_batch(inputs, tick_count)` 替换 `inject_inputs/tick/read_outputs` 三步
- [x] 面板单值速度，删除"神经/世界"双值显示

### 未来方向
- [ ] 长时间运行稳定性验证
- [ ] 可视化增强（进化树、基因拓扑）
- [ ] 优势种命名加入存活时长信息
- [ ] 痕迹点颜色随生物颜色同步
