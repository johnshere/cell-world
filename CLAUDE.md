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

| 模块     | 文件                    | 职责                           |
| -------- | ----------------------- | ------------------------------ |
| 神经网络 | `src/neural/genome.rs`  | NEAT 基因组，结构变异           |
|          | `src/neural/network.rs` | 神经网络前向传播               |
| 世界系统 | `src/world/world.rs`    | 主循环、感知、动作执行         |
|          | `src/world/creature.rs` | 生物结构                       |
|          | `src/world/energy.rs`   | 能量粒子                       |
|          | `src/world/trail.rs`    | 痕迹点系统                     |
|          | `src/world/spatial.rs`  | 空间索引 O(1) 查询             |
| 渲染     | `src/render/canvas.rs`  | 画布渲染、拖拽缩放             |
|          | `src/render/panel.rs`   | 侧边栏统计面板                 |
| 应用     | `src/app.rs`            | egui 应用主循环、日志、选中     |
| 配置     | `src/config.rs`         | 参数配置                       |
| 存储     | `src/store.rs`          | 生物模板存档（JSON）            |

## 关键设计

- **无限世界**: 无边界，视窗可自由拖拽缩放
- **火山 + 陨石能量**: 火山定期喷发，陨石随机降落；间隔和能量均受正弦周期调制（模拟季节）
- **感知系统（扫描眼）**:
  - 双眼窄波束逐帧扫描（280°/s），140°全FOV，探测距离=vision_range
  - 左眼从heading+20°逆时针扫，右眼从heading-20°顺时针扫
  - 扫描目标：粒子(0.33)、痕迹(0.67)、生物(1.0)
  - 全FOV能量密度：反距离²加权（Σ energy/dist²），与散热计算一致
  - 朝向差/速度差：仅当最近目标为生物时有值，否则为0
  - 自身状态: 1通道（能量/2000），始终更新
- **17维输入**: 左眼8 + 右眼8 + 自身1
  - 左眼 [0..7]: 扫描角归一化(-1~1), 目标接近度, 目标能量/200, 实体类型(0/0.33/0.67/1.0), 基因相似度(0~1), 能量密度, 朝向差(-1~1,仅生物), 速度差(-1~1,仅生物)
  - 右眼 [8..15]: 扫描角归一化, 目标接近度, 目标能量/200, 实体类型, 基因相似度, 能量密度, 朝向差, 速度差
  - 自身 [16]: 能量/2000
- **7维输出（全部直读模式 tanh）**: 转向角(0)、速度(1，×10)、嘴(2，<-0.1=咬/接触食物自动吸收，冷却1s)、繁殖意愿(3，>0.2触发，冷却10s)、繁殖阈值(4，sigmoid→20~200)、子代能量比例(5，sigmoid→0.1~0.5)、痕迹强度(6，正半轴→0~0.3自身能量比例)
- **体温逸散（指数衰减 + floor）**:
  - nearby_energy = vision_range 内所有粒子能量 + 生物能量
  - heat_factor = heat_floor + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)
  - heat_cost = heat_dissipation_coefficient × 周长 × heat_factor × dt
  - 荒野(nearby_energy=0)时 heat_factor=1.0，散热最大但有上界；能量丰富区散热降至 heat_floor
- **痕迹系统**: 基础痕迹=移动消耗（无额外开销）+ 神经网络控制额外投放（输出6，正半轴映射0~30%自身能量），自己的痕迹不可吃、其他生物均可吃；衰减率0.12/s；抑制半径10px，生成间隔0.25s
- **落地杀伤**: 火山/陨石粒子落地时砸死半径内生物（火山20px/陨石40px）
- **火山中心富集**: u²分布，50%粒子在25%半径内
- **咬合系统**: 咬合力=|mouth|（mouth<-0.1触发），伤害=战力比×bite_transfer_rate(0.8)，无额外咬消耗，冷却1s；喂食机制已移除，能量分享通过痕迹实现
- **战力系统**: 战力 = f(能量, 速度, 同族援助)，咬时攻方乘咬合力；同族援助范围=vision_range
- **物理约束**: 存在消耗（基础代谢×年龄倍率+体温逸散）、繁殖成本（神经控制10%~50%）、死亡条件（能量≤0）
- **统一变异率**: 所有变异逻辑共用同一个 `mutation_rate`（15%）
- **祖先追溯聚类**: 沿 parent_id 追溯最老活祖先，相似度≥0.9 归入同族
- **正弦周期调制**: 火山/陨石的间隔和能量均随时间正弦波动，不同周期交织形成复杂环境

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(rate)` 单一变异率控制所有变异
2. 感知系统: `world.rs` → `compute_perception_scanning()` 扫描推进+`compute_perception_scanning_inner()` 窄波束检测粒子/生物/痕迹
3. 动作执行: `world.rs` → `execute_actions()` 7输出映射（全直读），嘴巴食物吸收不受冷却限制，咬受冷却限制
4. 战力+咬合: `config.rs` → `combat_power()` 公式，`world.rs` → 咬时攻方战力×咬合力 vs 守方战力
5. 周围能量: `world.rs` → `compute_nearby_energy()` 查询 vision_range 内粒子+生物总能量
6. 空间索引使用 FxHashMap，查询复用缓冲区避免分配（creature/energy两套）
7. 种族聚类: `calculate_clan_cache()` 每秒更新一次
8. 所有复杂行为应是进化结果，避免硬编码
9. 繁殖阈值和子代能量比例由神经网络输出[4][5]控制，非固定参数
10. 周围能量密度是推动集群行为涌现的核心机制：生物聚集→nearby_energy升高→散热降低→存活率提升

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
