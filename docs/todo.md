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

4、初始生命神经网络重构 — ✅ **已完成**
- ✅ `random_minimal(initial_connection_ratio: f64)` 84 节点拓扑：20 Input + 8 Output + 20×2 input 侧（独占 Proc + 配对反向 Out）+ 8×2 output 侧（独占 Out + 配对反向 Proc）= 84
- ✅ Input 不再直连 Output，必经 block 内独占节点
- ✅ UI 面板和 `config.toml` 改为 `initial_connection_ratio`，删除 `initial_connections_min/max`

5、随机额外连接 — ✅ **已完成**
- ✅ 额外边数 = `(INPUT_SIZE + OUTPUT_SIZE) × initial_connection_ratio` 四舍五入
- ✅ 复用 `mutate_add_connection(_, Some(10))`，自动遵守 ConnProbs/target_pref + C1≤10 + 硬约束
- ✅ 新边权重 `[-0.1, 0.1]` 小扰动

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
| **C1** | 节点活跃连接 ≤10 | `passes_c1_cap()` + `mutate_add_connection(_, Option<usize>)` | 仅 `random_minimal` 启用 `Some(10)`，演化期 `None` 完全不限 |
| **C2** | block 内 Proc+Out 同时存在 | `mutate_add_node` 检测缺失类型，90% 概率补齐 | 永久 |
| **C3** | 跨 block 连接 Proc 目标 ×3 | `mutate_add_connection` 加权采样阶段 | 永久 |
