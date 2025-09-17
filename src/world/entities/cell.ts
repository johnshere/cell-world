import { viewport } from '../../graph';

import Entity from './entity';

export type Direction = -1 | 0 | 1;
export type Position = { row: number; col: number };
export type Positions = Position[];

export default class Cell extends Entity {
  /** 能量 */
  energy = 1;
  energyToSplit = 20; // 分裂所需的能量

  generation = 0;
  maxGeneration = 3; // 最大分裂次数
  maxNearingCells = 2; // 周围同类细胞数量超过此值时不分裂
  breathInterval = 500; // 呼吸间隔时间（毫秒）
  breathDuration = 3000; // 呼吸颜色持续时间（毫秒）
  breathColor = 'white'; // 呼吸颜色

  // 新增：记录前一次位置
  footprint?: Position;
  direction: { x: Direction; y: Direction } = { x: 0, y: 0 };

  // 新增：方向改变概率相关参数
  directionChangeChance = 0.05; // 基础改变方向概率 5%
  directionChangeIncrement = 0.05; // 每次移动增加的概率 5%
  constructor() {
    super();

    // 取当前视窗范围，随机生成逻辑位置
    this.col = Math.floor(Math.random() * viewport.cols) + viewport.col;
    this.row = Math.floor(Math.random() * viewport.rows) + viewport.row;

    this.color = 'pink';
    this.directionSense(true);
  }
  directionSense(force = false) {
    if (force || Math.random() < this.directionChangeChance) {
      this.directionChangeChance = this.directionChangeIncrement;
      this.direction = {
        x: Math.random() < 0.5 ? -1 : 1,
        y: Math.random() < 0.5 ? -1 : 1,
      };
    } else {
      this.directionChangeChance += this.directionChangeIncrement;
    }
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
        if (this.color === this.breathColor) {
          this.color = color;
        } else {
          this.color = this.breathColor;
        }
        if (Date.now() - startTime >= this.breathDuration) {
          this.color = color;
          isBreathing = false;
          return;
        }
        setTimeout(flash, this.breathInterval);
      };
      flash();
    };
  }
  /** 死亡 */
  die() {
    this.ocean.removeEntity(this);
  }
  /** 获取相邻位置 */
  scanNearPositions(range = 1, scan?: (poss: Positions) => boolean | void) {
    const positions: Position[] = [];

    // 按顺时针方向获取指定范围内的位置
    for (let r = 1; r <= range; r++) {
      // 上边（从左到右）
      for (let col = this.col - r; col <= this.col + r; col++) {
        positions.push({ row: this.row - r, col });
      }

      // 右边（从上到下，排除右上角）
      for (let row = this.row - r + 1; row <= this.row + r; row++) {
        positions.push({ row, col: this.col + r });
      }

      // 下边（从右到左，排除右下角）
      for (let col = this.col + r - 1; col >= this.col - r; col--) {
        positions.push({ row: this.row + r, col });
      }

      // 左边（从下到上，排除左下角和左上角）
      for (let row = this.row + r - 1; row > this.row - r; row--) {
        positions.push({ row, col: this.col - r });
      }
      if (scan?.(positions)) {
        return positions;
      }
    }

    return positions;
  }
  /** 查找同类位置 */
  findSpecifyClassPositions(Ctor: new () => Cell, range = 1) {
    const adjacentPositions = this.scanNearPositions(range);
    return adjacentPositions.filter(pos => {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) return false;
      for (const e of set) {
        if (e instanceof Ctor) return true;
      }
      return false;
    });
  }
  findNotSameFreePosition(adjacentPositions?: Positions) {
    if (!adjacentPositions) {
      adjacentPositions = this.scanNearPositions();
    }
    // 空闲位置定义：该格子中不存在与当前细胞同类的细胞（允许异类共址）
    const SelfCtor = this.constructor as new () => Cell;
    return adjacentPositions.filter(pos => {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) return true;
      for (const e of set) {
        if (e instanceof SelfCtor) return false;
      }
      return true;
    });
  }
  setPosition(row: number, col: number): void {
    if (this.row === row && this.col === col) return;
    this.footprint = { row: this.row, col: this.col };
    // 更新当前方向为实际移动的方向
    this.direction.y = (row - this.row) as Direction;
    this.direction.x = (col - this.col) as Direction;

    super.setPosition(row, col);
  }
  getNextMovePosition(): Position | void {
    // 根据当前方向获取目标位置
    const targetRow = this.row + this.direction.y;
    const targetCol = this.col + this.direction.x;

    const targetPosition = { row: targetRow, col: targetCol };

    // 检查目标位置是否空闲
    const freePositions = this.findNotSameFreePosition([targetPosition]);

    if (freePositions.length > 0) {
      return targetPosition;
    } else {
      // 目标位置被占用，尝试寻找附近的空闲位置
      const nearPositions = this.scanNearPositions();
      const allFreePositions = this.findNotSameFreePosition(nearPositions);

      if (allFreePositions.length > 0) {
        // 随机选择一个空闲位置
        const randomPos =
          allFreePositions[Math.floor(Math.random() * allFreePositions.length)];

        return randomPos;
      } else {
        // 没有空闲位置，无法移动
        return;
      }
    }
  }
  /** 向目标移动 */
  moveTowardsTarget(target: Cell) {
    // 计算向目标移动的方向
    const deltaRow = target.row - this.row;
    const deltaCol = target.col - this.col;

    // 检查是否已经到达目标位置
    if (deltaRow === 0 && deltaCol === 0) {
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

    // 移动到新位置
    this.setPosition(nextRow, nextCol);
  }
  /** 分裂 */
  split() {
    // 获取相邻位置（周围8个方向）
    const adjacentPositions = this.scanNearPositions();

    // 过滤出空闲位置（没有其他细胞占据的位置）
    const freePositions = this.findNotSameFreePosition(adjacentPositions);

    // 仅统计周围相邻格子中“同类”细胞的数量（每格按是否存在同类计数一次）
    const SelfCtor = this.constructor as new () => Cell;
    const sameTypeNeighbors = adjacentPositions.filter(pos => {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) return false;
      for (const e of set) {
        if (e instanceof SelfCtor) return true;
      }
      return false;
    }).length;
    // 如果周围同类细胞数量超过配置的最大值，不进行分裂
    if (sameTypeNeighbors > this.maxNearingCells) {
      return;
    }

    // 如果有空闲位置，随机选择一个进行分裂
    if (freePositions.length > 0) {
      const randomPos =
        freePositions[Math.floor(Math.random() * freePositions.length)];

      // 创建与当前细胞相同类型的新细胞
      const NewCellClass = this.constructor as new () => Cell;
      const child = new NewCellClass();
      child.ocean = this.ocean;
      child.setPosition(randomPos.row, randomPos.col);

      // 添加到海洋中（使用索引）
      this.ocean.registerEntity(child);

      this.generation++;
      return child;
    }
  }
}
