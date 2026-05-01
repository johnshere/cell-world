---
name: lifecycle_118904
description: 生物118904完整生命周期能量追踪（100秒以上存活个体）
type: project
---

# 生物118904生命周期追踪

**追踪目标**: ID=118904, generation=0, parent_id=null
**初始采样时间**: world_time=待记录
**初始能量**: 108.62 J
**初始年龄**: 101.30秒
**出生时间**: 估算~world_time-101.30
**初始位置**: (-7.26, -1084.56)

## 基本信息
- generation: 0（初代个体）
- parent_id: null
- node_count: 35
- connection_count: 190

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

### T=101.30s (初始采样)
- energy: 108.62 J
- speed: 19.70
- position: (-7.26, -1084.56)
- alive: true
- follow_level: 0.521
- last_outputs: [0.716, 0.985, 0.536, 0.577, 0.967, 0, 0, 0]
- 嘴输出[2]=0.536（正，有进食欲望）
- light_intensity: 0.5

### T=145.30s (当前采样)
- energy: 41.76 J
- speed: 20.0（最大）
- position: (-17.23, -1106.20)
- alive: true
- follow_level: 0.523
- last_outputs: [0.937, 1.0, 0.798, 0.587, 0.971, 0, 0, 0]
- 嘴输出[2]=0.798（高进食欲望）
- light_intensity: 0.5

---

## 能量变化分析

| 时间点 | age(s) | energy | ΔE | ΔE/Δt | 状态 |
|--------|--------|--------|-----|-------|------|
| T=101 | 101.30 | 108.62 | - | - | alive |
| T=145 | 145.30 | 41.76 | -66.86 | -1.52 J/s | alive |

### 关键观察
- 44秒内消耗66.86J，消耗率约1.52 J/s
- 速度保持在19.7-20.0（高速移动）
- 嘴输出始终为正（0.536→0.798），有强烈进食欲望
- 位置变化约22px，高速移动

---

## 消耗分解（预估）

| 消耗类型 | 估算量(J) | 占比 | 说明 |
|---------|---------|------|------|
| 移动消耗 | ~52 | ~78% | 速度20.0，持续移动 |
| 基础代谢 | ~14 | ~21% | 随年龄增长 |
| 痕迹释放 | ~0.5 | ~1% | mouth输出正，有主动释放 |
| 体温逸散 | 0 | 0% | 已关闭 |
| **总计** | **~66.5** | **100%** | |

---

## 神经网络结构

- node_count: 35
- connection_count: 190
- 关键神经元: block(-1)=28, block(1)=29, block(0)=30, block(-2)=31
- maturation_time: 5000.0s

### 生理敏感度（默认值均为1.0）
- pleasure_energy: 1.0
- pleasure_group: 1.0
- pleasure_trail: 1.0

### 行为观察
- 嘴输出始终为正（0.536→0.798），说明该个体演化出了进食本能
- follow_level~0.52，有群体行为
- 位置接近(0, -1100)，附近可能有其他生物或粒子