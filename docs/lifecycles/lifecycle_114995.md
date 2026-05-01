---
name: lifecycle_114995
description: 生物114995完整生命周期能量追踪（100秒以上存活个体）
type: project
---

# 生物114995生命周期追踪

**追踪目标**: ID=114995, generation=0, parent_id=null
**初始采样时间**: world_time=待记录
**初始能量**: 89.47 J (age~101.23s)
**初始年龄**: 101.23秒
**出生时间**: 估算~world_time-101.23
**初始位置**: (-1332.77, 239.23)

## 基本信息
- generation: 0（初代个体）
- parent_id: null
- node_count: 35
- connection_count: 188

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

### T=101.23s (初始采样)
- energy: 89.47 J
- speed: 8.15
- position: (-1332.77, 239.23)
- alive: true
- follow_level: 0.024
- last_outputs: [0.609, -0.407, -0.192, 0.096, 0.301, 0, 0, 0]
- 嘴输出[2]=-0.192（略负，不主动进食）

### T=128.23s (当前采样)
- energy: 44.92 J
- speed: 18.52
- position: (-1307.37, 260.17)
- alive: true
- follow_level: 0.024
- last_outputs: [0.997, -0.926, -0.593, 0.226, 0.640, 0, 0, 0]
- 嘴输出[2]=-0.593（负，不进食）
- light_intensity: 0.5

---

## 能量变化分析

| 时间点 | age(s) | energy | ΔE | ΔE/Δt | 状态 |
|--------|--------|--------|-----|-------|------|
| T=101 | 101.23 | 89.47 | - | - | alive |
| T=128 | 128.23 | 44.92 | -44.55 | -1.65 J/s | alive |

### 关键观察
- 27秒内消耗44.55J，消耗率约1.65 J/s
- 速度从8.15升至18.52（高速移动）
- 嘴输出始终为负，不主动进食
- 位置变化约25px，速度较高

---

## 消耗分解（预估）

| 消耗类型 | 估算量(J) | 占比 | 说明 |
|---------|---------|------|------|
| 移动消耗 | ~35 | ~79% | 速度18.52，27秒位移约25px |
| 基础代谢 | ~9 | ~20% | 随年龄增长 |
| 痕迹释放 | ~0.5 | ~1% | mouth负值，无主动释放 |
| 体温逸散 | 0 | 0% | 已关闭 |
| **总计** | **~44.5** | **100%** | |

---

## 神经网络结构

- node_count: 35
- connection_count: 188
- 关键神经元: block(-1)=28, block(1)=29, block(0)=30, block(-2)=31
- maturation_time: 5000.0s

### 生理敏感度（默认值均为1.0）
- pleasure_energy: 1.0
- pleasure_group: 1.0  
- pleasure_trail: 1.0