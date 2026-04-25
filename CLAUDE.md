# CLAUDE.md

Cell-World 神经网络涌现生态模拟器 - Claude Code 开发指南

> **完整设计文档**: [docs/DESIGN.md](docs/DESIGN.md)

## ⚠️ 文档同步铁律

**每次修改代码都必须同步更新 CLAUDE.md 和 docs/DESIGN.md**，这是强制要求，不是建议。

- 修改核心行为、数据结构、模块职责、架构、配置字段、关键常量 → 必须立刻更新两个文档中对应的段落
- 新增/删除文件 → 必须更新 CLAUDE.md 的"核心模块"表
- 修改时间模型、神经后端、线程通信协议 → 必须更新"关键设计"和 DESIGN.md "技术栈/世界特性"
- 提交前自查：能否从文档推导出当前代码行为？不能就说明文档已过时

文档失真比代码 bug 更难发现，一旦过时，未来的修改会基于错误的假设做出错误的决策。

## ⚠️ 涌现不可干预原则

cell-world 的核心理念是让群体行为（集群、尾随、捕猎、哺育、协作）通过演化**自然涌现**。当某个期望行为"应该出现但还没出现"时，有两种思路，只有一种正确：

- ❌ **错**：找到约束该行为的参数，放宽它（降低 clamp / 提高系数 / 增加默认偏置 / 加入预构造的奖励通道）
- ✅ **对**：保持约束不变，让演化用更长时间自行发现该行为所需的结构

**判据**：某个改动是不是"我替演化提前做了一半工作"？如果是，就是错的。只有"修复明显 bug 或架构缺陷"（比如 crossover 算子主动消灭多样性、异步 bridge 破坏加速不变性）才是对的。

**能修 ≠ 该修**：技术上可以放宽某个限制，不代表应该放宽。物理约束本身是演化压力的一部分，绕过它等于偷掉一层涌现。

### 历史教训（不要重复）

- **SNN decay clamp**：曾考虑把 `node.decay` 的 clamp 从 0.99 放宽到 0.9999，让单神经元具备秒级工作记忆，以加速群体行为涌现。理由看似合理（单神经元物理上最多只能记 ~100ms），但**长时记忆正是演化本应发现的结构性创新**——通过多神经元递归环路（A→B→A + 合适的权重/decay 组合）可以形成任意长的工作记忆。放宽 clamp 等于偷掉这一层涌现，让"演化发现了记忆回路"和"我们在物理层作弊"变得无法区分。**结论：不改 decay clamp，耐心等递归环路被演化自然发现。**
- **适应度共享**：曾考虑用"同族拥挤税"打破单一优势种，但拥挤税本质是惩罚聚集，和目标（演化群体行为）方向相反。**结论：不加。**

### 如何正确推动涌现

当行为不出现时，应该优先问：
1. **是不是有架构缺陷阻断了演化路径？**（如 crossover 主动消灭多样性 → 修）
2. **是不是演化信号被某个 bug 扭曲了？**（如加速模式破坏决策一致性 → 修）
3. **还是只是时间不够？**（→ 耐心等，跑更长）
4. **最后**才考虑"是否要调整物理参数"——且只调与该行为**无直接因果关系**的参数（例如调整环境扰动强度，而不是调整生物的感知/决策/记忆能力）

## 常用命令

```bash
cargo build           # 编译项目
cargo run             # 运行项目
cargo build --release # 发布构建（启用 LTO 优化）
cargo fmt             # 格式化代码
cargo clippy          # 代码检查
```

## 核心模块

| 模块     | 文件                       | 职责                                                  |
| -------- | -------------------------- | ----------------------------------------------------- |
| 神经网络 | `src/neural/genome.rs`     | NEAT 基因组，结构变异，PhysioGene 生理敏感度基因      |
|          | `src/neural/spiking.rs`    | CPU SpikingNetwork 前向传播（legacy/cpu 后端）        |
|          | `src/neural/block.rs`      | 分区（感官/联合/运动），ConnProbs 区块连接概率基因    |
|          | `src/neural/bridge.rs`     | 同步批处理桥（TickRequest/TickResponse mpsc 通道）    |
|          | `src/neural/thread.rs`     | 神经线程入口，TickExecutor trait，CpuExecutor 实现    |
|          | `src/neural/gpu.rs`        | GPU 后端（wgpu），批处理 run_batch + 单次 readback    |
|          | `src/neural/snn_tick.wgsl` | Compute shader：atomic spike 累加 + first_outputs     |
|          | `src/neural/slot_alloc.rs` | GPU 固定槽位分配（MAX_CREATURES=512）                 |
| 世界系统 | `src/world/world.rs`       | 主循环、感知、动作执行、bridge 同步调用              |
|          | `src/world/sim_thread.rs`  | sim 线程入口，命令处理，快照导出                     |
|          | `src/world/creature.rs`    | 生物结构                                              |
|          | `src/world/energy.rs`      | 能量粒子                                              |
|          | `src/world/trail.rs`       | 痕迹点系统                                            |
|          | `src/world/spatial.rs`     | 空间索引 O(1) 查询                                    |
|          | `src/world/terrain.rs`     | 50×50 chunk fBm 噪声高度图（生成后冻结）              |
| 渲染     | `src/render/canvas.rs`     | 画布渲染、拖拽缩放                                    |
|          | `src/render/panel.rs`      | 侧边栏统计面板                                        |
| 应用     | `src/app.rs`               | egui 应用主循环、日志、选中                           |
| 配置     | `src/config.rs`            | 参数配置                                              |
| 存储     | `src/store.rs`             | 生物模板存档（JSON）                                  |

## 关键设计

- **线程架构**: 三线程解耦
  - UI 主线程（egui 渲染、面板、输入）
  - sim 线程（world.update、感知并行、动作串行、快照导出，`sim_thread::spawn_sim_thread`）
  - neural 线程（SNN 批处理，`neural::thread::spawn_neural_thread`，仅 bridge 模式存在）
  - UI ↔ sim：`mpsc::Sender<SimCommand>`（命令）+ `Arc<RwLock<SimSnapshot>>`（快照数据）
  - sim ↔ neural：`NeuralBridge` 内 `mpsc<TickRequest>`/`mpsc<TickResponse>`，`TickRequest` 携带 events + inputs + tick_count + rewards（上一帧奖励信号）
  - sim 墙钟节拍 = 1/30s（固定），每拍做 `speed × N` 次 `world.update(SIM_DT)`
- **时间模型**: 固定步长 dt=1/30 模拟秒，加速通过每帧多次 update 实现；SNN 每次 update 固定 10 ticks（neural_tick_rate=300，`300 × 1/30 = 10 ticks/update`）
- **加速语义主旨**: **加速只是更快获得结果，不影响结果**。任意倍速 v 下，给定初始状态 $S_0$ 和配置 $C$，运行 K 次 update 后的状态 $S_K$ 与 v 无关（二进制一致）
  - **Legacy 后端**（`neural_backend="legacy"`）：world.update 内直接调用 `brain.tick_multi(perception, 10)`，CPU SpikingNetwork 原子推进
  - **Bridge 后端**（`auto`/`cpu`/`gpu`）：world 每次 update 向 neural 线程发送 `TickRequest { events, inputs, tick_count=10 }`，**同步阻塞等** `TickResponse`。neural 线程完全由 world 驱动，没有独立节奏、没有反压、没有墙钟
  - 两种后端都严格保持主旨
- **GPU 批内单次 readback（C 方案）**: Bridge + GPU 模式的关键优化
  - Shader（`snn_tick.wgsl`）：`binding 4=spike_counts (atomic<u32>)`、`5=first_outputs (f32)`、`6=tick_params uniform`
  - 批开始：CPU 清零 spike_counts + first_outputs GPU 缓冲
  - 批内每个 tick：write_tick_index → 独立 submit 一次 dispatch（ping-pong bind group 预构建）
  - Shader 行为：tick 0 写直读输出到 first_outputs；所有 tick 内发放的输出节点 `atomicAdd(&spike_counts[out])`
  - 批末尾：一次 `copy_buffer_to_buffer` 把 spike_counts + first_outputs 连续拷到 staging，`map_async` + `Maintain::Wait` 一次性读回
  - CPU 侧按 `output_modes_cache` 决定每个输出是取 first_outputs 还是 `spike_counts / tick_count × 2 - 1`
  - 对比旧方案：从每 tick 1 次 readback + spin_loop → 每批 1 次 readback + Wait，CPU↔GPU 同步开销 10×
- **面板速度**: 同步批处理模型下只有单一"FPS: X | 速度: Nx"显示。历史上的"神经/世界"双值已移除（反压已删除）
- **无限世界**: 无边界，视窗可自由拖拽缩放
- **火山 + 熔岩流**: 火山定期喷发；间隔和能量均受正弦周期调制（模拟季节）。每次喷发额外产生熔岩流粒子（lava_count），使用独立衰减率（lava_decay_rate）。自然衰减死亡时链式扩散子代（lava_spread_probability），被吃不扩散。扩散采用**溢流机制**：子粒子初始落入父粒子所在区块，然后沿等效液面梯度溢流。等效高度=地形高度+lava_level_per_particle×n，n=区块内所有存活粒子数（含普通粒子和熔岩粒子）。溢流时当前区块模拟+1粒子高度，若高于8邻居中最低者则流向最低（多个最低随机选一），标记已访问区块防回弹，最大跳跃数=lava_max_overflow_depth，超限或超出火山半径则丢弃粒子。同一帧多个父粒子死亡按id排序处理，逐个子粒子顺序放置并实时更新区块计数。周期性杀伤关联扩散代数（depth_ratio=chain_depth/max_chain_depth）：间隔=lava_kill_base_interval×(1+depth_ratio×lava_kill_distance_scale)，半径=volcano_kill_radius×max(0,2×(1-depth_ratio))，chain_depth=0杀伤最强，max_chain_depth时半径归零
- **感知系统（扫描眼 + 发光感知）**:
  - 双眼窄波束逐帧扫描（280°/s），140° 全 FOV，探测距离=vision_range
  - 左眼从 heading+20° 逆时针扫，右眼从 heading-20° 顺时针扫
  - 扫描目标：粒子(0.33)、痕迹(0.67)、生物(1.0)
  - 全 FOV 能量密度：反距离 ² 加权（Σ energy/dist²），与散热计算一致
  - 朝向差/速度差：仅当最近目标为生物时有值，否则为 0
  - 自身状态: 1 通道（能量/2000），始终更新
  - 发光感知：360° 全向扫描，独立 light_scan_offset，与眼睛同速，检测 vision_range 内发光生物
- **20 维输入**: 左眼 8 + 右眼 8 + 自身 1 + 地形 1 + 发光感知 2
  - 左眼 [0..7]: 扫描角归一化(-1~1), 目标接近度, 目标能量/200, 实体类型(0/0.33/0.67/1.0), 基因相似度(0~1), 能量密度, 朝向差(-1~1,仅生物), 速度差(-1~1,仅生物)  → block -1
  - 右眼 [8..15]: 扫描角归一化, 目标接近度, 目标能量/200, 实体类型, 基因相似度, 能量密度, 朝向差, 速度差  → block 1
  - 自身 [16]: 能量/2000  → block 0
  - 地形 [17]: 前方坡度方向 `dh.signum()`（-1=下坡, 0=平地或无地形, 1=上坡），前瞻 15px  → block 0
  - 发光扫描角 [18]: 归一化(-1~1)  → block -2
  - 发光强度 [19]: 最近发光生物的 light_intensity(0~1)  → block -2
- **8 维输出（全部直读模式 tanh）**:
  - 转向角(0, block 25)、速度(1, block 25)、嘴(2, block 25，<-0.1=咬/接触食物自动吸收，冷却 1s)
  - 繁殖意愿(3, block -25，>0.2 触发)、繁殖阈值(4, block -25，sigmoid→20~200)、子代能量比例(5, block -25，sigmoid→0.1~0.5)
  - 痕迹强度(6, block 25，正半轴 →0~0.3 自身能量比例)
  - 发光强度(7, block 26，tanh→(v+1)/2→0~1，量化一位小数)
- **发光器官**: 由 output[7] 控制，渲染为身体正中白色小方块（alpha=intensity×255），intensity=0 不渲染
- **体温逸散（指数衰减 + floor）**:
  - nearby_energy = vision_range 内所有粒子能量 + 生物能量
  - heat_factor = heat_floor + (1 - heat_floor) × exp(-nearby_energy / energy_denominator)
  - heat_cost = heat_dissipation_coefficient × 周长 × heat_factor × dt
  - 荒野(nearby_energy=0)时 heat_factor=1.0，散热最大但有上界；能量丰富区散热降至 heat_floor
- **痕迹系统**: 基础痕迹=移动消耗（无额外开销）+ 神经网络控制额外投放（输出 6，正半轴映射 0~30%自身能量），自己的痕迹不可吃、同族其他生物可吃（clan_hash 匹配）；衰减率 0.12/s；抑制半径 10px，生成间隔 0.25s
- **落地杀伤**: 火山喷发粒子落地时砸死半径内生物（volcano_kill_radius）
- **火山粒子分布**: 线性分布（r=u×radius）
- **咬合系统**: 咬合力=|mouth|（mouth<-0.1 触发），伤害=战力比 ×bite_transfer_rate(0.8)，无额外咬消耗，冷却 1s；喂食机制已移除，能量分享通过痕迹实现
- **战力系统**: 战力 = f(能量, 速度, 同族援助)，咬时攻方乘咬合力；同族援助范围=vision_range
- **生理系统（3 通道 + 面板开关）**:
  - 框架：`PhysioState` 帧内缓冲（世界注入），`PhysioGene` 各通道敏感度（可演化）
  - 通道 1：**能量吸收快乐**（pleasure_energy）—— 摄食吸收 `+absorbed/initial_energy`，面板 `reward_energy_enabled` 控制
  - 通道 2：**痕迹吸收快乐**（pleasure_trail）—— 吃痕迹 `+absorbed/initial_energy`，面板 `reward_trail_enabled` 控制
  - 通道 3：**集体快乐**（pleasure_group）—— 每帧 `follow_level × 0.1`，面板 `reward_group_enabled` 控制
  - 学习路径：`total_reward = Σ(通道值 × 敏感度)` → eligibility trace × total_reward × hebbian_sign × hebbian_rate → Δw
  - **Bridge 模式下奖励信号通过 `NeuralBridge.send_reward()` 延迟一帧发送给神经线程**，在下一批 tick 前对正确的 SpikingNetwork 实例调用 `apply_physiology`；Legacy 模式下直接在 world 线程本地调用
  - 三通道独立开关（能量/痕迹/集体），任一通道开启即有奖励驱动学习
- **物理约束**: 存在消耗（基础代谢 × 年龄倍率+体温逸散）、繁殖成本（神经控制 10%~50%）、死亡条件（能量 ≤0）
- **变异率（v2.5 改为全局常量）**: `config.mutation_rate` 单一字段（默认 0.15）
  - 同时作为 base/block 两类变异的触发概率
  - **不再是基因组内可演化基因**（`MutationGene` 已删除）
  - 原因：自适应变异率在稳定环境下必然塌到下界，拖累演化
  - 想调节探索强度直接改 config
- **发育时间基因（maturation_time）**: 控制结构变异（add_connection/add_node）的活跃窗口
  - 存储在 `Genome.maturation_time`（f64，单位=模拟秒），初代=5000.0
  - 繁殖遗传：`child.maturation_time = (parent.age + parent.maturation_time) / 2`（取发起繁殖方）
  - 结构变异调制：`effective_rate = base_rate × 2 × exp(-parent_age / maturation_time)`
    - 幼年父代（age≈0）：结构变异概率 ×2（神经可塑性高）
    - 发育完成（age=maturation_time）：×0.74（略低于基准）
    - 老年（age=3×maturation_time）：×0.10（结构几乎冻结）
  - 仅影响结构变异，权重微调不受影响（权重=持续学习，结构=发育期可塑性）
  - crossover 中原子孟德尔遗传（50/50 选父/母之一）
- **Crossover（v2.5 改为全原子孟德尔）**: 所有连续参数按"原子"整取，不做算术平均
  - 原子粒度：每条共享连接 / 每个共享节点 / 每个 block 的 ConnProbsGene / LearningGene 整块 / PhysioGene 整块 / maturation_time
  - crossover 不创造新值，只重组；创造新值是 mutation 的职责
  - 结果：crossover 不再主动收缩群体方差，多样性保持完全依赖 mutation 注入
- **交配阈值（v2.5）**: `find_mate` 使用 `species_similarity_threshold × 0.9` 作为交配相似度下限
  - 聚类阈值（0.95）严格 → 显示上的种族区分
  - 交配阈值（0.855）宽松 → 允许跨 clan 基因流
  - 目的：打破单一优势种垄断，保持种群遗传多样性
- **祖先追溯聚类**: 沿 parent_id 追溯最老活祖先，相似度 ≥0.9 归入同族
- **正弦周期调制**: 火山的间隔和能量均随时间正弦波动，不同周期交织形成复杂环境
- **地形系统（手动生成，fBm 噪声）**:
  - 与渲染网格共用常量 `GRID_WORLD_SIZE = 50.0`，每个 chunk 整数高度
  - **算法**：火山圆锥基底（中心高 → 向外平滑下降）+ 多倍频 value noise (fBm) 提供自然起伏
  - **种子化**：相同 seed + 相同参数 → 相同地形；弹框中可手动输入或点 🎲 随机
  - **可调参数**（在 ⛰ 弹框中实时调节）：火山口高度、圆锥衰减、噪声尺度、噪声幅度、倍频层数、随机种子
  - 用户点击侧边栏 "⛰" 按钮（在保存按钮左边）触发，二次确认后按当前 `volcano_radius` 一次性生成
  - 覆盖范围 = 半径 + 一格 chunk 余量（避免边界突兀）
  - 生成后**永久冻结**，`generated_radius` 快照与 UI 半径解耦；新扩展区域 `terrain_factor = 1.0`
  - 移动消耗 ×= `1 + max(dh/dist, 0) × terrain_slope_cost`：上坡费力，下坡不补贴
  - 配置项：`terrain_slope_cost`（默认 2.0）
  - 持久化：地形独立 `terrain.json`（与 snapshot 解耦），⛰ 生成时立即写盘，启动无条件加载——无论"发现存档"对话框选恢复或新游戏，地形都保留

## 开发注意

1. NEAT 变异: `genome.rs` → `mutate(conf)` 从 `conf.mutation_rate` 读取触发概率；base 管常规权重/连接/SNN/Layer，block 管 block 编号迁移与 conn_probs，两者目前共享同一个全局 rate
2. 感知系统: `world.rs` → `compute_perception_pure()` 窄波束扫描，左右眼各 8 通道，并行阶段使用（rayon）
3. 动作执行: `world.rs` → `execute_actions()` 7 输出映射（全直读），嘴巴食物吸收不受冷却限制，咬受冷却限制。这部分是串行的，O(N) 扩展瓶颈
4. update_creatures 三阶段:
   - 2a 并行感知 + 应用代谢结果 + 收集 bridge_inputs
   - 2b 同步调用 `bridge.run_batch_sync(inputs, 10)` 拿 fresh outputs
   - 2c 串行动作执行 + 情绪信号收集 + apply_emotions
5. 战力+咬合: `config.rs` → `combat_power()` 公式，`world.rs` → 咬时攻方战力 × 咬合力 vs 守方战力
6. 周围能量: `world.rs` → `compute_nearby_energy_pure()` 查询 vision_range 内粒子+生物总能量
7. 空间索引使用 FxHashMap，查询复用缓冲区避免分配（creature/energy 两套）
8. 种族聚类: `calculate_clan_cache()` 每秒更新一次
9. 神经后端切换: `config.toml` 的 `neural_backend = auto | cpu | gpu | legacy`；legacy 在 world 内直跑 CPU，其他通过 bridge 跑神经线程
10. 所有复杂行为应是进化结果，避免硬编码
11. 繁殖阈值和子代能量比例由神经网络输出[4][5]控制，非固定参数
12. 周围能量密度是推动集群行为涌现的核心机制：生物聚集 →nearby_energy 升高 → 散热降低 → 存活率提升

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

见本文件最上方"⚠️ 文档同步铁律"。每次代码修改都是一次文档同步机会，不要等到"功能完成"再集中更新——那时候细节已经丢失。
