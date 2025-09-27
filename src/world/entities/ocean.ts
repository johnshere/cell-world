import { OceanConfig } from '../../const/config';
import { viewport } from '../../graph';
import { drawRectsBatch } from '../../graph';
import type { Ctor } from '../types';

import type { Positions } from './cell';
import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellOmniv from './cell-omniv';
import CellPlant from './cell-plant';
import Entity from './entity';

const ocean = {
  deltaTime: 0,
  // 以行->列->实体集合 的索引结构，便于按网格快速查询
  grid: new Map<number, Map<number, Set<Entity>>>(),
  // 高性能实体存在性查找的Set索引，O(1)时间复杂度
  entities: new Set<Entity>(),

  // 对象池：按构造函数分类复用实例
  pools: new Map<Function, Entity[]>(),

  // 增量统计缓存，避免每次遍历所有实体
  // 使用动态Map结构，支持任意类型的实体统计
  entityStats: new Map<string, number>(),

  // 获取单元格的实体集合
  getEntitySet(
    row: number,
    col: number,
    newWhenNone = false
  ): Set<Entity> | undefined {
    let rowMap = this.grid.get(row);
    if (!rowMap) {
      if (!newWhenNone) return undefined;
      rowMap = new Map<number, Set<Entity>>();
      this.grid.set(row, rowMap);
    }
    let set = rowMap.get(col);
    if (!set && newWhenNone) {
      set = new Set<Entity>();
      rowMap.set(col, set);
    }
    return set;
  },
  // 注册实体（加入数组与索引）
  registerEntity(entity: Entity) {
    if (!this.isExist(entity)) {
      this.entities.add(entity);
      // 增量更新统计
      this.updateEntityStats(entity, 1);
    }
    const set = this.getEntitySet(entity.row, entity.col, true)!;
    set.add(entity);
  },
  // 从世界中移除实体（数组与索引）
  removeEntity(entity: Entity) {
    // 先从索引移除
    const set = this.getEntitySet(entity.row, entity.col);
    if (set) {
      set.delete(entity);
      // 清理空集合
      if (set.size === 0) {
        const rowMap = this.grid.get(entity.row);
        rowMap?.delete(entity.col);
        if (rowMap && rowMap.size === 0) this.grid.delete(entity.row);
      }
    }
    // 再从实体数组和Set移除
    if (this.isExist(entity)) {
      this.entities.delete(entity);
      // 增量更新统计
      this.updateEntityStats(entity, -1);
    }
    // 回收至对象池
    this.release(entity);
  },

  // 增量更新实体统计
  updateEntityStats(entity: Entity, delta: number) {
    const type = entity.name;
    const current = this.entityStats.get(type) || 0;
    this.entityStats.set(type, current + delta);
  },

  updateEntityPosition(
    entity: Entity,
    oldRow: number,
    oldCol: number,
    newRow: number,
    newCol: number
  ) {
    if (oldRow === newRow && oldCol === newCol) return;
    // 从旧位置移除
    const oldSet = this.getEntitySet(oldRow, oldCol);
    if (oldSet) {
      oldSet.delete(entity);
      if (oldSet.size === 0) {
        const rowMap = this.grid.get(oldRow);
        rowMap?.delete(oldCol);
        if (rowMap && rowMap.size === 0) this.grid.delete(oldRow);
      }
    }
    // 加入新位置
    const newSet = this.getEntitySet(newRow, newCol, true)!;
    newSet.add(entity);
  },
  // 高性能判断实体是否还在ocean中，O(1)时间复杂度
  isExist(entity?: Entity): boolean {
    if (!entity) return false;
    return this.entities.has(entity);
  },
  rebuildGrid() {
    this.grid.clear();
    this.entities.clear();
  },
  // 基于配置的初始生成权重（替代硬编码模板数组）
  pickCellType(): typeof Entity {
    const weights = OceanConfig.SpawnWeights;
    const items: Array<{ ctor: typeof Entity; w: number }> = [
      { ctor: CellPlant, w: weights.plant ?? 0 },
      { ctor: CellHerbiv, w: weights.herbiv ?? 0 },
      { ctor: CellOmniv, w: weights.omniv ?? 0 },
      { ctor: CellCarniv, w: weights.carniv ?? 0 },
    ];
    const total = items.reduce((sum, it) => sum + Math.max(0, it.w), 0);
    if (total <= 0) {
      // 回退：若权重全为0，则默认使用植物
      return CellPlant;
    }
    let r = Math.random() * total;
    for (const it of items) {
      const w = Math.max(0, it.w);
      if (r < w) return it.ctor;
      r -= w;
    }
    return items[0].ctor; // 理论上不会走到这里
  },

  // 从对象池/构造函数获取实例
  acquire<T extends Entity>(Ctor: Ctor<T>): T {
    const stack = this.pools.get(Ctor) ?? [];
    const inst = stack.pop();
    if (inst) {
      // 复用实例：做必要的初始化
      inst.init();
      return inst as T;
    }
    // 新建实例（构造器内有初始随机化）
    const fresh = new Ctor();
    // 这里不调用 initFromPool，避免重复随机化，保持与构造器逻辑一致
    fresh.init();
    return fresh;
  },
  // 释放实例到对象池
  release(entity: Entity) {
    entity.releaseToPool();
    const ctor = entity.constructor as Function;
    let stack = this.pools.get(ctor);
    if (!stack) {
      stack = [];
      this.pools.set(ctor, stack);
    }
    stack.push(entity);
  },

  creator(cellTypes?: (typeof Entity)[]) {
    // 随机生成细胞类型之一
    let randomType: typeof Entity;
    if (cellTypes && cellTypes.length > 0) {
      randomType = cellTypes[Math.floor(Math.random() * cellTypes.length)];
    } else {
      randomType = this.pickCellType();
    }
    const newOne = this.acquire(randomType);
    // 使用索引注册
    this.registerEntity(newOne);
    return newOne;
  },
  storm() {
    const count = (viewport.cols * viewport.rows) / OceanConfig.initEntityRatio;
    while (this.entities.size < count) {
      const newOne = this.creator();
      if (newOne instanceof CellPlant) {
        newOne.energy = OceanConfig.initPlantEnergy;
      }
    }
    let time = 0;
    let unit = (viewport.cols * viewport.rows) / 100000;
    unit *= OceanConfig.SpawnNaturalPlantRate;
    const bornInterval = Math.ceil(1000 / unit);
    this.storm = function () {
      time += this.deltaTime;
      if (time > bornInterval) {
        time = 0;
        this.creator([CellPlant]);
      }
    };
  },
  update(deltaTime: number) {
    this.deltaTime = deltaTime;
    this.storm();
    // 更新所有实体
    this.entities.forEach(entity => entity.update(deltaTime));

    console.log(
      Array.from(this.entities).filter(e => e instanceof CellOmniv).length
    );
  },
  render() {
    // 仅渲染视窗范围内的实体
    const startRow = viewport.row;
    const endRow = viewport.row + viewport.rows;
    const startCol = viewport.col;
    const endCol = viewport.col + viewport.cols;

    // 按颜色分组实体，实现批量渲染
    const colorGroups = new Map<string, Positions>();

    for (let r = startRow; r <= endRow; r++) {
      const rowMap = this.grid.get(r);
      if (!rowMap) continue;
      for (let c = startCol; c <= endCol; c++) {
        const set = rowMap.get(c);
        if (!set) continue;
        for (const entity of set) {
          const color = entity.color || 'white';
          if (!colorGroups.has(color)) {
            colorGroups.set(color, []);
          }
          colorGroups.get(color)!.push({ row: entity.row, col: entity.col });
        }
      }
    }

    // 批量渲染相同颜色的实体
    for (const [color, positions] of colorGroups) {
      if (positions.length > 0) {
        this.batchRenderRects(positions, color);
      }
    }
  },

  // 批量渲染相同颜色的矩形
  batchRenderRects(positions: Positions, color: string) {
    // 转换为drawRectsBatch所需的格式
    const batchPositions = positions.map(pos => ({
      col: pos.col,
      row: pos.row,
    }));
    drawRectsBatch(batchPositions, color);
  },
};

// 共享空数组实例，避免重复分配
const EMPTY: readonly Entity[] = Object.freeze([]);

export type Ocean = typeof ocean;
export default ocean;
