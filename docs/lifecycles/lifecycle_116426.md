---
name: lifecycle_116426
description: 生物116426完整生命周期能量追踪（高世代个体）
type: project
---

# 生物116426生命周期追踪

**追踪目标**: ID=116426, generation=108, parent_id=?
**初始采样时间**: world_time=待记录
**初始能量**: 19.75 J (非常低!)
**初始年龄**: 100.90秒
**出生时间**: 估算~world_time-100.90
**初始位置**: (-1566.48, -451.31)

## 基本信息
- generation: 108（高世代个体）
- node_count: 46
- connection_count: 226

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

### T=100.90s (初始采样)
- energy: 19.75 J（极低！濒死状态）
- speed: 20.0（最大速度）
- position: (-1566.48, -451.31)
- alive: true
- follow_level: 0.334
- last_outputs: [-0.065, -1.0, -0.448, -1.0, -1.0, -0.745, 0, 0]
- 嘴输出[2]=-0.448（负，不进食）
- light_intensity: 0.5
- generation: 108（高世代）

### 分析
- **能量极低（19.75J）**，远低于一般个体的50-100J
- **高速移动（20.0）**，持续消耗能量
- **嘴输出负值**，不进食
- 可能是濒死状态的老年个体

---

## 能量变化分析

| 时间点 | age(s) | energy | ΔE | ΔE/Δt | 状态 |
|--------|--------|--------|-----|-------|------|
| T=100 | 100.90 | 19.75 | - | - | alive（濒死） |

### 关键观察
- 能量仅19.75J，属于极低水平
- 高速移动(20.0)加速能量消耗
- 108代个体，神经网络高度复杂化
- 可能是自然衰老死亡

---

## 死亡记录

（待补充 - 生物可能很快死亡）