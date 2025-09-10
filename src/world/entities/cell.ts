import { GraphConfig } from '../../const/graph-config';
import { viewport } from '../../graph';
import { CellConfig } from '../../const/config';

import Entity from './entity';

export default class Cell extends Entity {
  generation = 0;
  moveDirections = [0, 1]; // 0-上 1-右 2-下 3-左
  constructor() {
    super();

    // 取当前视窗范围，随机生成逻辑位置
    this.col = Math.floor(Math.random() * viewport.cols) + viewport.col;
    this.row = Math.floor(Math.random() * viewport.rows) + viewport.row;

    const dir = Math.floor(Math.random() * 4);
    const dir2 = (dir + 1) % 4;
    this.moveDirections = [dir, dir2];

    this.color = 'pink';
  }
  update(deltaTime: number) {
    super.update(deltaTime);
    this.separate();
    this.align();
    this.cohesion();
    this.hunt();
    this.move();
    this.grow();
  }
  /** 分离 */
  separate() {}
  /** 对齐 */
  align() {}
  /** 聚集 */
  cohesion() {}
  /** 觅食 */
  hunt() {}
  /** 移动 */
  move() {}
  /** 生长 */
  grow() {}
  /** 呼吸 */
  breath() {
    let isBreathing = false;
    const color = this.color;
    this.breath = function () {
      if (isBreathing) {
        return;
      }
      isBreathing = true;
      const startTime = Date.now();

      const flash = () => {
        if (this.color === color) {
          this.color = CellConfig.breathColor;
        } else {
          this.color = color;
        }
        if (Date.now() - startTime >= CellConfig.breathDuration) {
          this.color = color;
          isBreathing = false;
          return;
        }
        setTimeout(flash, CellConfig.breathInterval);
      };
      flash();
    };
  }
  /** 死亡 */
  die() {
    this.ocean.removeEntity(this);
  }
  getAdjacentPositions() {
    // 获取相邻位置（周围8个方向）
    return [
      { row: this.row - 1, col: this.col - 1 }, // 左上
      { row: this.row - 1, col: this.col }, // 上
      { row: this.row - 1, col: this.col + 1 }, // 右上
      { row: this.row, col: this.col + 1 }, // 右
      { row: this.row + 1, col: this.col + 1 }, // 右下
      { row: this.row + 1, col: this.col }, // 下
      { row: this.row + 1, col: this.col - 1 }, // 左下
      { row: this.row, col: this.col - 1 }, // 左
    ];
  }
  findFreePosition(adjacentPositions?: { row: number; col: number }[]) {
    if (!adjacentPositions) {
      adjacentPositions = this.getAdjacentPositions();
    }
    return adjacentPositions.filter(pos => {
      return !this.ocean.hasEntityAt(pos.row, pos.col);
    });
  }
  getNextMovePosition(): { row: number; col: number } | void {
    let count = 0;
    const getDirection = () => {
      if (count > 10) {
        return 0;
      }
      count++;
      const direction = Math.floor(Math.random() * 4);
      if (!this.moveDirections.includes(direction)) return getDirection();
      return direction;
    };
    const direction = getDirection();
    let adjacentPositions = this.getAdjacentPositions();
    if (direction === 0) {
      adjacentPositions = adjacentPositions.splice(0, 3);
    } else if (direction === 1) {
      adjacentPositions = adjacentPositions.splice(2, 3);
    } else if (direction === 2) {
      adjacentPositions = adjacentPositions.splice(4, 3);
    } else if (direction === 3) {
      adjacentPositions = adjacentPositions
        .splice(6, 2)
        .concat(...adjacentPositions.splice(0, 1));
    }
    // 过滤出空闲位置
    const freePositions = this.findFreePosition(adjacentPositions);

    if (freePositions.length === 0) {
      return;
    } else if (freePositions.length > 0) {
      // 随机移动到一个空闲位置
      const randomPos =
        freePositions[Math.floor(Math.random() * freePositions.length)];
      return randomPos;
    } else {
      return this.getNextMovePosition();
    }
  }
  /** 分裂 */
  split() {
    if (!this.ocean) return;

    // 获取相邻位置（周围8个方向）
    const adjacentPositions = this.getAdjacentPositions();

    // 过滤出空闲位置（没有其他细胞占据的位置）
    const freePositions = this.findFreePosition(adjacentPositions);

    const nearingCellsCount = adjacentPositions.length - freePositions.length;
    // 如果周围细胞超过配置的最大值，不进行分裂
    if (nearingCellsCount > CellConfig.maxNearingCells) {
      return;
    }

    // 如果有空闲位置，随机选择一个进行分裂
    if (freePositions.length > 0) {
      const randomPos =
        freePositions[Math.floor(Math.random() * freePositions.length)];

      // 创建与当前细胞相同类型的新细胞
      const NewCellClass = this.constructor as new () => Cell;
      const child = new NewCellClass();
      child.row = randomPos.row;
      child.col = randomPos.col;
      child.ocean = this.ocean;

      // 添加到海洋中（使用索引）
      this.ocean.registerEntity(child);

      this.generation++;
      return child;
    }
  }
}
