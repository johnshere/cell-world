# CLAUDE.md

Cell-World 神经网络涌现生态模拟器 - Claude Code 开发指南

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

| 模块     | 文件                    | 职责                       |
| -------- | ----------------------- | -------------------------- |
| 神经网络 | `src/neural/genome.rs`  | NEAT 基因组，结构变异      |
|          | `src/neural/network.rs` | 神经网络前向传播           |
| 世界系统 | `src/world/world.rs`    | 主循环、感知、动作执行     |
|          | `src/world/creature.rs` | 生物结构                   |
|          | `src/world/energy.rs`   | 能量粒子                   |
|          | `src/world/spatial.rs`  | 空间索引 O(1) 查询         |
| 渲染     | `src/render/canvas.rs`  | 画布渲染、拖拽缩放         |
|          | `src/render/panel.rs`   | 侧边栏统计面板             |
| 应用     | `src/app.rs`            | egui 应用主循环、日志、选中 |
| 配置     | `src/config.rs`         | 参数配置                   |
| 存储     | `src/store.rs`          | 生物模板存档（JSON）        |

## 关键设计

- **无限世界**: 无边界，视窗可自由拖拽缩放
- **火山 + 陨石能量**: 火山定期喷发（30秒/次，60粒子），陨石随机降落（12秒/次，18粒子）
- **3眼感知系统**: 左眼(-45°)、中眼(0°)、右眼(+45°)，半角30°，视觉半径150
- **11维输入**: 3眼×3通道（食物距离、同族距离、异族距离）+ 自身能量 + 体温状态
- **4维输出**: 转向角(0)、速度(1)、嘴(2，负=咬/正=喂/接触食物自动吸收)、繁殖(3)
- **三条物理约束**: 存在消耗（基础 + 年龄倍率 + 体温逸散）、繁殖成本（30%）、死亡条件（能量≤0）
- **统一变异率**: 所有变异逻辑共用同一个 `mutation_rate`（15%）
- **祖先追溯聚类**: 沿 parent_id 追溯最老活祖先，相似度≥0.9 归入同族

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(rate)` 单一变异率控制所有变异
2. 3眼感知: `world.rs` → `compute_eye_perception()` 每帧计算
3. 动作执行: `world.rs` → `execute_actions()` 4输出映射
4. 空间索引使用 FxHashMap，查询复用缓冲区避免分配
5. 种族聚类: `calculate_clan_cache()` 每秒更新一次
6. 所有复杂行为应是进化结果，避免硬编码

## 目标

现在，希望你启动程序。每隔段时间观察日志，必要情况下增加日志，用于调试。期望不通过硬编码，通过配置调试、神经学习
，让这个世界演化出采集、族群、哺育、捕猎等等，群体性的行为。
