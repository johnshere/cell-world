# 版本 2.4 更新

> 不考虑数据迁移，本版本生效后从零开始演化。

1、基因库的列表 checkbox 选中后，会定时投放，投放逻辑删除；选中 checkbox 在自动记录的模板不显示，改为显示；低于最低生命数量会自动投放，不再从自动记录中投放，改为从选中的投放。即两者结合取消定时投放，改为最低数量时投放选中模板

2、鼠标滑到力导图连线时，应该显示线的信息

3、修复 Output 接收侧硬约束（架构 bug）
- 现象：b25 motor 节点会连到 output 27（光嘴）、output 23（繁殖），违反"每个 output 只接收其专属 motor 块"的架构语义
- 根因：`mutate_add_connection` 的硬约束写松了——`is_motor(from_blk)` 只校验 `|b|≥25`，对 b25 / b-25 / b-26 一视同仁
- 修复点：
  - `genome.rs:1028-1032` 把 `is_motor(from_blk)` 改为 `from_blk == motor_block_for_output(out_idx)`，与 Input 侧（`from_blk == sensory_block_for_input(input_id)`）对称
  - 在 `connections.push` 前加 `debug_assert!` 校验两条硬约束（input→专属感官块、专属 motor 块→output），下次再有遗漏立即崩
  - 同步排查全代码库其他可能写入 connections 的路径是否也遵守该硬约束（见本版本"附加排查"）
- 不需要清理历史违规（重新演化）

4、初始生命神经网络重构
- 现在初始生命的节点数是 36，且包含 input、output；改为数量不定，节点初始数量不确定，由 input、output 数量衍生
- 每个 input、output 都至少连接一个节点（目标区域的），不允许 input 直连 output；且 input 初始连接 proc 节点，output 初始连接 out 节点，如此节点数就是 输入输出节点数*2
- 初始每个 block 在连接 input、output 之后，只有单一的 proc 或 out 节点；再补全另一类型节点，数量相等；此时节点数就是 输入输出节点数*3
- UI 面板中关于神经网络初始连接的配置删除，改为初始连接数与输入输出节点数的倍率 n，n 可以是一位小数，运算时取整

5、跟进配置给初始生命神经网络的节点随机连线
- 在第4项骨架基础上，按 `n × (INPUT_SIZE + OUTPUT_SIZE)` 取整作为额外随机边数
- 候选边池排除 "Input→Output 直连"，遵守 ConnProbs 5 方向 + target_pref 加权采样（与 mutate_add_connection 同一套规则）
- 新边权重用小扰动 `[-0.1, 0.1]`，与变异保持一致

## 附加排查（版本 2.4 守门）

### 硬约束守门（input 专属感官块 / output 专属 motor 块）
- 全代码库所有写入 `genome.connections` 的入口都必须强制：
  - `from = Input` ⇒ `to.block == sensory_block_for_input(from.id)`
  - `to = Output` ⇒ `from.block == motor_block_for_output(to.id - INPUT_SIZE)`
- 已知入口：`random_minimal`、`mutate_add_connection`、`mutate_add_node`、`crossover`
- 见"附加排查报告"（待补充）逐一核对

### ConnProbs / target_pref 守门
- 所有"按拓扑方向随机选目标节点"的路径都应走 `conn_target` + `matches_conn_target` + `target_pref` 加权，而不是独立写一套
- 已知入口：`mutate_add_connection`、`mutate_add_node`（X 的 block 选择）
- 见"附加排查报告"（待补充）
