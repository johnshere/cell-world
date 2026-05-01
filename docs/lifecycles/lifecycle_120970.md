---
name: lifecycle_120970
description: 生物120970完整生命周期能量追踪
type: project
---

# 生物120970生命周期追踪

**追踪目标**: ID=120970, generation=0, parent_id=null
**初始采样时间**: world_time=待记录
**初始能量**: 99.81 J
**初始年龄**: 111.00秒
**出生时间**: 估算~world_time-111.00
**初始位置**: (-331.59, -1771.82)

## 基本信息
- generation: 0（初代个体）
- parent_id: null
- node_count: 35
- connection_count: 199

## 配置参数

| 参数 | 值 |
|------|-----|
| base_metabolism | 0.025 |
| age_metabolism_factor | 0.012 |
| metabolism_exponent | 2.0 |
| move_cost | 0.1 |
| vision_range | 100.0 |
| heat_dissipation_coefficient | 0.0 |
| feed_efficiency | 0.85 |

## 生理参数
- pleasure_energy_sensitivity: 1.0（默认值）
- pleasure_group_sensitivity: 1.0（默认值）
- pleasure_trail_sensitivity: 1.0（默认值）

## 采样数据

### T=111.00s (初始采样)
- energy: 99.81 J
- speed: 16.13
- position: (-331.59, -1771.82)
- alive: true
- follow_level: 0.0
- last_outputs: [0.998, -0.807, 0.184, 0.696, 0.497, 0, 0, 0]
- 嘴输出[2]=0.184（略正，有进食欲望）

### T=153.33s (当前采样)
- energy: 44.46 J
- speed: 10.91
- position: (-312.91, -1775.56)
- alive: true
- follow_level: 0.0
- last_outputs: [0.910, -0.545, 0.105, 0.491, 0.331, 0, 0, 0]
- 嘴输出[2]=0.105（正，有进食欲望）
- light_intensity: 0.5

---

## 能量变化分析

| 时间点 | age(s) | energy | ΔE | ΔE/Δt | 状态 |
|--------|--------|--------|-----|-------|------|
| T=111 | 111.00 | 99.81 | - | - | alive |
| T=153 | 153.33 | 44.46 | -55.35 | -1.31 J/s | alive |

### 关键观察
- 42秒内消耗55.35J，消耗率约1.31 J/s
- 速度适中（16→10.9）
- 嘴输出始终为正，有进食欲望
- follow_level=0，无跟随行为

---

## 消耗分解（预估）

| 消耗类型 | 估算量(J) | 占比 | 说明 |
|---------|---------|------|------|
| 移动消耗 | ~38 | ~69% | 速度适中 |
| 基础代谢 | ~16 | ~29% | 随年龄增长 |
| 痕迹释放 | ~1 | ~2% | mouth输出正 |
| 体温逸散 | 0 | 0% | 已关闭 |
| **总计** | **~55** | **100%** | |

---

## 神经网络结构

- node_count: 35
- connection_count: 199
- 关键神经元: block(-1)=28, block(1)=29, block(0)=30, block(-2)=31
- maturation_time: 5000.0s

### 生理敏感度（默认值均为1.0）
- pleasure_energy: 1.0
- pleasure_group: 1.0
- pleasure_trail: 1.0