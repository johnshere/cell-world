import { HerbivCellConfig } from '../../const/config';

import Cell from './cell';
import CellPlant from './cell-plant';

/** 植食细胞 */
export default class CellHerbiv extends Cell {
  private lastMoveTime = 0;
  private moveInterval = 0;
  private target: CellPlant | null = null; // 处于狩猎状态
  energy = HerbivCellConfig.basedEnergy;
  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() *
        (HerbivCellConfig.moveMaxInterval - HerbivCellConfig.moveMinInterval) +
      HerbivCellConfig.moveMinInterval;

    this.color = 'brown';
  }

  grow() {
    if (this.generation >= HerbivCellConfig.maxGeneration || this.energy <= 0) {
      this.die();
    }
    // 检查是否可以分裂
    if (this.energy >= HerbivCellConfig.energyToSplit) {
      const child = this.split() as CellHerbiv;
      const energy = this.energy / 2;
      if (child) {
        child.energy = energy;
        this.energy = energy; // 分裂后重置能量
      }
    }
  }

  /** 移动到相邻位置 */
  move() {
    if (!this.ocean) return;
    this.lastMoveTime += this.deltaTime;
    // 检查是否可以移动
    if (this.lastMoveTime < this.moveInterval - this.energy * 40) {
      return;
    }
    this.lastMoveTime = 0;

    if (this.target) {
      // 处于狩猎状态则向目标移动
      this.moveTowardsTarget();
    } else {
      // 不处于狩猎状态，随即移动
      const nextPos = this.getNextMovePosition();
      if (nextPos) {
        this.row = nextPos.row;
        this.col = nextPos.col;
      }
    }
    // 移动后检查当前位置是否有植物细胞并吃掉它们
    this.eatPlantsAtCurrentPosition();

    this.energy += HerbivCellConfig.energyToMove;
  }

  /** 向目标移动并尝试进食 */
  private moveTowardsTarget() {
    if (!this.target || !this.ocean) {
      this.target = null;
      return;
    }

    // 检查目标是否还存在于海洋中
    if (!this.ocean.entities.includes(this.target)) {
      this.target = null;
      return;
    }

    // 计算向目标移动的方向
    const deltaRow = this.target.row - this.row;
    const deltaCol = this.target.col - this.col;

    // 检查是否已经到达目标位置
    if (deltaRow === 0 && deltaCol === 0) {
      // 已经在目标位置，清除目标
      this.target = null;
      return;
    }

    // 确定下一步移动位置
    let nextRow = this.row;
    let nextCol = this.col;

    if (Math.abs(deltaRow) > Math.abs(deltaCol)) {
      // 优先在行方向移动
      nextRow = this.row + (deltaRow > 0 ? 1 : -1);
    } else {
      // 优先在列方向移动
      nextCol = this.col + (deltaCol > 0 ? 1 : -1);
    }

    // 检查目标位置是否被非植物细胞占用（用索引加速）
    const blocking = this.ocean
      .getCellEntities(nextRow, nextCol)
      .some(entity => entity !== this && !(entity instanceof CellPlant));

    if (!blocking) {
      // 移动到新位置
      this.row = nextRow;
      this.col = nextCol;
    }
    // 如果目标位置被非植物细胞占用，保持当前位置，下次再尝试
  }

  /** 吃掉当前位置的植物细胞 */
  private eatPlantsAtCurrentPosition() {
    if (!this.ocean) return;

    // 用索引快速取出当前格子的植物
    const plantsAtCurrentPosition = this.ocean
      .getCellEntities(this.row, this.col)
      .filter((e): e is CellPlant => e instanceof CellPlant);

    // 吃掉所有找到的植物细胞
    plantsAtCurrentPosition.forEach(plant => {
      // 从海洋中移除植物
      plant.die();

      // 增加能量
      this.energy += plant.generation;

      // 如果吃掉的是当前目标，清除目标
      if (plant === this.target) {
        this.target = null;
      }
    });

    if (plantsAtCurrentPosition.length) {
      this.breath();
    }
  }

  /** 觅食 - 检测指定方向范围内的植物细胞 */
  hunt() {
    if (!this.ocean || this.target) return;

    const range = HerbivCellConfig.huntRange;

    // 根据方向确定搜索的正方形区域
    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    // 先用索引收集候选植物集合
    const candidates = new Set<CellPlant>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const ents = this.ocean.getCellEntities(r, c);
        for (const e of ents) {
          if (e instanceof CellPlant) {
            candidates.add(e);
          }
        }
      }
    }

    if (candidates.size === 0) return;

    // 按原 entities 顺序选择首个候选，尽量不改变既有逻辑
    for (const e of this.ocean.entities) {
      if (candidates.has(e as CellPlant)) {
        this.target = e as CellPlant;
        break;
      }
    }
  }
}
