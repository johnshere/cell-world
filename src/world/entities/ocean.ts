import { OceanConfig } from '../../const/config';
import { viewport } from '../../graph';

import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import Entity from './entity';

const ocean = {
  entities: [] as Entity[],
  deltaTime: 0,
  // 以行->列->实体集合 的索引结构，便于按网格快速查询
  grid: new Map<number, Map<number, Set<Entity>>>(),

  // 获取单元格的实体集合
  getCellSet(
    row: number,
    col: number,
    create = false
  ): Set<Entity> | undefined {
    let rowMap = this.grid.get(row);
    if (!rowMap) {
      if (!create) return undefined;
      rowMap = new Map<number, Set<Entity>>();
      this.grid.set(row, rowMap);
    }
    let set = rowMap.get(col);
    if (!set && create) {
      set = new Set<Entity>();
      rowMap.set(col, set);
    }
    return set;
  },
  // 注册实体（加入数组与索引）
  registerEntity(entity: Entity) {
    if (!this.entities.includes(entity)) {
      this.entities.push(entity);
    }
    const set = this.getCellSet(entity.row, entity.col, true)!;
    set.add(entity);
  },
  // 从世界中移除实体（数组与索引）
  removeEntity(entity: Entity) {
    // 先从索引移除
    const set = this.getCellSet(entity.row, entity.col);
    if (set) {
      set.delete(entity);
      // 清理空集合
      if (set.size === 0) {
        const rowMap = this.grid.get(entity.row);
        rowMap?.delete(entity.col);
        if (rowMap && rowMap.size === 0) this.grid.delete(entity.row);
      }
    }
    // 再从实体数组移除
    const idx = this.entities.indexOf(entity);
    if (idx !== -1) this.entities.splice(idx, 1);
  },
  // 当实体位置发生变化时更新索引
  updateEntityPosition(
    entity: Entity,
    oldRow: number,
    oldCol: number,
    newRow: number,
    newCol: number
  ) {
    if (oldRow === newRow && oldCol === newCol) return;
    // 从旧位置移除
    const oldSet = this.getCellSet(oldRow, oldCol);
    if (oldSet) {
      oldSet.delete(entity);
      if (oldSet.size === 0) {
        const rowMap = this.grid.get(oldRow);
        rowMap?.delete(oldCol);
        if (rowMap && rowMap.size === 0) this.grid.delete(oldRow);
      }
    }
    // 加入新位置
    const newSet = this.getCellSet(newRow, newCol, true)!;
    newSet.add(entity);
  },
  hasEntityAt(row: number, col: number): boolean {
    const set = this.getCellSet(row, col);
    return !!set && set.size > 0;
  },
  getCellEntities(row: number, col: number): readonly Entity[] {
    const set = this.getCellSet(row, col);
    return set ? Array.from(set) : [];
  },
  rebuildGrid() {
    this.grid.clear();
    for (const e of this.entities) {
      this.registerEntity(e);
    }
  },
  // 基于配置的初始生成权重（替代硬编码模板数组）
  pickCellType(): typeof Entity {
    const weights = OceanConfig.SpawnWeights;
    const items: Array<{ ctor: typeof Entity; w: number }> = [
      { ctor: CellPlant, w: weights.plant ?? 0 },
      { ctor: CellHerbiv, w: weights.herbiv ?? 0 },
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
  creator(cellTypes?: (typeof Entity)[]) {
    // 随机生成三种细胞类型之一
    let randomType: typeof Entity;
    if (cellTypes && cellTypes.length > 0) {
      randomType = cellTypes[Math.floor(Math.random() * cellTypes.length)];
    } else {
      randomType = this.pickCellType();
    }
    const newOne = new randomType();
    newOne.ocean = this;
    // 使用索引注册
    this.registerEntity(newOne);
  },
  storm() {
    while (this.entities.length < OceanConfig.initEntities) {
      this.creator();
    }
    let time = 0;
    this.storm = function () {
      time += this.deltaTime;
      if (time > OceanConfig.SpawnNaturalPlantInterval) {
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
  },
  render() {
    // 仅渲染视窗范围内的实体
    const startRow = viewport.row;
    const endRow = viewport.row + viewport.rows;
    const startCol = viewport.col;
    const endCol = viewport.col + viewport.cols;

    for (let r = startRow; r <= endRow; r++) {
      const rowMap = this.grid.get(r);
      if (!rowMap) continue;
      for (let c = startCol; c <= endCol; c++) {
        const set = rowMap.get(c);
        if (!set) continue;
        for (const entity of set) {
          entity.render();
        }
      }
    }
  },
};

export type Ocean = typeof ocean;
export default ocean;
