## 大版本更新

### 1. 地形消耗最大倍率上限

#### 当前状态

```rust
terrain_factor = 1.0 + max(dh / distance, 0.0) × terrain_slope_cost  // terrain_slope_cost 默认 2.0
move_cost = ... × terrain_factor
```

`dh / distance` 是真实坡度（单位水平距离的高度变化），与帧步长无关，最大坡度取决于 fBm 参数和火山锥斜率。目前没有显式 clamp，实际观测最大约 10 倍。

#### 需求

最大倍率上限从 ~10 改为 **100**。

#### 公式讨论

**线性方案（现状基础上只加 clamp）**：
```
terrain_factor = 1.0 + min(max(dh/distance, 0.0), max_slope) × terrain_slope_cost
```
- `max_slope` 控制「坡度生效上限」，平坦区不受影响
- `terrain_slope_cost` 控制「坡度惩罚强度」
- 达到 `max_slope` 后地形因子封顶，不再随坡度增加
- 需要新增一个配置项 `terrain_max_slope`（默认建议 50，让 1 + 50×2 = 101 ≈ 100）
- 优点：直观，两个参数独立
- 缺点：在 max_slope 处有硬拐点

**非线性方案（sigmoid）**：
```
terrain_factor = 1.0 + max_cap × 2 / (1 + exp(-slope/steepness)) - max_cap
                 // 或更简单的：1 + max_cap × slope / (slope + half_k)
```
- 平滑过渡，无硬拐点
- max_cap 直接就是最大额外倍率（如 99，总倍率上限 100）
- 优点：优雅，无参数耦合
- 缺点：参数语义不如线性直观

**复合方案**：
```
terrain_factor = 1.0 + terrain_slope_cost × slope × exp(-slope / softening)
```
- 低坡线性（系数 ≈ terrain_slope_cost），高坡被指数压弯
- 软化点 `softening` 控制"非线性化起点"
- 最大值约在 slope = softening 处，值 ≈ 1 + terrain_slope_cost × softening / e
- 优点：低坡行为不变，高坡自限
- 缺点：参数多，用户需要同时调 slope_cost 和 softening 才能设准上限

#### 决策

待定。倾向 **线性 + clamp**，新增 `terrain_max_slope` 配置项，保持当前 `terrain_slope_cost=2.0` 语义不变。

### 2. 神经网络初始拓扑：双层连接区 + 内部随机 + cross 桥接

替换当前 v2.4.1 的 56 节点 / 68 边骨架。

#### 2.1 动机

当前设计只有 3 跳（input→Proc→motorOut→output），且 sensory block 无 Out、motor block 无 Proc（故意打破 C2）。问题：
- 信号在无 fan-in 的 daisy chain 上每跳衰减，5 跳以上基本死
- 演化被迫先"修好" block 类型缺失，浪费早期代际
- 没有区内处理（sensory 内部、motor 内部），网络深度全靠 evolve add_node 慢慢堆

新设计让初始拓扑自带区内处理 web + 区间桥接，更深但 fan-in 更大，演化从更合理的基底起步。

#### 2.2 节点结构（84 = 28 I/O + 56 block）

##### Input 区（sensory blocks）

每 input 创建 1 Proc + 1 Out（同 sensory block）：

| block | 通道 | Input | Proc | Out | 合计 |
|-------|------|-------|------|-----|------|
| -1 左眼 | 8 | 8 | 8 | 8 | 24 |
| +1 右眼 | 8 | 8 | 8 | 8 | 24 |
| -2 光耳 | 2 | 2 | 2 | 2 | 6 |
| -3 自身能量 | 1 | 1 | 1 | 1 | 3 |
| +3 地形 | 1 | 1 | 1 | 1 | 3 |

Input 区 block 节点合计：**40**（20 Proc + 20 Out）

##### Output 区（motor blocks）

每 output 创建 1 Out + 1 Proc（同 motor block）：

| block | output | Out | Proc | Output | 合计 |
|-------|--------|-----|------|--------|------|
| +25 运动 | 4 | 4 | 4 | 4 | 12 |
| -25 繁殖 | 3 | 3 | 3 | 3 | 9 |
| -26 光嘴 | 1 | 1 | 1 | 1 | 3 |

Output 区 block 节点合计：**16**（8 Out + 8 Proc）

##### 总计

20 Input + 8 Output + 40 sensory block + 16 motor block = **84 节点**

#### 2.3 连接规则（5 类）

##### A. 硬 I/O 边（28 条，权重 [-1.0, 1.0]）

- input_i → 其独占 Proc_i（20 条）
- 独占 Out_j → output_j（8 条）

##### B. Input 区内部随机连接（~76 条）

- 在每个 sensory block 内部，Proc↔Out 随机互联
- 规则：每个 Proc 随机连 2 个同 block Out；每个 Out 随机连 2 个同 block Proc
- 若 block 内目标数 ≤ 连入数，改为全连（去重）
- 权重 [-1.0, 1.0]

##### C. Output 区内部随机连接（~30 条）

- 每个 motor Proc 随机连 2 个同 block Out；每个 motor Out 随机连 2 个同 block Proc
- 权重 [-1.0, 1.0]

##### D. 跨半球同源连接（~20 条，sensory Out → 对侧对应 sensory Proc）

- 范围：仅 Input 区同源对（|-1|↔|+1| 视觉，|-2|↔|+2| 光语言），|-3|↔|+3| 非同源跳过
- 规则：每个 sensory block 的 Out 随机选 1 个对侧对应 block 的 Proc 连边
- 连入端限制：对侧 Proc 最多收 2 条（防单点过载）
- 权重 [-0.5, 0.5]（胼胝体语义——辅助通道，非主通路）
- 设计意图：让左右眼/左右听觉信息在初始就有弱通路互相参照，不依赖演化后期偶然撒出跨半球连接

##### E. Cross 桥接（40 条，sensory Out → motor Proc）

- 每条 sensory Out 随机连 2 个 motor Proc（共 8 个，均匀分布）
- 权重 [-1.0, 1.0]

##### 边数合计

28 + 76 + 30 + 20 + 40 ≈ **194 条**（MAX_CONNS=512，充足）

#### 2.4 路径深度 & 信号衰减验算

主路径：input → Proc → Out(sensory) → Proc(motor, cross) → Out → output = **5 跳**

跨半球支路（辅助，非必经）：Proc(L) → Out(L) → Proc(R, 跨半球) → Out(R) → ... = +2 跳绕行

**fan-in 信号累积**（与 daisy chain 本质不同）：
- 每个 sensory Out 从 ~2 个 Proc 收信号 + 可选从对侧跨半球收 ~1 条 → 累积放大
- 每个 motor Proc 从 ~5 个 sensory Out 收 cross 信号（20×2/8=5）→ 大幅累积
- 每个 motor Out 从 ~2 个 motor Proc 收信号 → 再次累积

**结论**：pass-through 网络中信号不衰减反而累积，5 跳主路径输出远强于当前 3 跳 daisy chain。速度输出轻松过 0.05 阈值，初代 100% 能动。

#### 2.5 C2 影响

新设计下所有活跃 block 初始就有 Proc+Out 共存，C2 在 t=0 已满足。mutate_add_node 的 "90% 补齐缺失类型" 逻辑仅对新诞生 block（联合区 4~24）生效，不再浪费在修 baseline。演化可以把结构创新全部投入联合区扩张。

#### 2.6 实现步骤

1. 重写 `Genome::random_minimal()`：
   - 步骤 1-2：创建 Input/Output 节点（不变）
   - 步骤 3：Input 区 — 每 input 创建 Proc + Out，按 block 分组
   - 步骤 4：Output 区 — 每 output 创建 Out + Proc，按 block 分组
   - 步骤 5：B 类（Input 区内部随机连接）
   - 步骤 6：C 类（Output 区内部随机连接）
   - 步骤 7：D 类（跨半球同源连接）
   - 步骤 8：E 类（Cross 桥接）
   - 步骤 9：初始化 block_probs（不变）
2. 更新函数注释（节点数、边数、路径、C2 说明）
3. 同步 CLAUDE.md "初始大脑拓扑"段落
4. 同步 docs/DESIGN.md 对应章节
5. cargo fmt + cargo build 验证
6. 运行观察初代行为（能动、能吃、不死锁）
