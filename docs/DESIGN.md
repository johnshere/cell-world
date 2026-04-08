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
- **海底火山地形**：可由用户一次性生成的高度图（50×50 chunk），影响生物移动消耗（坡度+海拔阻力），生成后冻结

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
   - altitude_factor = 1 + |h - comfort_h| / range × terrain_altitude_cost  （远离中间舒适带加倍开销）
   - 地形未生成或所在 chunk 无数据时因子 = 1.0
```

### 地形系统（海底火山）

- **网格**：与渲染网格共用常量 `GRID_WORLD_SIZE = 50.0`，每个 50×50 chunk 一个整数高度
- **覆盖范围**：以原点为中心、`volcano_radius + 一格` 的圆，确保边界平滑（任何与该圆相交的 chunk 都会生成）
- **高度算法**（`src/world/terrain.rs::chunk_terrain_height`）：
  - 火山圆锥基底 `60 - dist/160`（clamp 8~60）
  - 6 圈环形山脉 ±5
  - 12 条放射沟壑 ±7
  - 微噪声 ±2
- **生成时机**：用户点击侧边栏 "⛰" 按钮触发（二次确认），按当前 `config.volcano_radius` 一次性生成；生成后**永久冻结**
- **半径变更解耦**：`TerrainMap` 内保存 `generated_radius` 快照；后续 UI 调整 `volcano_radius` 不影响已生成地形；新扩展区域 `terrain_factor = 1.0`
- **持久化**：仅保存 `terrain_generated` 标志和 `terrain_generated_radius`，加载时重新调用 `generate()` 重建（确定性算法，节省存档体积）
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

### 输入（17 维）

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
```

> 常量来源：`Genome::INPUT_SIZE = 17`，写入逻辑见 `src/world/world.rs::compute_perception_pure`。

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

### 变异率基因 MutationGene

变异率不再是全局 config，而是基因组内自演化的双速率：

```rust
struct MutationGene {
    base: f64,   // 默认 0.15，clamp 0.01~0.30
    block: f64,  // 默认 0.15，clamp 0.01~0.30
}
```

- **base**：控制权重 / 连接增删 / SNN 参数 / Layer 切换 / learning / reward / mutation_rate 自身变异
- **block**：控制联合区 block 编号迁移、conn_probs 区块概率基因变异
- 两者各自独立演化（自变异时彼此独立）

### 变异类型（base 速率）

| 变异 | 触发概率 | 说明 |
|------|----------|------|
| 权重变异 | base，每条连接 | 90% 微调 ±0.5，10% 重置 [-1,1] |
| 新增连接 | base | 随机连接两个节点（受 conn_probs 加权） |
| 新增节点 | base | 拆分现有连接，插入隐藏节点 |
| 开关连接 | base | 启用/禁用随机连接 |
| SNN 参数 | base | decay / threshold / refractory_period 抖动 |
| Layer 切换 | base | Block 节点 Processing ↔ Output |
| Learning/Reward | base | 学习/奖励基因抖动 |

### 变异类型（block 速率）

| 变异 | 触发概率 | 说明 |
|------|----------|------|
| Block 迁移 | block | 联合区节点 block 编号 ±2 移动（限制在 8~24） |
| ConnProbs   | block | 区块连接概率基因抖动 |

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
# 注：变异率已迁移至基因组内 MutationGene { base, block }，由生物自身演化
initial_connections_min = 6
initial_connections_max = 12
species_similarity_threshold = 0.95
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
neural_tick_rate = 600.0
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
- [x] 多神经后端：CPU SNN / GPU SNN / 可选异步线程

### 未来方向
- [ ] 长时间运行稳定性验证
- [ ] 可视化增强（进化树、基因拓扑）
- [ ] 优势种命名加入存活时长信息
- [ ] 痕迹点颜色随生物颜色同步
