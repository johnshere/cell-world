# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Cell-World 是一个基于 Rust + egui 的神经网络涌现生态模拟器。所有生物行为（集群、种群分化、捕食、合作）均通过 NEAT 算法自然进化产生，而非预设编程。

## 常用命令

```bash
cargo build           # 编译项目
cargo run             # 运行项目
cargo build --release # 发布构建（启用 LTO 优化）
cargo fmt             # 格式化代码
cargo clippy          # 代码检查
```

## 架构概览

### 核心模块

- **神经网络** (`src/neural/`):
  - `genome.rs`: NEAT 基因组，支持结构变异和功能解锁
  - `network.rs`: 神经网络前向传播，拓扑排序计算
  - `neat.rs`: 创新号追踪

- **世界系统** (`src/world/`):
  - `world.rs`: 主循环、感知、动作执行、能量生成
  - `creature.rs`: 生物结构，含基因组和神经网络
  - `energy.rs`: 能量粒子（阳光）
  - `spatial.rs`: 空间索引网格，O(1) 邻居查询

- **渲染系统** (`src/render/`):
  - `canvas.rs`: 世界画布，支持拖拽缩放
  - `panel.rs`: 统计面板

- **应用入口** (`src/app.rs`): egui 主应用循环

### 设计理念

详见 `docs/DESIGN.md`

**三条物理约束**：
1. 存在消耗：活着每秒消耗能量
2. 繁殖成本：繁殖时分走能量给子代
3. 死亡条件：能量 ≤ 0 → 死亡

**功能池**（6 个可进化解锁的功能）：
| 编号 | 功能 | 输出范围 |
|------|------|---------|
| 0 | 移动X | -1 ~ +1 |
| 1 | 移动Y | -1 ~ +1 |
| 2 | 吸收 | >0.5 触发 |
| 3 | 释放 | 0 ~ 1 |
| 4 | 繁殖 | >0.5 触发 |
| 5 | 能量转移 | -1 ~ +1 |

**25 维输入**：
- 0-7: 8 方向能量感知
- 8-15: 8 方向邻居存在
- 16-23: 8 方向基因相似度
- 24: 自身能量

### 关键参数

```rust
// Config (src/config.rs)
world_width: 800.0           // 世界宽度
world_height: 600.0          // 世界高度
initial_creatures: 50        // 初始生物数
initial_energy: 100.0        // 初始能量
base_metabolism: 0.1         // 每秒基础消耗
mutation_rate: 0.1           // 变异率
sense_range: 50.0            // 感知半径
```

## 代码规范

- **格式**: `cargo fmt`
- **检查**: `cargo clippy`
- **依赖**: egui 0.29, rand 0.8, rustc-hash 2.0

## 开发注意

1. NEAT 变异逻辑在 `genome.rs` 的 `mutate()` 方法
2. 感知计算在 `world.rs` 的 `perceive()` 方法
3. 空间索引使用 FxHashMap 提升性能
4. 所有复杂行为应是进化结果，避免硬编码行为规则
