1. 面板中能量、配置两个tab下现有的配置输入框后在增加一个副本，该副本用于指定现实时间24小时后的参数变化目标。需要一个方案，随演化时间增长，设定演化参数变化
2. ✅ 已过时，删除。
3. ✅ 已实现。基因库列表按 recorded_at 倒序排列（最新在前），store.rs 的 load_all 和 save 两处排序均已更新
4. ✅ 已分析，无问题。模拟时间按实际倍率推进，非目标倍率。
   - world.time / volcano_timer / creature.age 等全部在 world.update(dt) 内推进，dt 恒为 SIM_DT=1/30
   - SNN 推进（bridge.run_batch_sync）是 world.update 内的阻塞调用，SNN 没跑完 time 不会前进，两者严格同步
   - 加速的实现是"内层批量"：每次 loop 迭代运行 speed 次 update，而非加快 loop 本身频率
   - target=10x 实际 5x 时：loop 从 30次/秒 降到 15次/秒，每次仍跑 10 个 update，总计 150 update/秒 = 5x，world.time 同步慢下来
   - 与"加大外层步进"的心理模型等价（乘法交换律），工程上选内层批量是因为不依赖 OS 调度精度
