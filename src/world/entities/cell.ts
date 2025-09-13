import { viewport } from '../../graph';

import Entity from './entity';

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
  getNearPositions(range = 1) {
    const positions: { row: number; col: number }[] = [];

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
    }

    return positions;
  }
  /** 查找同类位置 */
  findSpecifyClassPositions(Ctor: new () => Cell, range = 1) {
    const adjacentPositions = this.getNearPositions(range);
    return adjacentPositions.filter(pos => {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) return false;
      for (const e of set) {
        if (e instanceof Ctor) return true;
      }
      return false;
    });
  }
  findNotSameFreePosition(adjacentPositions?: { row: number; col: number }[]) {
    if (!adjacentPositions) {
      adjacentPositions = this.getNearPositions();
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
    let adjacentPositions = this.getNearPositions();
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
    const freePositions = this.findNotSameFreePosition(adjacentPositions);

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
    // 获取相邻位置（周围8个方向）
    const adjacentPositions = this.getNearPositions();

    // 过滤出空闲位置（没有其他细胞占据的位置）
    const freePositions = this.findNotSameFreePosition(adjacentPositions);

    // 仅统计周围相邻格子中“同类”细胞的数量（每格按是否存在同类计数一次）
    const SelfCtor2 = this.constructor as new () => Cell;
    const sameTypeNeighbors = adjacentPositions.filter(pos => {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) return false;
      for (const e of set) {
        if (e instanceof SelfCtor2) return true;
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
