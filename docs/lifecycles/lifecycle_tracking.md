---
name: lifecycle_tracking
description: 生物生命周期追踪系统
type: project
---

# 生命周期追踪日志

## 当前目标: 142521 (切换自70170)

## 采样筛选规则（2026-05-01 更新）

新增追踪目标必须**同时**满足：

1. **generation ≥ 10**：跳过初代（gen=0）和早期世代，演化效果尚未稳定的样本无分析价值
2. **age ≥ 50s**：原阈值 100s 改为 50s，扩大可观察样本量
3. 已存在的 lifecycle\_\*.md 文件不受新规则影响（历史归档）

筛选 MCP 调用示例：

```
get_creatures(min_generation=10, min_age=50, alive_only=true, sort_by=age, sort_desc=true)
```

## 散热核对（2026-05-01 修复）

历史 lifecycle 文档中 `heat_dissipation_coefficient = 0.0` 是 MCP bug 导致的旧值（MCP 用启动时克隆的 Config，运行时改值不生效）。已修复为共享 Arc<RwLock<Config>>。新采样必须以 `get_config` 实时返回值为准，不要照抄旧文档的 0.0。

## 任务目标

追踪15个生命的完整生命周期，每个生命从出生到死亡全程采样记录。

## 追踪规则

- 目标生物年龄 > 50s（确保进入成熟期），直到死亡结束这个目标
- 死亡后切换到新生生物继续追踪
- 累计完成15个完整生命周期后汇总分析

## 数据格式

| 时间戳 | world_time | age | energy | speed | x | y | follow_level | mouth_output | 备注 |

## 当前追踪状态

- 完成数: 0 / 15
- 当前目标: 待定

## 完成记录
