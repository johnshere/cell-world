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

    // 检查目标位置是否被非植物细胞占用
    const blockingEntity = this.ocean.entities.find(
      entity =>
        entity !== this &&
        entity.row === nextRow &&
        entity.col === nextCol &&
        !(entity instanceof CellPlant)
    );

    if (!blockingEntity) {
      // 移动到新位置
      this.row = nextRow;
      this.col = nextCol;
    }
    // 如果目标位置被非植物细胞占用，保持当前位置，下次再尝试
  }

  /** 吃掉当前位置的植物细胞 */
  private eatPlantsAtCurrentPosition() {
    if (!this.ocean) return;

    // 找到当前位置的所有植物细胞
    const plantsAtCurrentPosition = this.ocean.entities.filter(
      entity =>
        entity instanceof CellPlant &&
        entity.row === this.row &&
        entity.col === this.col
    ) as CellPlant[];

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

    // 检测搜索区域内是否有植物细胞
    const plantCell = this.ocean.entities.find(entity => {
      // 检查实体是否是植物细胞（通过类型判断）
      if (!(entity instanceof CellPlant)) return false;
      // 检查实体是否在搜索区域内
      if (
        entity.row >= startRow &&
        entity.row <= endRow &&
        entity.col >= startCol &&
        entity.col <= endCol
      ) {
        return true;
      }
      return false;
    });
    if (plantCell && !this.target) {
      // 发现植物且当前不在狩猎状态，转换到狩猎状态
      this.target = plantCell as CellPlant;
    }
  }
}
