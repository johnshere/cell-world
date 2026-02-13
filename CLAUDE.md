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

| 模块 | 文件 | 职责 |
|------|------|------|
| 神经网络 | `src/neural/genome.rs` | NEAT 基因组，结构变异 |
| | `src/neural/network.rs` | 神经网络前向传播 |
| 世界系统 | `src/world/world.rs` | 主循环、感知、动作执行 |
| | `src/world/creature.rs` | 生物结构 |
| | `src/world/spatial.rs` | 空间索引 O(1) 查询 |
| 渲染 | `src/render/canvas.rs` | 画布渲染、拖拽缩放 |
| 配置 | `src/config.rs` | 参数配置 |

## 关键设计

- **无限世界**: 无边界，视窗可自由拖拽缩放，能量粒子仅在视窗内生成
- **17 维输入**: 8方向能量 + 8方向邻居相似度 + 自身能量
- **6 功能池**: 移动XY、吸收、释放、繁殖、能量转移
- **初始功能**: 移动X(0)、移动Y(1)、吸收(2)、繁殖(4)
- **三条物理约束**: 存在消耗（基础+百分比）、繁殖成本、死亡条件
- **统一变异率**: 所有变异逻辑共用同一个 `mutation_rate`

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(rate)` 单一变异率控制所有变异
2. 感知计算: `world.rs` → `perceive()`
3. 空间索引使用 FxHashMap
4. 所有复杂行为应是进化结果，避免硬编码
