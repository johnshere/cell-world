1. 面板中能量、配置两个tab下现有的配置输入框后在增加一个副本，该副本用于指定现实时间24小时后的参数变化目标。需要一个方案，随演化时间增长，设定演化参数变化
2. ✅ 已过时，删除。
3. ✅ 已实现。基因库列表按 recorded_at 倒序排列（最新在前），store.rs 的 load_all 和 save 两处排序均已更新
4. ✅ 已分析，无问题。模拟时间按实际倍率推进，非目标倍率。
   - world.time / volcano_timer / creature.age 等全部在 world.update(dt) 内推进，dt 恒为 SIM_DT=1/30
   - SNN 推进（bridge.run_batch_sync）是 world.update 内的阻塞调用，SNN 没跑完 time 不会前进，两者严格同步
   - 加速的实现是"内层批量"：每次 loop 迭代运行 speed 次 update，而非加快 loop 本身频率
   - target=10x 实际 5x 时：loop 从 30次/秒 降到 15次/秒，每次仍跑 10 个 update，总计 150 update/秒 = 5x，world.time 同步慢下来
   - 与"加大外层步进"的心理模型等价（乘法交换律），工程上选内层批量是因为不依赖 OS 调度精度
5. 当前block编号区分左右脑的形式比较取巧，同时也带来一个问题，不应归属左或右脑区的不好控制，只有一个0编号可用无法扩充。给个建议，只分析
6. 增加区块选中功能，选中后可查看地图地形中区块相关信息，两部分：本区块信息、全局信息（最高最低等等有用的）
7. ✅ 已实现。删除温泉机制，增加熔岩流机制：
   - 每次火山喷发附带 lava_count 个熔岩流粒子（火山口附近小范围）
   - 熔岩粒子自然衰减死亡时链式扩散 lava_spread_count 个子粒子（被吃不扩散）
   - 杀伤半径 = volcano_kill_radius × max(0, 2×(1-dist/radius))，火山口2倍→边缘归零
   - 扩散方向受地形引导（下坡优先，lava_terrain_bias 控制偏好强度）
   - 子粒子能量/衰减率复用火山配置，最大链式 lava_max_chain_depth 代
