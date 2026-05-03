# Cell-World MCP Server

## 概述

cell-world 内嵌 MCP（Model Context Protocol）服务器，通过 SSE 传输向 Claude Code 暴露模拟运行时数据。

**端口**: `localhost:9877`

**启动方式**:
```bash
# MCP 模式（headless，无 GUI）
cargo run -- --mcp

# 自定义端口
cargo run -- --mcp --mcp-port 9877

# 正常 GUI 模式（不启动 MCP）
cargo run
```

---

## Claude Code 集成配置

在 `.claude/settings.json` 中添加：

```json
{
  "mcp_servers": {
    "cell-world": {
      "type": "sse",
      "url": "http://localhost:9877/sse"
    }
  }
}
```

---

## 使用模式

### 推荐工作流

```
1. set_paused(true)        → 冻结世界，确保多次查询结果一致
2. get_stats()              → 概览种群状态
3. get_creatures(...)       → 按条件筛选生物
4. get_creature(id)         → 查看目标生物的基因组
5. get_creatures_nearby(...)→ 分析某生物附近的生态
6. set_paused(false)        → 恢复演化
```

> **重要**: 所有查询工具读取的都是**当前帧快照**。不暂停的情况下多次调用可能看到不同帧的数据。

---

## 工具（Tools）

### `get_stats` — 种群统计

无需参数。

**返回值**:

| 字段 | 类型 | 说明 |
|------|------|------|
| `time` | f64 | 模拟时间（秒） |
| `creature_count` | usize | 存活生物总数 |
| `energy_particle_count` | usize | 能量粒子总数 |
| `trail_count` | usize | 痕迹点总数 |
| `total_energy` | f64 | 粒子总能量 |
| `creature_energy` | f64 | 生物总能量 |
| `theoretical_energy` | f64 | 历史投放能量总量 |
| `max_generation` | usize | 最大世代数 |
| `avg_energy` | f64 | 生物平均能量 |
| `clan_count` | usize | 族群数量 |
| `top_clans` | `[{hash, count, ratio}]` | 前 5 大族群 |
| `dominant_species` | `{...} | null` | 优势种（详见下方） |
| `death_age_stats` | `{count, avg, median, max, min}` | 死亡年龄统计 |
| `action_counts` | `[移动, 吸收, 咬, 无性繁殖, 有性繁殖]` | 行为计数 |
| `reward_counts` | `[能量, 痕迹, 集体]` | 奖励计数 |
| `volcano_countdown` | f64 | 下次喷发倒计时（秒） |
| `paused` | bool | 模拟是否暂停 |

`dominant_species` 字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `genome_hash` | u64 | 优势种基因哈希 |
| `avg_energy` | f64 | 平均能量 |
| `avg_age` | f64 | 平均年龄 |
| `max_generation` | usize | 最大世代 |
| `population_ratio` | f64 | 种群占比（0~1） |
| `score` | f64 | 竞争力评分 |

---

### `get_performance` — 性能分解

无需参数。

**返回值**:

| 字段 | 类型 | 说明 |
|------|------|------|
| `perceive_ms` | f64 | 感知阶段耗时（ms） |
| `snn_ms` | f64 | SNN 推理耗时（ms） |
| `actions_ms` | f64 | 动作执行耗时（ms） |
| `spatial_ms` | f64 | 空间索引耗时（ms） |
| `total_ms` | f64 | 总耗时（ms） |
| `creature_count` | usize | 本次 update 处理的生物数 |
| `avg_compute_ns` | f64 | 生物平均计算耗时（ns） |

---

### `get_creatures` — 查询生物（核心工具，支持筛选+排序+分页）

**参数**（全部可选）：

#### 空间筛选

| 参数 | 类型 | 说明 |
|------|------|------|
| `x_min` | f64? | 矩形范围左边界 |
| `x_max` | f64? | 矩形范围右边界 |
| `y_min` | f64? | 矩形范围下边界 |
| `y_max` | f64? | 矩形范围上边界 |
| `center_x` | f64? | 圆形范围圆心 x（优先于矩形） |
| `center_y` | f64? | 圆形范围圆心 y |
| `radius` | f64? | 圆形范围半径 |

#### 属性筛选

| 参数 | 类型 | 说明 |
|------|------|------|
| `clan_hash` | u64? | 按族群哈希精确匹配 |
| `min_energy` | f64? | 最小能量 |
| `max_energy` | f64? | 最大能量 |
| `min_age` | f64? | 最小年龄（秒） |
| `max_age` | f64? | 最大年龄（秒） |
| `min_generation` | usize? | 最小世代 |
| `max_generation` | usize? | 最大世代 |
| `min_speed` | f64? | 最小当前速度 |
| `max_speed` | f64? | 最大当前速度 |
| `min_follow` | f64? | 最小跟随度（0~1） |
| `max_follow` | f64? | 最大跟随度（0~1） |
| `min_light` | f64? | 最小发光强度（0~1） |
| `max_light` | f64? | 最大发光强度（0~1） |
| `min_nodes` | usize? | 最小神经网络节点数 |
| `max_nodes` | usize? | 最大神经网络节点数 |
| `min_connections` | usize? | 最小突触连接数 |
| `max_connections` | usize? | 最大突触连接数 |
| `alive_only` | bool | 默认 `true`，只查存活生物 |

#### 排序与分页

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `sort_by` | string | `"id"` | `"id"` / `"energy"` / `"age"` / `"generation"` / `"speed"` |
| `sort_desc` | bool | `false` | 是否降序 |
| `page` | u32 | `0` | 页码（从 0 开始） |
| `page_size` | u32 | `20` | 每页条数（最大 200） |

**返回值**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `items` | `[CreatureSummary]` | 生物摘要列表 |
| `total_count` | usize | 符合条件的总数 |
| `page` | u32 | 当前页码 |
| `page_size` | u32 | 页大小 |
| `total_pages` | u32 | 总页数 |

`CreatureSummary` 字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | u64 | 唯一标识 |
| `x` | f64 | 坐标 x |
| `y` | f64 | 坐标 y |
| `energy` | f64 | 当前能量 |
| `age` | f64 | 年龄（秒） |
| `heading` | f64 | 朝向（弧度，0=右，π/2=下） |
| `generation` | usize | 世代 |
| `clan_hash` | u64 | 族群哈希 |
| `current_speed` | f64 | 当前速度 |
| `follow_level` | f64 | 跟随度（0~1） |
| `light_intensity` | f64 | 发光强度（0~1） |
| `node_count` | usize | 神经网络节点数 |
| `connection_count` | usize | 突触连接数 |
| `last_outputs` | `[f64; 8]` | 8 维 SNN 输出 |

---

### `get_creature` — 单生物完整详情

**参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | u64 | 是 | 生物 ID |

**返回值**：CreatureSummary 的所有字段 + 以下扩展字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `parent_id` | u64? | 父生物 ID |
| `heading_persist` | f64 | 朝向 EWMA 模长（∈[0,1]，直走→1，转向/绕圈→<1） |
| `perception_cache` | `[f64; 20]` | 20 维感知输入 |
| `physio` | `{pleasure_energy, pleasure_trail, pleasure_group}` | 本帧生理奖励 |
| `mouth_cooldown_timer` | f64 | 嘴巴冷却计时 |
| `genome` | `GenomeDetail` | 完整基因组结构 |

`GenomeDetail` 字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `hash` | u64 | 基因组哈希 |
| `nodes` | `[{id, neuron_type, block_id, activation, bias, decay, response, ...}]` | 节点列表 |
| `connections` | `[{in_node, out_node, weight, enabled, innov}]` | 连接列表 |
| `physio` | `{pleasure_energy_sensitivity, pleasure_trail_sensitivity, pleasure_group_sensitivity}` | 生理敏感度基因 |
| `conn_probs` | `{key: {source_pct, target_pref, ...}}` | 区块连接概率基因 |
| `maturation_time` | f64 | 发育时间基因 |

---

### `get_neighbors` — 生物附近的其他生物（高频快捷查询）

**参数**：

| 参数 | 类型 | 必填 | 默认 | 说明 |
|------|------|------|------|------|
| `creature_id` | u64 | 是 | — | 目标生物 ID |
| `radius` | f64 | 否 | `100.0` | 搜索半径（px） |
| `page` | u32 | 否 | `0` | 页码 |
| `page_size` | u32 | 否 | `20` | 每页条数（最大 200） |

**返回值**：同 `get_creatures` 的分页结构。

---

### `get_energy_particles` — 查询能量粒子

**参数**（全部可选）：

| 参数 | 类型 | 说明 |
|------|------|------|
| `x_min`, `x_max`, `y_min`, `y_max` | f64? | 矩形范围 |
| `center_x`, `center_y`, `radius` | f64? | 圆形范围（优先于矩形） |
| `lava_only` | bool | 是否只查熔岩粒子，默认 `false` |
| `min_energy`, `max_energy` | f64? | 能量范围 |
| `page` | u32 | 默认 `0` |
| `page_size` | u32 | 默认 `20`，最大 `200` |

**返回值**：`{items: [{id, x, y, energy, initial_energy, lava, chain_depth, source}], total_count, page, page_size, total_pages}`

---

### `get_trails` — 查询痕迹点

**参数**（全部可选）：

| 参数 | 类型 | 说明 |
|------|------|------|
| `x_min`, `x_max`, `y_min`, `y_max` | f64? | 矩形范围 |
| `center_x`, `center_y`, `radius` | f64? | 圆形范围 |
| `clan_hash` | u64? | 按族群过滤 |
| `min_energy`, `max_energy` | f64? | 能量范围 |
| `page`, `page_size` | u32 | 分页 |

**返回值**：`{items: [{x, y, energy, initial_energy, clan_hash, creator_id, age}], ...分页}`

---

### `get_clans` — 族群分布详情

无需参数。

**返回值**：

```json
[{
  "hash": 12345,
  "count": 42,
  "ratio": 0.35,
  "avg_energy": 150.0,
  "avg_age": 45.2,
  "max_generation": 12,
  "representative_id": 1001
}]
```

按数量降序排列。

---

### `get_dominant_species` — 优势种详情

无需参数。若无优势种返回 `null`。

**返回值**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `genome_hash` | u64 | 基因哈希 |
| `avg_energy` | f64 | 平均能量 |
| `avg_age` | f64 | 平均年龄 |
| `max_generation` | usize | 最大世代 |
| `population_ratio` | f64 | 种群占比 |
| `score` | f64 | 竞争力评分 |
| `genome` | `GenomeDetail` | 代表基因组（同 `get_creature` 的 genome） |

---

### `get_config` — 当前配置

无需参数。

**返回值**：`config.toml` 全部字段（详见 `src/config.rs` 的 `Config` 结构体）。

---

### `get_terrain_info` — 地形信息

**参数**（全部可选）：

| 参数 | 类型 | 说明 |
|------|------|------|
| `x` | f64? | 查询点 x 坐标 |
| `y` | f64? | 查询点 y 坐标 |

**返回值**（无坐标时）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `is_generated` | bool | 地形是否已生成 |
| `width` | usize | 宽度（chunks） |
| `height` | usize | 高度（chunks） |
| `min_height` | i32 | 最低点高度 |
| `max_height` | i32 | 最高点高度 |
| `volcano_x` | f64 | 火山口 x |
| `volcano_y` | f64 | 火山口 y |

有坐标时额外返回：

| 字段 | 类型 | 说明 |
|------|------|------|
| `height_at_point` | i32 | 该坐标地形高度 |

---

### `set_paused` — 暂停/继续模拟

**参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `paused` | bool | 是 | `true`=暂停，`false`=继续 |

**返回值**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `ok` | bool | 是否成功 |
| `was_paused` | bool | 操作前的暂停状态 |
| `time` | f64 | 当前模拟时间 |

> 暂停后 sim 线程停止更新，`SimSnapshot` 冻结。恢复后从断点继续。

---

## 资源（Resources）

| URI | 等价工具 |
|-----|---------|
| `cellworld://stats` | `get_stats` |
| `cellworld://clans` | `get_clans` |
| `cellworld://config` | `get_config` |
| `cellworld://performance` | `get_performance` |
| `cellworld://dominant` | `get_dominant_species` |
| `cellworld://terrain` | `get_terrain_info`（无坐标） |

---

## 示例查询

### 1. 暂停模拟并查看状态

```
set_paused({ paused: true })
→ { ok: true, was_paused: false, time: 1250.3 }
```

### 2. 查看所有高能量生物

```
get_creatures({
  min_energy: 200,
  sort_by: "energy",
  sort_desc: true,
  page_size: 10
})
```

### 3. 查看某生物附近的生物

```
get_neighbors({
  creature_id: 42,
  radius: 150.0
})
```

### 4. 矩形区域内的生物

```
get_creatures({
  x_min: -200, x_max: 200,
  y_min: -200, y_max: 200,
  page_size: 50
})
```

### 5. 分析复杂生物（神经网络结构）

```
get_creatures({
  min_nodes: 20,
  min_connections: 50,
  sort_by: "energy",
  sort_desc: true
})
```

### 6. 查看优势种基因组

```
get_dominant_species()
```

### 7. 查看能量粒子分布

```
get_energy_particles({
  center_x: 0, center_y: 0,
  radius: 300,
  lava_only: true
})
```

### 8. 恢复模拟

```
set_paused({ paused: false })
```
