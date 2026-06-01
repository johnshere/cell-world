# 版本 2.4.1 更新

> 不考虑数据迁移，本版本生效后从零开始演化。
> （DESIGN.md 已有 v2.4 同步批处理里程碑，本次以小号 v2.4.1 区分；用户口头沿用"v2.4"作为简称）

1、基因库的列表 checkbox 选中后，会定时投放，投放逻辑删除；选中 checkbox 在自动记录的模板不显示，改为显示；低于最低生命数量会自动投放，不再从自动记录中投放，改为从选中的投放。即两者结合取消定时投放，改为最低数量时投放选中模板 — **pending**

2、鼠标滑到力导图连线时，应该显示线的信息 — **pending**

3、修复 Output 接收侧硬约束（架构 bug）— ✅ **已完成**
- ✅ `mutate_add_connection` 把 `is_motor(from_blk)` 改为 `from_blk == motor_block_for_output(out_idx)`
- ✅ 三处 push 前加 `debug_assert_valid_io_edge`（release 零开销）
- ✅ Input 侧硬约束沿用现有 `from_blk == sensory_block_for_input(input_id)`，断言一并覆盖
- 不需要清理历史违规（重新演化）

4、初始生命神经网络重构 — ✅ **已完成（三次修订为 56 节点 + 68 边预制骨架）**
- ✅ `random_minimal()`（无参数）56 节点拓扑（2n）：
  - 20 Input + 8 Output + 20 input 独占 Proc + 8 output 独占 Out = 56
  - **取消反向类型补齐**：sensory block 初始只有 Proc / motor block 初始只有 Out，C2 由演化在 mutate_add_node 阶段补齐
- ✅ 68 条预制必要边（全权重 `[-1.0, 1.0]`，演化基线强信号）：
  - 20 条 input → 独占 Proc（I/O 硬约束）
  - 8 条 独占 Out → output（I/O 硬约束）
  - 40 条 **C 方案 cross 边**：每 motor Out × 每 sensory block 选 1 随机 Proc 连边
- ✅ 3 跳路径 `input → 独占 Proc → motor Out → output` 在 t=0 就连通，0.5³ × max_speed=2.5 直接突破运动阈值 0.05，初代 **100% 能动**
- ✅ 删除全部连接密度配置：`initial_connections_min/max`、`initial_connection_ratio` 全清；初始拓扑由代码完全确定

5、随机额外连接 — ❌ **方案废弃**
- 原方案"`n × ratio` 撒随机额外边"被 C 方案预制 40 条 cross 边替代
- 理由：随机撒边 P(能动) 极低（ratio=0.2 → 7%，ratio=2 → 49%）；C 方案确定性预制 → 100% 能动
- `mutate_add_connection(_, Some(10))` 函数保留，但不再被 random_minimal 调用；仅作为演化通路的预留接口

## 附加排查（版本 2.4.1 守门）

### 硬约束守门（input 专属感官块 / output 专属 motor 块）— ✅ **已完成**
- ✅ 通过 `debug_assert_valid_io_edge` 强制守门，覆盖以下入口的 push 点：
  - `mutate_add_connection` Input 路径
  - `mutate_add_connection` Block 源路径
  - `random_minimal` 间接通过 `mutate_add_connection` 注入
- 其他写入入口结论：
  - `random_minimal` 本体的 I/O 必要连接（input→独占 Proc / 独占 Out→output / block 内部线）：拓扑构造时按硬约束直接生成，不经过随机采样路径，不会违规
  - `mutate_add_node` 仅做"已有连接 A→B 中插入 X"的中性变异（A→X=权重1, X→B=原权重），不创造跨 I/O 端点的新边
  - `crossover` 走 NEAT 标准 disjoint/excess，从父代取整条连接，原本就合规；若父代合规则子代必合规

### ConnProbs / target_pref 守门 — ✅ **已完成**
- `mutate_add_connection` 已走 `conn_target` + `matches_conn_target` + `target_pref` 加权 + 新加 C3 软偏好（跨 block Proc 候选 ×3）
- `mutate_add_node` 的 X block 选择走 `from_pref.target_pref.get(out_blk)`，已合规
- 没有其他独立写一套的路径

## 三条永久软约束（替代旧的临时脚手架）

| 约束 | 含义 | 落点 | 开关 |
|------|------|------|------|
| **C1** | 节点活跃连接 ≤10 | `passes_c1_cap()` + `mutate_add_connection(_, Option<usize>)` | **当前 dormant**（C 方案预制骨架后 random_minimal 不再调用 Some）；演化期 `None` 完全不限；机制保留供未来用 |
| **C2** | block 应有 Proc+Out 共存 | `mutate_add_node` 检测缺失类型，90% 概率补齐 | **仅在 add_node 触发时生效，初始 random_minimal 不预制** |
| **C3** | 跨 block 连接 Proc 目标 ×3 | `mutate_add_connection` 加权采样阶段 | 永久 |

## 双重硬约束（block + layer，由 debug_assert 守门）

| 端点 | 约束 | 由谁守门 |
|------|------|----------|
| Input 源 | 目标必须 `block == sensory_block_for_input(input_id) && layer == Processing` | `mutate_add_connection` Input 分支 filter + `debug_assert_valid_io_edge` |
| Output 目标 | 源必须 `block == motor_block_for_output(out_idx) && layer == Output` | `mutate_add_connection` Block 源分支 filter + `debug_assert_valid_io_edge` |

在 2n 初始拓扑下，sensory block 只有 Proc / motor block 只有 Out，layer 硬约束初始天然满足；当演化通过 `mutate_add_node` 给 sensory block 加 Out 节点后，layer 硬约束防止 input 错连到新 Out。
