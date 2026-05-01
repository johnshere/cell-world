---
name: lifecycle_72881
description: 生物72881完整生命周期能量追踪
type: project
---

# 生物72881生命周期追踪

**追踪目标**: ID=72881, generation=0, parent=null
**初始能量**: 140.44
**出生时间**: ~11224s (world time)
**初始位置**: (-1879.14, 689.82)
**地形高度**: 42

## 配置参数(用于消耗计算)

| 参数 | 值 | 说明 |
|------|-----|------|
| base_metabolism | 0.025 | 基础代谢 |
| age_metabolism_factor | 0.012 | 年龄代谢系数 |
| metabolism_exponent | 2.0 | 代谢指数 |
| move_cost | 0.1 | 移动消耗系数 |
| vision_range | 100.0 | 感知范围 |
| heat_dissipation_coefficient | 0.0 | 散热系数(已关闭) |
| feed_efficiency | 0.85 | 喂食效率 |
| initial_energy | 160.0 | 初始能量 |

## 消耗估算公式

### 1. 基础代谢
```
base_metabolism_rate = base_metabolism × (1 + age × age_metabolism_factor)^metabolism_exponent
```
- dt = 1/30 s per update

### 2. 移动消耗
```
radius = sqrt(energy / 1.28)
base_rate = move_cost × 0.00001 × radius³
actual_cost = base_rate × distance × speed × terrain_factor × follow_discount
```

### 3. 体温逸散
```
nearby_energy = vision_range内所有粒子+生物能量
heat_factor = heat_floor + (1-heat_floor) × exp(-nearby_energy / energy_denominator)
heat_cost = heat_dissipation_coefficient × perimeter × heat_factor × dt
```

---

## 采样数据

### T=0 (age=0.1s)
- energy: 140.44
- speed: 2.23
- position: (-1879.14, 689.82)
- alive: true
- nearby_creatures: 0
- nearby_energy_particles: 0
- estimated_metabolism_cost: ?

### T=30s (age=~0.1+30=30.1s)
- energy: 待采样
- speed: 待采样
- alive: 待确认