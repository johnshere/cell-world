# SNN 推理路径 CPU/GPU 语义对齐方案

> 目标：让 CPU 和 GPU 两条 SNN 推理路径产出**逐 tick 完全等价**的状态演化（除 f64/f32 浮点精度差），存档可在两种后端间无缝切换。

---

## 1. 当前两条路径的差异（问题回顾）

### CPU 路径（`src/neural/spiking.rs`）

- 节点拓扑排序后**串行**执行 `tick_inner`
- 区分 `forward_inputs`（读当前 tick 已更新的节点）和 `recurrent_inputs`（读 `prev_state`，即上一 tick 末的状态）
- **正向连接零延迟**：一次 `tick(inputs)` 调用，input → block → output 整条前馈链路在同一 tick 内贯穿完毕
- `tick_multi` 直读输出取 **tick 0** 的值（此时拓扑序已贯穿，输出有意义）

### GPU 路径（`src/neural/snn_tick.wgsl` + `gpu.rs`）

- 所有节点**并行** dispatch，**全部从 `nodes_prev` 读**，不区分正向/回环
- 等价于"每条边硬延迟 1 tick"——一个 N 层网络需要 N 个 tick 才能让信号穿透
- 直读输出 first_outputs 在 **tick 0** 写入；但 tick 0 时输出层读到的 `nodes_prev` 是**上一批末尾的残影**，与本帧感知无关

### 后果

GPU 训练出的权重适配的是「每帧只前进一格 + 动作输出读上一帧残影」的延迟电路，并不是教科书上的 SNN。换到 CPU 会立刻崩溃。

---

## 2. 对齐策略：让 CPU 也变成水流式

把 CPU 的 `tick_inner` 改成 **"所有连接都从快照读"** —— 与 GPU shader 行为一致。

理由：
- GPU 端只需把 `first_outputs` 的 tick 0 守卫去掉（最小一处改动），让最后一 tick 的写入自然胜出
- CPU 端改造为"每 tick 开始时拍一张全网快照，所有连接读快照"
- 改完后两边都是同一个数学模型：**每条边 1 tick 延迟 + 取末次直读输出**

---

## 3. 具体改动清单

### 3.1 GPU shader：`src/neural/snn_tick.wgsl`

**唯一改动**：去掉直读输出的 tick 0 守卫，让每个 tick 都覆盖写 `first_outputs`，最后一次写入即末次值。

**位置**：`snn_tick.wgsl:208-218`

**改动**：
```wgsl
// 改前
if is_direct_read(next_node.flags) {
    if params.tick_index == 0u {
        let x = next_node.membrane;
        let x2 = x * x;
        first_outputs[out_global] = clamp(x * (27.0 + x2) / (27.0 + 9.0 * x2), -1.0, 1.0);
    }
}

// 改后
if is_direct_read(next_node.flags) {
    let x = next_node.membrane;
    let x2 = x * x;
    first_outputs[out_global] = clamp(x * (27.0 + x2) / (27.0 + 9.0 * x2), -1.0, 1.0);
}
```

**安全性**：
- 每个 `(creature_slot, output_idx)` 对应**唯一一个线程**写入，无 race
- `clear_counters` 已在每批开头清零 `first_outputs_buf`（`gpu.rs:751-756`）
- `spike_counts`（脉冲模式）路径完全不动

---

### 3.2 CPU 推理：`src/neural/spiking.rs`

#### 3.2.1 `prev_state` 字段语义扩展

**改前**：仅存"回环源节点"的 `(membrane, fired)`
**改后**：存**所有节点**的 `(membrane, fired)`

#### 3.2.2 `from_genome` 初始化

```rust
// 改前
let mut prev_state = FxHashMap::default();
for &src_id in &recurrent_source_ids {
    prev_state.insert(src_id, (0.0, false));
}

// 改后
let mut prev_state = FxHashMap::default();
for node in &genome.nodes {
    prev_state.insert(node.id, (0.0, false));
}
```

> `recurrent_source_ids` 临时变量保留亦可，不再实际使用，可以一并删除。

#### 3.2.3 `save_recurrent_state` 改名 + 全节点快照

```rust
// 改前
fn save_recurrent_state(&mut self) {
    for (&src_id, state) in self.prev_state.iter_mut() {
        if let Some(node) = self.nodes.get(&src_id) {
            *state = (node.membrane, node.fired);
        }
    }
}

// 改后
fn save_state_snapshot(&mut self) {
    for (&id, state) in self.prev_state.iter_mut() {
        if let Some(node) = self.nodes.get(&id) {
            *state = (node.membrane, node.fired);
        }
    }
}
```

> 函数体几乎没变，只是迭代范围语义从"回环源"扩展到"所有节点"。调用方 `tick` 和 `tick_free` 内的 `self.save_recurrent_state()` 改名即可。

#### 3.2.4 `tick_inner` 全水流化（核心改动）

```rust
fn tick_inner(&mut self) -> Vec<f64> {
    let eval_order = self.eval_order.clone();
    for &node_id in &eval_order {
        if self.input_ids_set.contains(&node_id) {
            continue;
        }

        let mut weighted_sum = 0.0;

        // 正向连接：改为读 prev_state（与回环统一）
        if let Some(inputs_list) = self.forward_inputs.get(&node_id) {
            for &(in_node, weight) in inputs_list {
                if let Some(&(prev_membrane, prev_fired)) = self.prev_state.get(&in_node) {
                    if prev_fired {
                        let threshold = self.nodes.get(&in_node).map(|n| n.threshold).unwrap_or(0.0);
                        if threshold == 0.0 {
                            weighted_sum += prev_membrane * weight;
                        } else {
                            weighted_sum += weight;
                        }
                    }
                }
            }
        }

        // 回环连接：本来就读 prev_state，逻辑不变
        if let Some(recurrent_list) = self.recurrent_inputs.get(&node_id) {
            for &(in_node, weight) in recurrent_list {
                if let Some(&(prev_membrane, prev_fired)) = self.prev_state.get(&in_node) {
                    if prev_fired {
                        let threshold = self.nodes.get(&in_node).map(|n| n.threshold).unwrap_or(0.0);
                        if threshold == 0.0 {
                            weighted_sum += prev_membrane * weight;
                        } else {
                            weighted_sum += weight;
                        }
                    }
                }
            }
        }

        // 节点状态更新部分（衰减 / 阈值 / 不应期）逻辑完全不变
        if let Some(node) = self.nodes.get_mut(&node_id) {
            // ... 与现有代码完全一致 ...
        }
    }

    // 输出收集：逻辑不变（仍然 tanh(membrane) 或 fired ? 1.0 : 0.0）
    self.output_ids.iter().zip(self.output_modes.iter()).map(...)...
}
```

**关键观察**：拓扑排序 `eval_order` 在水流模型下**不再必要**（顺序无关，因为大家都读 prev），但保留也无害——只是退化为任意顺序。改动最小化先保留。

> 进一步简化：可以把 `forward_inputs` 和 `recurrent_inputs` 合并成 `all_inputs`，因为水流模型下不再需要区分。这是后续清理任务，本次不做。

#### 3.2.5 `tick_multi` 改为取末次直读输出

```rust
pub fn tick_multi(&mut self, inputs: &[f64], ticks: usize) -> Vec<f64> {
    let n = self.output_ids.len().min(8);
    let mut spike_counts = [0u32; 8];

    // tick 0：注入输入
    let mut last_outputs = self.tick(inputs);
    for (j, (&v, &direct_read)) in last_outputs.iter().zip(self.output_modes.iter()).enumerate().take(n) {
        if !direct_read && v > 0.5 {
            spike_counts[j] += 1;
        }
    }

    // tick 1..N：保持输入，让信号继续往下游流
    for _ in 1..ticks {
        last_outputs = self.tick_free();
        for (j, (&v, &direct_read)) in last_outputs.iter().zip(self.output_modes.iter()).enumerate().take(n) {
            if !direct_read && v > 0.5 {
                spike_counts[j] += 1;
            }
        }
    }

    // 直读取末次值（last_outputs 已经是 tick N-1 的）；脉冲取发放率
    for (j, &direct_read) in self.output_modes.iter().enumerate().take(n) {
        if !direct_read && j < last_outputs.len() {
            let rate = spike_counts[j] as f64 / ticks.max(1) as f64;
            last_outputs[j] = rate * 2.0 - 1.0;
        }
    }

    last_outputs
}
```

#### 3.2.6 `update_eligibility_traces` 时序对齐

让 CPU 的 trace 更新也使用 `prev_state` 中的 fired（与 GPU shader 一致）：

```rust
fn update_eligibility_traces(&mut self) {
    let decay = self.learning_gene.eligibility_decay;

    // 收集所有连接键，避免双重借用
    let pairs: Vec<(usize, usize)> = self.forward_inputs.iter()
        .flat_map(|(&out, list)| list.iter().map(move |(in_n, _)| (*in_n, out)))
        .chain(self.recurrent_inputs.iter()
            .flat_map(|(&out, list)| list.iter().map(move |(in_n, _)| (*in_n, out))))
        .collect();

    for (in_node, out_node) in pairs {
        let pre_fired = self.prev_state.get(&in_node).map(|(_, f)| *f).unwrap_or(false);
        let post_fired = self.prev_state.get(&out_node).map(|(_, f)| *f).unwrap_or(false);

        let key = (in_node, out_node);
        let trace = self.eligibility_traces.entry(key).or_insert(0.0);
        *trace *= decay;
        if pre_fired && post_fired {
            *trace += 1.0;
        }
    }
}
```

> **副作用**：trace 反映"上一 tick 共激活"而非"当前 tick 共激活"。和 GPU 的（也是非因果的）行为一致。这条与"修复 trace 因果性"是另一个独立任务——本次只做对齐，不修 GPU 的 trace 语义。

---

### 3.3 CPU 执行器：`src/neural/thread.rs`

`CpuExecutor::run_batch` 抓取直读输出快照的时机改为**最后一 tick**（thread.rs:111）：

```rust
// 改前
if tick_idx == 0 && outputs.len() >= 8 {
    let mut arr = [0.0; 8];
    arr.copy_from_slice(&outputs[..8]);
    self.first_outputs.insert(id, arr);
}

// 改后
if tick_idx == n - 1 && outputs.len() >= 8 {
    let mut arr = [0.0; 8];
    arr.copy_from_slice(&outputs[..8]);
    self.first_outputs.insert(id, arr);
}
```

> 字段名 `first_outputs` 已经名实不符（语义变成"末次直读输出"），但本次不重命名，避免扩散 diff。后续单独清理时可改为 `last_direct_outputs` 或 `final_outputs`。

---

## 4. 改动清单总结

| 文件 | 改动点 | 行数估计 |
|------|--------|--------|
| `src/neural/snn_tick.wgsl` | 去掉 tick 0 守卫 | -3 行 |
| `src/neural/spiking.rs` | `from_genome` 初始化全节点 prev | +3 -3 |
| `src/neural/spiking.rs` | `save_recurrent_state` 改名 + 全节点快照 | 改名 + 注释 |
| `src/neural/spiking.rs` | `tick_inner` 正向连接改读 prev_state | ~10 行 |
| `src/neural/spiking.rs` | `tick_multi` 取末次直读输出 | ~10 行 |
| `src/neural/spiking.rs` | `update_eligibility_traces` 全 prev | ~15 行 |
| `src/neural/thread.rs` | `run_batch` 抓快照时机改末次 | -1 +1 |

总改动量：约 60 行有效改动。

---

## 5. 改完之后的语义保证

| 维度 | CPU | GPU |
|------|-----|-----|
| 一次 tick 内信号能走多远 | 1 层 | 1 层 |
| 10 tick 后信号传播深度 | 最多 10 层 | 最多 10 层 |
| 直读输出反映的状态 | 第 9 tick 的 membrane.tanh() | 第 9 tick 的 membrane fast-tanh approx |
| 跨帧状态保留 | 是（membrane / 不应期 / prev_state） | 是（nodes_buf_a/b / eligibility_traces） |
| trace 共激活判定 | prev tick 的 pre & post | prev tick 的 pre & post |

唯一保留的差异：
- **浮点精度**（CPU f64 vs GPU f32）
- **直读输出的 tanh 实现**（CPU 用 std::f64::tanh，GPU 用 Padé 近似 `x*(27+x²)/(27+9x²)`）—— 可统一，但本次不改

---

## 6. 验证方法

改完后做一次最小验证：

1. 启动同一个 seed 的世界，用同一份初始 genome
2. 跑 1000 update，对比 CPU 后端与 GPU 后端的：
   - 每只生物每帧 8 维输出向量
   - 期望：每个分量误差 < 1e-3（仅浮点精度差）
3. 载入旧存档：CPU/GPU 都崩（预期，因为旧权重适配的是旧延迟模型）
4. 从零演化新世界：CPU 与 GPU 应能跑出近似的群体行为分布

---

## 7. 不在本次改动范围内的事项（独立任务）

1. **trace 因果性修复**：让 trace 反映"上一 tick pre fired → 这一 tick post fired"的真因果对（需要分别记录 pre 和 post 的时态）
2. **GPU MAX_NODES=64 / MAX_CONNS=128 截断告警**：超出时打日志或拒绝注册
3. **GPU ping/pong 跨批次假设**：tick_count 必须为偶数才正确，加显式断言或修代码
4. **forward_inputs / recurrent_inputs 合并**：水流模型下不再需要区分，可统一为 `all_inputs`
5. **`first_outputs` 字段重命名**：`last_direct_outputs`
6. **CLAUDE.md / DESIGN.md 文档同步**：补充"水流式 SNN"语义说明

---

## 8. 已知风险

- **所有现有存档失效**：无论 CPU 还是 GPU 训练的，都不再适用。需要给用户明示这是一次"演化重置"。
- **CPU 推理性能基本不变**：原来 tick_multi 也跑 10 次扫描，只是后 9 次是 tick_free 空转；改后 10 次都做有效传播，但单次复杂度相同。
- **水流模型下网络深度上限被 tick_count 钳制**：N=10 ticks 意味着信号最多传 10 层。如果未来希望支持更深网络，需要相应增加 tick_count（同时承担 SNN 时间分辨率与网络深度的耦合代价）。

---

## 附：实施顺序建议

1. 先单独改 GPU shader 一处，跑通编译，确认 GPU 行为没崩
2. 再改 CPU `tick_inner` + `tick_multi` + `update_eligibility_traces`
3. 改 `thread.rs` 的 `CpuExecutor::run_batch`
4. 改 `prev_state` 初始化和快照范围
5. 跑对比实验验证两边输出向量一致
6. 文档同步（CLAUDE.md / DESIGN.md）
