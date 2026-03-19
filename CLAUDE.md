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
- **感知系统（双眼，带冷却）**:
  - 双眼（±50°方向，70°半角，探测距离=vision_range，冷却0.075s）: 距离模型，4通道×2（食物/同族/异族/能量密度）
  - 自身状态: 2通道（能量/周围能量丰富度），始终更新
- **10维输入**: 左眼4 + 右眼4 + 自身2
  - 左眼 [0..3]: 食物接近度, 同族接近度, 异族接近度, 能量密度
  - 右眼 [4..7]: 食物接近度, 同族接近度, 异族接近度, 能量密度
  - 自身 [8..9]: 能量, 周围能量丰富度(nearby_energy/energy_denominator)
- **6维输出**: 转向角(0)、速度(1)、嘴(2，负=咬/正=喂/接触食物自动吸收)、繁殖意愿(3)、繁殖阈值(4，20~200)、子代能量比例(5，0.1~0.5)
- **体温逸散（指数衰减 + floor）**:
  - nearby_energy = vision_range 内所有粒子能量 + 生物能量
  - heat_factor = heat_floor + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)
  - heat_cost = heat_dissipation_coefficient × 周长 × heat_factor × dt
  - 荒野(nearby_energy=0)时 heat_factor=1.0，散热最大但有上界；能量丰富区散热降至 heat_floor
- **痕迹点系统**: 移动消耗转化为痕迹点（能量守恒），可被吸收，衰减率0.12/s；抑制半径10px，生成间隔0.25s
- **落地杀伤**: 火山/陨石粒子落地时砸死半径内生物（火山20px/陨石40px）
- **火山中心富集**: u²分布，50%粒子在25%半径内
- **咬合系统**: 咬合力=|mouth|，伤害=战力比×bite_transfer_rate(0.4)，无额外咬消耗，冷却1s
- **战力系统**: 战力 = f(能量, 速度, 同族援助)，咬时攻方乘咬合力；同族援助范围=vision_range
- **物理约束**: 存在消耗（基础代谢×年龄倍率+体温逸散）、繁殖成本（神经控制10%~50%）、死亡条件（能量≤0）
- **统一变异率**: 所有变异逻辑共用同一个 `mutation_rate`（15%）
- **祖先追溯聚类**: 沿 parent_id 追溯最老活祖先，相似度≥0.9 归入同族
- **正弦周期调制**: 火山/陨石的间隔和能量均随时间正弦波动，不同周期交织形成复杂环境

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(rate)` 单一变异率控制所有变异
2. 感知系统: `world.rs` → `compute_perception_with_cooldown()` 冷却触发+`compute_perception_inner()` 双眼扫描+温度通道
3. 动作执行: `world.rs` → `execute_actions()` 6输出映射，嘴巴食物吸收不受冷却限制，咬/喂受冷却限制
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
