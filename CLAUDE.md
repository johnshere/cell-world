# CLAUDE.md

Cell-World v1.0 神经网络涌现生态模拟器 - Claude Code 开发指南

> **完整设计文档**: [docs/DESIGN.md](docs/DESIGN.md)

## 常用命令

```bash
cargo build           # 编译项目
cargo run             # 运行项目
cargo build --release # 发布构建（启用 LTO 优化）
cargo fmt             # 格式化代码
cargo clippy          # 代码检查
```

## 核心模块

| 模块     | 文件                    | 职责                        |
| -------- | ----------------------- | --------------------------- |
| 神经网络 | `src/neural/genome.rs`  | NEAT 基因组，结构变异       |
|          | `src/neural/network.rs` | 神经网络前向传播            |
| 世界系统 | `src/world/world.rs`    | 主循环、感知、动作执行      |
|          | `src/world/creature.rs` | 生物结构                    |
|          | `src/world/energy.rs`   | 能量粒子                    |
|          | `src/world/trail.rs`    | 痕迹点系统                  |
|          | `src/world/spatial.rs`  | 空间索引 O(1) 查询          |
|          | `src/world/terrain.rs`  | 50×50 chunk fBm 噪声高度图（火山圆锥+多倍频噪声，生成后冻结）|
| 渲染     | `src/render/canvas.rs`  | 画布渲染、拖拽缩放          |
|          | `src/render/panel.rs`   | 侧边栏统计面板              |
| 应用     | `src/app.rs`            | egui 应用主循环、日志、选中 |
| 配置     | `src/config.rs`         | 参数配置                    |
| 存储     | `src/store.rs`          | 生物模板存档（JSON）        |

## 关键设计

- **时间模型**: 固定步长 dt=1/30 模拟秒，加速通过每帧多次 update 实现；SNN 每次 update 固定 10 ticks（neural_tick_rate=300）
  - **Legacy 后端**（本机 CPU）：世界/神经同一次 update 原子推进，严格 1:1，加速不影响结果
  - **Bridge 后端**（异步/远程 GPU）：世界和神经解耦，容易出现"世界跑 10x、神经只跑 3x"的失真。采用自适应反压：`effective_speed = min(target, max(1, ceil(neural_smooth × 1.2)))`，EMA τ=2s 平滑神经吞吐，探针比例 ×1.2 让世界略领先以逼近 target；失真上界 ≤1/(N+1) 的决策漂移（一帧内首个 sub-step 用新鲜决策、其余用缓存）
  - 面板速度标签：bridge 模式且神经显著落后时显示"神经/世界"双值，否则单值；目标倍速在滑杆上
- **无限世界**: 无边界，视窗可自由拖拽缩放
- **火山 + 陨石能量**: 火山定期喷发，陨石随机降落；间隔和能量均受正弦周期调制（模拟季节）
- **感知系统（扫描眼）**:
  - 双眼窄波束逐帧扫描（280°/s），140° 全 FOV，探测距离=vision_range
  - 左眼从 heading+20° 逆时针扫，右眼从 heading-20° 顺时针扫
  - 扫描目标：粒子(0.33)、痕迹(0.67)、生物(1.0)
  - 全 FOV 能量密度：反距离 ² 加权（Σ energy/dist²），与散热计算一致
  - 朝向差/速度差：仅当最近目标为生物时有值，否则为 0
  - 自身状态: 1 通道（能量/2000），始终更新
- **17 维输入**: 左眼 8 + 右眼 8 + 自身 1
  - 左眼 [0..7]: 扫描角归一化(-1~1), 目标接近度, 目标能量/200, 实体类型(0/0.33/0.67/1.0), 基因相似度(0~1), 能量密度, 朝向差(-1~1,仅生物), 速度差(-1~1,仅生物)
  - 右眼 [8..15]: 扫描角归一化, 目标接近度, 目标能量/200, 实体类型, 基因相似度, 能量密度, 朝向差, 速度差
  - 自身 [16]: 能量/2000
- **7 维输出（全部直读模式 tanh）**: 转向角(0)、速度(1，×10)、嘴(2，<-0.1=咬/接触食物自动吸收，冷却 1s)、繁殖意愿(3，>0.2 触发，冷却 10s)、繁殖阈值(4，sigmoid→20~200)、子代能量比例(5，sigmoid→0.1~0.5)、痕迹强度(6，正半轴 →0~0.3 自身能量比例)
- **体温逸散（指数衰减 + floor）**:
  - nearby_energy = vision_range 内所有粒子能量 + 生物能量
  - heat_factor = heat_floor + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)
  - heat_cost = heat_dissipation_coefficient × 周长 × heat_factor × dt
  - 荒野(nearby_energy=0)时 heat_factor=1.0，散热最大但有上界；能量丰富区散热降至 heat_floor
- **痕迹系统**: 基础痕迹=移动消耗（无额外开销）+ 神经网络控制额外投放（输出 6，正半轴映射 0~30%自身能量），自己的痕迹不可吃、其他生物均可吃；衰减率 0.12/s；抑制半径 10px，生成间隔 0.25s
- **落地杀伤**: 火山/陨石粒子落地时砸死半径内生物（火山 20px/陨石 40px）
- **火山中心富集**: u² 分布，50%粒子在 25%半径内
- **咬合系统**: 咬合力=|mouth|（mouth<-0.1 触发），伤害=战力比 ×bite_transfer_rate(0.8)，无额外咬消耗，冷却 1s；喂食机制已移除，能量分享通过痕迹实现
- **战力系统**: 战力 = f(能量, 速度, 同族援助)，咬时攻方乘咬合力；同族援助范围=vision_range
- **物理约束**: 存在消耗（基础代谢 × 年龄倍率+体温逸散）、繁殖成本（神经控制 10%~50%）、死亡条件（能量 ≤0）
- **变异率基因 (MutationGene)**: 拆分为两个独立速率
  - `base`（默认 0.15）：权重、连接增删、SNN 参数、Layer 切换、learning/reward 基因变异
  - `block`（默认 0.15）：联合区 block 编号变异、conn_probs 区块概率基因变异
  - 两者各自独立演化（自变异时彼此独立，clamp 0.01~0.30）
- **祖先追溯聚类**: 沿 parent_id 追溯最老活祖先，相似度 ≥0.9 归入同族
- **正弦周期调制**: 火山/陨石的间隔和能量均随时间正弦波动，不同周期交织形成复杂环境
- **地形系统（手动生成，fBm 噪声）**:
  - 与渲染网格共用常量 `GRID_WORLD_SIZE = 50.0`，每个 chunk 整数高度
  - **算法**：火山圆锥基底（中心高 → 向外平滑下降）+ 多倍频 value noise (fBm) 提供自然起伏
  - **种子化**：相同 seed + 相同参数 → 相同地形；弹框中可手动输入或点 🎲 随机
  - **可调参数**（在 ⛰ 弹框中实时调节）：火山口高度、圆锥衰减、噪声尺度、噪声幅度、倍频层数、随机种子
  - 用户点击侧边栏 "⛰" 按钮（在保存按钮左边）触发，二次确认后按当前 `volcano_radius` 一次性生成
  - 覆盖范围 = 半径 + 一格 chunk 余量（避免边界突兀）
  - 生成后**永久冻结**，`generated_radius` 快照与 UI 半径解耦；新扩展区域 `terrain_factor = 1.0`
  - 移动消耗 ×= `slope_factor × altitude_factor`：上坡费力（不补贴下坡）+ 远离中间舒适带费力
  - 配置项：`terrain_slope_cost`（默认 2.0）、`terrain_altitude_cost`（默认 0.5）
  - 持久化：地形独立 `terrain.json`（与 snapshot 解耦），⛰ 生成时立即写盘，启动无条件加载——无论"发现存档"对话框选恢复或新游戏，地形都保留

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(conf)` 使用 `MutationGene { base, block }` 双速率控制；base 管常规权重/连接/SNN/Layer，block 管 block 编号迁移与 conn_probs
2. 感知系统: `world.rs` → `compute_perception_scanning()` 扫描推进+`compute_perception_scanning_inner()` 窄波束检测粒子/生物/痕迹
3. 动作执行: `world.rs` → `execute_actions()` 7 输出映射（全直读），嘴巴食物吸收不受冷却限制，咬受冷却限制
4. 战力+咬合: `config.rs` → `combat_power()` 公式，`world.rs` → 咬时攻方战力 × 咬合力 vs 守方战力
5. 周围能量: `world.rs` → `compute_nearby_energy()` 查询 vision_range 内粒子+生物总能量
6. 空间索引使用 FxHashMap，查询复用缓冲区避免分配（creature/energy 两套）
7. 种族聚类: `calculate_clan_cache()` 每秒更新一次
8. 所有复杂行为应是进化结果，避免硬编码
9. 繁殖阈值和子代能量比例由神经网络输出[4][5]控制，非固定参数
10. 周围能量密度是推动集群行为涌现的核心机制：生物聚集 →nearby_energy 升高 → 散热降低 → 存活率提升

## 待办

- [ ] 优势种自动命名中加入存活时长信息（当前格式 `v{version}_{月日_时分}`，改为包含 avg_age 或 max_age）
  - 代码位置: `src/app.rs:241` → `format!("v{}_{}", version, now.format("%m%d_%H%M"))`
  - DominantCandidate 中已有 `avg_age` 和 `max_generation` 字段可用
- [ ] 痕迹点颜色应与产生它的生物颜色一致，且生物颜色变化时痕迹点颜色也要同步更新
  - 痕迹点结构: `src/world/trail.rs`
  - 渲染: `src/render/canvas.rs`
  - 需要在 trail 中记录来源生物 ID 或颜色，渲染时取对应颜色

## 目标

现在，希望你启动程序。每隔段时间观察日志，必要情况下增加日志，用于调试。期望不通过硬编码，通过配置调试、神经学习
，让这个世界演化出采集、族群、哺育、捕猎等等，群体性的行为。

## 更新文档

每次更新代码、更新版本号时都要更新文档；claude.md、design.md 两个文档，必须与代码保持一致
