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
```

---

## 四、神经网络设计

### 感知系统（双眼 + 温度通道，带冷却）

```
       左眼(140°)    右眼(140°)
      heading-50°    heading+50°
         \              /
          \            /
           ○ (生物)
```

- **双眼**：±50° 方向，各 70° 半角（单眼140°视野，左右重叠40°），最近距离模型，冷却 0.075s
  - 探测距离 = vision_range（固定）
  - 从眼睛位置（体表）计算距离，而非身体中心
  - 每只眼新增**热感温度通道**：视锥方向中点的火山热 + 视锥内生物集体热
- **自身状态**：2通道，始终更新（不受冷却影响）

### 输入（10维）

```
左眼 (4通道) [0..3]:
  [0] 食物接近度（最近，1-dist/range）
  [1] 同族接近度（最近）
  [2] 异族接近度（最近）
  [3] 热感温度（视锥方向火山热 + 视锥内生物集体热）

右眼 (4通道) [4..7]:
  [4] 食物接近度
  [5] 同族接近度
  [6] 异族接近度
  [7] 热感温度

自身状态 (2通道) [8..9]:
  [8] 自身能量 (energy/200, clamp 0~1)
  [9] 当前环境温度 (env_temp = 火山热 + 集体热, clamp 0~1)
```

### 输出（6维，固定）

| 编号 | 功能 | 输出范围 | 说明 |
|------|------|---------|------|
| 0 | 转向角 | tanh(-1~1) | × 2π = 每秒转向弧度 |
| 1 | 速度 | tanh(-1~1) | abs × 25 = 每秒移动距离 |
| 2 | 嘴 | tanh(-1~1) | <-0.1 咬（捕食）；>+0.1 喂（哺育） |
| 3 | 繁殖意愿 | tanh(-1~1) | >0.2 时触发繁殖 |
| 4 | 繁殖阈值 | tanh→sigmoid | 映射到 20~200 能量阈值 |
| 5 | 子代能量比例 | tanh→sigmoid | 映射到 0.1~0.5 |

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
    nodes: Vec<NodeGene>,
    connections: Vec<ConnectionGene>,
    next_node_id: usize,
}
```

### 变异类型

| 变异 | 触发概率 | 说明 |
|------|----------|------|
| 权重变异 | 15% 每条连接 | 90% 微调 ±0.5，10% 重置 [-1,1] |
| 新增连接 | 15% | 随机连接两个节点 |
| 新增节点 | 15% | 拆分现有连接，插入隐藏节点 |
| 开关连接 | 15% | 启用/禁用随机连接 |

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

## 七、环境温度（火山热 + 集体热）

### 火山热公式

```
linear = (1 - dist_to_volcano / volcano_heat_range).clamp(0, 1)
volcano_temp = linear³  // 三次方衰减，火山口附近极热、远处急剧下降
```

### 集体热公式

```
group_energy = Σ(nearby_creatures.energy)  // group_heat_radius 范围内
group_temp = group_energy / group_heat_denominator
env_temp = min(volcano_temp + group_temp, 1.0)
```

### 体温逸散

```
body_radius = (energy × 1.28)^(1/3)
circumference = body_radius × 2π
heat_cost = heat_dissipation_coefficient × circumference / (env_temp + 0.01) × dt
```

- env_temp 越低散热越快
- 集群可减缓散热（集体热提升 env_temp）
- 小体型生物周长/能量比更大，散热更快

### 眼睛热感通道

每只眼的热感温度 = 视锥方向中点的火山热 + 视锥内生物能量/group_heat_denominator

这让生物可以感知不同方向的温度差异，进化出向暖区或集群方向移动的能力。

### 落地杀伤

- 火山/陨石粒子落地时砸死半径内生物（火山 20px / 陨石 40px）

---

## 八、交互规则

### 吸收

- 接触能量粒子：自动吸收，获得剩余能量（不受嘴巴冷却限制）
- 接触痕迹点：自动吸收

### 咬（捕食，受冷却限制）

```
条件：嘴输出 < -0.1，且接触到其他生物
咬合力 = |mouth_output|
攻方战力 = combat_power(energy, env_temp, speed, ally_energy) × 咬合力
伤害 = 战力比 × bite_transfer_rate (0.4)
无额外咬消耗
```

### 喂（哺育，受冷却限制）

```
条件：嘴输出 > +0.1，且接触到其他生物
效果：转移能量，效率 85%
```

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

```toml
# 速度与初始化
initial_speed = 3.0
min_creatures = 60
initial_energy = 160.0
initial_scale = 0.6

# 火山
volcano_x = 0.0
volcano_y = 0.0
volcano_interval = 60.0
volcano_radius = 900.0
volcano_count = 130
volcano_particle_energy = 80.0
volcano_decay_rate = 0.035

# 陨石
meteorite_interval = 40.0
meteorite_count = 20
meteorite_length = 350.0
meteorite_particle_energy = 120.0
meteorite_decay_rate = 0.01

# 正弦周期调制
volcano_interval_cycle = 700.0
volcano_interval_amplitude = 0.7
volcano_energy_cycle = 300.0
volcano_energy_amplitude = 0.5
meteorite_interval_cycle = 400.0
meteorite_interval_amplitude = 0.9
meteorite_energy_cycle = 250.0
meteorite_energy_amplitude = 0.9

# 落地杀伤
volcano_kill_radius = 20.0
meteorite_kill_radius = 40.0

# 冷却
eye_cooldown = 0.075
mouth_cooldown = 1.0
bite_transfer_rate = 0.4

# 环境温度
volcano_heat_range = 1300.0
cold_loss_factor = 1.0
thermal_mass_factor = 12.0

# 集体热效应
group_heat_radius = 150.0
group_heat_denominator = 5000.0

# 代谢
base_metabolism = 0.035
age_metabolism_factor = 0.03
move_cost = 0.0003
heat_dissipation_coefficient = 0.006
feed_efficiency = 0.85

# 感知
vision_range = 150.0
contact_range = 15.0

# 进化
mutation_rate = 0.15
initial_connections_min = 6
initial_connections_max = 12
species_similarity_threshold = 0.9

# 战力
combat_temp_weight = 0.5
combat_speed_weight = 0.3
combat_ally_weight = 0.8
combat_ally_range = 50.0

# 优势种
dominant_min_age = 350.0

# 痕迹点
trail_decay_rate = 0.12
trail_suppress_radius = 10.0
trail_emit_interval = 0.25

# 繁殖
reproduce_cooldown = 10.0
```

---

## 十一、里程碑

### v0.1 - 基础框架 ✅
### v0.2 - 神经网络 + NEAT ✅

### v1.0 - 简化版 ✅
- [x] 删除器官基因系统（鼻子/眼睛/嘴巴开关和功率基因）
- [x] 简化感知为纯双眼系统（10维输入）
- [x] 新增集体热效应（group_heat_radius + group_heat_denominator）
- [x] 眼睛热感温度通道（感知不同方向的温度，支持趋暖行为）
- [x] 简化体温逸散（coefficient × circumference / env_temp × dt）
- [x] 删除咬消耗，提高咬转移率至 0.4
- [x] 痕迹点系统保留
- [x] 正弦周期调制保留
- [x] 神经控制繁殖保留
- [x] 落地杀伤保留

### 未来方向
- [ ] 并行计算（rayon）
- [ ] 参数调优与进化实验
- [ ] 长时间运行稳定性验证
- [ ] 可视化增强（进化树、基因拓扑）
