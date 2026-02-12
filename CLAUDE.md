# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Cell-World 是一个基于 Vite + TypeScript 的生态演化模拟器，使用 Canvas 实时渲染细胞生物的自主行为与种群演化。

## 常用命令

```bash
pnpm dev          # 开发模式：类型检查 + Vite 热重载
pnpm build        # 生产构建：TS 编译 + Vite 打包
pnpm lint         # ESLint 自动修复
pnpm lint:check   # ESLint 仅检查
pnpm format       # Prettier 格式化
pnpm type-check   # TypeScript 类型检查
```

## 架构概览

### 核心模块

- **游戏循环** (`src/world/index.ts`): 帧率控制（默认 10FPS）、自动加速/减速机制、性能监控
- **生物系统** (`src/world/entities/`):
  - `Ocean`: 网格索引管理，O(1) 空间查询
  - `Cell` 基类 + 三类子类：`CellPlant`（植物）、`CellHerbiv`（草食）、`CellCarniv`（肉食）
- **渲染系统** (`src/graph/index.ts`): Canvas 2D 视口系统、拖拽缩放、网格刻度尺
- **面板系统** (`src/panel/index.ts`): 实时 FPS、世界信息、种群统计（500ms 节流更新）

### 配置系统

- **世界配置**: `src/const/config.ts` - 帧率、加速参数、生物权重
- **渲染配置**: `src/const/graph-config.ts` - 网格大小、缩放范围
- **运行时调整**: 通过 `window.CellWorldConfig` 全局对象

### 关键参数

```typescript
// WorldConfig
FrameRate: 10              // 目标帧率
AccelerateMax: 30          // 最大加速倍数
SpawnWeights: { plant: 4, herbiv: 1, carniv: 0 }  // 生物生成权重

// GraphConfig
gridSize: 6                // 网格像素大小
scaleRange: [0.5, 3]       // 缩放范围
```

## 代码规范

- **格式**: Prettier（LF 换行、2 空格缩进、单引号）
- **检查**: ESLint + TypeScript strict 模式
- **自动化**: VSCode 保存时自动修复和格式化

## 开发注意

1. 性能关键点：Ocean 的网格索引、Entity 批量更新
2. Canvas 渲染需处理 viewport 坐标变换
3. 面板数据通过专用 update 函数推送，避免每帧同步
