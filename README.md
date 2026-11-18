# Cell World

> 神经网络涌现生态模拟器：集群、种群分化、捕食、合作都不是写死的规则，而是 20 维感知 + 8 维输出的小网络在能量约束下**自然进化**出来的结果。
> Rust + egui/eframe（可选 wgpu GPU 计算），带快照持久化与 MCP 只读接口。

## 快速开始

```bash
cargo run --release              # 图形界面（egui）
cargo run --release -- --mcp     # headless：只起 MCP SSE 服务（127.0.0.1:9877）
```

- 配置全在 `config.toml`，每个字段都有注释（速度、生物上下限、火山参数、变异率…）。
- feature：默认 `persistence + gpu`；`--no-default-features` 可跑纯 CPU 版本。
- 界面操作：拖拽平移，滚轮以鼠标位置为中心缩放。

## 三条设计原则

| 原则 | 体现 |
| --- | --- |
| **无预设行为** | 代码里没有"集群""捕食""合作"的分支，只有 感知 → 网络 → 输出 → 能量结算 这一条循环；行为是选择压力下的副产物 |
| **能量守恒** | 移动消耗转化为痕迹点、被吸收时回到生物、火山喷发是唯一的外源投放；MCP 用 `theoretical_energy` 与 `total_energy` 对账，不允许凭空生能 |
| **加速不改结果** | 固定步长 `dt = 1/30`，加速 = 每帧多跑几次 `update`；任意倍速下给定 `(S₀, config)` 跑 K 次 update，结果与 1× 二进制一致 |

第三条是这个项目最硬的约束：把"快进"做成纯粹的求解器加速，而不是改物理参数。

## 模拟内核

| 模块 | 设计 |
| --- | --- |
| 感知 | 20 维输入：扫描眼，每种感官单侧投射，靠跨半球同源连接传给对侧 |
| 输出 | 8 维、固定语义，全部直读 `tanh` |
| 神经网络 | 脉冲网络（SNN），每次 `update` 固定 10 ticks（`neural_tick_rate = 300`，`300 × 1/30 = 10`）；CPU 多线程与 GPU 两条实现按同一 tick 语义对齐（见 `docs/snn-cpu-gpu-alignment.md`） |
| 结构进化 | NEAT 式：基因组 = 节点 + 连接 + 调质基因（含 `maturation_time` 发育时间基因）；结构变异受发育期调制，交叉为全原子孟德尔遗传 |
| 世界 | 无限平面，无边界；fBm 噪声海底火山地形（一次性生成后冻结，影响上坡移动消耗） |
| 环境驱动力 | 火山按正弦周期调制喷发间隔与能量（模拟季节性）；环境温度 = 火山热（三次方衰减）+ 附近生物能量贡献的集体热——这是集群涌现的压力源 |
| 痕迹点 | 移动消耗转成痕迹，能量守恒，可被吸收 |

## 代码结构

| 路径 | 行数 | 职责 |
| --- | --- | --- |
| `src/world/world.rs` | 2547 | 世界状态与主循环、生物增删、世代统计 |
| `src/app.rs` | 1978 | egui 应用、视窗变换、倍速与暂停控制 |
| `src/neural/genome.rs` | 1665 | 基因组结构、变异与交叉、发育时间 |
| `src/render/force_graph.rs` | 1594 | 力导图：族群关系的布局与梯度着色 |
| `src/mcp.rs` | 1186 | MCP SSE 服务器（tokio + axum），12 个只读工具 |
| `src/render/panel.rs` | 1168 | 数据面板 |
| `src/neural/gpu.rs` + `snn_tick.wgsl` | 1025 + 198 | wgpu 计算通路与 WGSL 核函数 |
| `src/neural/spiking.rs` / `thread.rs` | 431 / 245 | 脉冲传播 / CPU 并行 tick |
| `src/render/canvas.rs` | 625 | 世界渲染 |
| `src/world/terrain.rs` / `sim_thread.rs` | 341 / 319 | 地形生成 / 模拟线程 |
| `src/config.rs` / `src/snapshot.rs` / `src/store.rs` | 444 / 294 / 216 | 配置、快照读写、存储 |

## 分支地图：四代重写

| 分支 | 内容 | 时间 |
| --- | --- | --- |
| `deep-sea`（默认） | **当前版本**：Rust 2.3.1，SNN + NEAT + GPU + MCP | 2026-06 |
| `main` | 第一代：Go，Conway 生命游戏 | 2025-08 |
| `py` | 第二代：Python 复现 | 2025-08 |
| `node` / `node-ts` | 第三代：JS / TS 实现 | 2025-09 ~ 10 |
| `world` | 世界结构实验 | 2025-09 |
| `Volcano-World` / `creature` / `volcano-bug` | 火山、生物形态、缺陷复现等专项实验 | 2026-02 ~ 04 |

整仓 362 次提交。换语言不是为了练语言，而是每一代都卡在不同的瓶颈上：DOM 渲染撑不住粒子数 → canvas → 语言级并行 → GPU 计算。

## 文档

这些文档和代码是同步维护的（`CLAUDE.md` 里有"改代码必须改文档"的硬规则）：

- [`docs/DESIGN.md`](docs/DESIGN.md)（513 行）—— 感知/输出维度、分区架构、NEAT 变异与交叉、温度与痕迹点的完整设计
- [`docs/MCP.md`](docs/MCP.md)（364 行）—— 12 个只读工具与字段语义
- [`docs/snn-cpu-gpu-alignment.md`](docs/snn-cpu-gpu-alignment.md)（264 行）—— CPU 与 GPU 两条脉冲实现的语义对齐
- [`docs/physio-systems.md`](docs/physio-systems.md)（148 行）· [`docs/SIMPLIFICATION.md`](docs/SIMPLIFICATION.md)（116 行）
- [`docs/lifecycles/`](docs/lifecycles) —— 生命史追踪样本

## MCP 接口

`--mcp` 启动后，任何 MCP 客户端都能只读查询实时模拟数据（如 `stat`）：

| 字段 | 含义 |
| --- | --- |
| `creature_count` / `energy_particle_count` / `trail_count` | 生物、能量粒子、痕迹点数量 |
| `total_energy` / `creature_energy` / `theoretical_energy` | 粒子总能 / 生物总能 / 历史投放总量（用于能量对账） |
| `max_generation` / `avg_energy` | 最大世代、生物平均能量 |
| `clan_count` / `top_clans` / `dominant_species` | 族群数、前 5 大族群、优势种 |
| `death_age_stats` / `action_counts` | 死亡年龄统计、五类行为计数 |

客户端配置见 [`.mcp.json`](.mcp.json)。

## 已知限制

- 仓库体积约 **436MB**：早期分支提交过构建产物（`main` 上的 `cellworld.exe`）与快照数据，克隆会比较慢；后续计划清理历史 + 用 LFS。
- `snapshots/`、`snapshot.json`、`terrain.json` 是运行状态文件，不是源码。
- `Cargo.toml` 的 `authors` 还是占位值，未回填。

## 许可

MIT，见 [LICENSE](LICENSE)。
