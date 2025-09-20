import { viewport } from '../../graph';

import Entity from './entity';
import type { Ocean } from './ocean';

export type Position = { row: number; col: number };
export type Positions = Position[];
export type Near = -1 | 0 | 1;
export type Direction = { col: Near; row: Near };
// 定义所有八个可能的方向
export const AllDirections: Direction[] = [
  { col: -1, row: -1 }, // 左上
  { col: 0, row: -1 }, // 上
  { col: 1, row: -1 }, // 右上
  { col: -1, row: 0 }, // 左
  { col: 1, row: 0 }, // 右
  { col: -1, row: 1 }, // 左下
  { col: 0, row: 1 }, // 下
  { col: 1, row: 1 }, // 右下
];

type Scan = (poss: Positions) => boolean | void;

export default class Cell extends Entity {
  /** 能量 */
  energy!: number;
  energyToSplit!: number; // 分裂所需的能量

  /** 先代 */
  ancestor?: Cell;

  generation!: number;
  maxGeneration!: number; // 最大分裂次数
  maxNearingCells!: number; // 周围同类细胞数量超过此值时不分裂
  breathInterval!: number; // 呼吸间隔时间（毫秒）
  breathDuration!: number; // 呼吸颜色持续时间（毫秒）
  breathColor!: string; // 呼吸颜色

  direction!: Direction;

  // 新增：方向改变概率相关参数
  directionChangeChance!: number; // 基础改变方向概率 5%
  directionChangeIncrement!: number; // 每次移动增加的概率 5%

  // 对象池复用初始化：重置公共Cell状态
  override init() {
    super.init();
    // 恢复颜色（子类会在各自init中覆盖）
    this.color = 'black';
    /** 能量 */
    this.energy = 1;
    this.energyToSplit = 20; // 分裂所需的能量

    /** 先代 */
    this.ancestor = undefined;

    this.generation = 0;
    this.maxGeneration = 3; // 最大分裂次数
    this.maxNearingCells = 2; // 周围同类细胞数量超过此值时不分裂
    this.breathInterval = 500; // 呼吸间隔时间（毫秒）
    this.breathDuration = 3000; // 呼吸颜色持续时间（毫秒）
    this.breathColor = 'white'; // 呼吸颜色

    this.direction = { col: 0, row: 0 };

    // 新增：方向改变概率相关参数
    this.directionChangeChance = 0.05; // 基础改变方向概率 5%
    this.directionChangeIncrement = 0.02; // 每次移动增加的概率 5%
    // 重新随机初始位置与初始方向
    const col = Math.floor(Math.random() * viewport.cols) + viewport.col;
    const row = Math.floor(Math.random() * viewport.rows) + viewport.row;
    this.setPosition(row, col);
    this.directionSense(true);
  }
  override releaseToPool() {
    super.releaseToPool();
    this.ancestor = undefined;
  }

  directionNormalize(dir: number): Near {
    if (dir > 0) return 1;
    if (dir < 0) return -1;
    return 0;
  }
  getOppositeDirection(dir: Direction): Direction[] {
    const adjacent: Direction[] = [];
    for (const d of AllDirections) {
      // 计算两个方向之间的距离（切比雪夫距离）
      const distance = Math.max(
        Math.abs(d.col - dir.col),
        Math.abs(d.row - dir.row)
      );
      if (distance === 1) {
        adjacent.push(d);
      }
    }
    return adjacent;
  }
  directionSense(force = false) {
    if (force || Math.random() < this.directionChangeChance) {
      const col = (Math.random() < 0.5 ? -1 : 1) as Near;
      const row = (Math.random() < 0.5 ? -1 : 1) as Near;
      const isOpposite =
        col === -this.direction.col && row === -this.direction.row;
      if (isOpposite) {
        this.directionSense(true);
        return;
      }
      this.direction = { col, row };
      this.directionChangeChance = this.directionChangeIncrement;
    } else {
      this.directionChangeChance += this.directionChangeIncrement;
    }
  }
  update(deltaTime: number) {
    super.update(deltaTime);
    this.sense();
    this.move();
    this.grow();
  }
  /** 感知 */
  sense() {}
  /** 移动 */
  move() {}
  /** 生长 */
  grow() {}
  /** 呼吸 */
  breath() {
    let isBreathing = false;
    const color = this.color;
    this.breath = () => {
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
  /**
   * 获取相邻位置
   * @param range 范围
   * @param center 中心位置
   * @param scan 扫描函数 返回true时停止扫描
   * @returns 相邻位置
   */
  scanNearPositions(range = 1, center?: Position | Scan, scan?: Scan) {
    const positions: Position[] = [];

    if (typeof center === 'function') {
      scan = center;
      center = { row: this.row, col: this.col };
    } else if (!center) {
      center = { row: this.row, col: this.col };
    }

    // 按顺时针方向获取指定范围内的位置
    for (let r = 1; r <= range; r++) {
      const layerPositions: Position[] = [];

      // 上边（从左到右）
      for (let col = center.col - r; col <= center.col + r; col++) {
        layerPositions.push({ row: center.row - r, col });
      }

      // 右边（从上到下，排除右上角）
      for (let row = center.row - r + 1; row <= center.row + r; row++) {
        layerPositions.push({ row, col: center.col + r });
      }

      // 下边（从右到左，排除右下角）
      for (let col = center.col + r - 1; col >= center.col - r; col--) {
        layerPositions.push({ row: center.row + r, col });
      }

      // 左边（从下到上，排除左下角和左上角）
      for (let row = center.row + r - 1; row > center.row - r; row--) {
        layerPositions.push({ row, col: center.col - r });
      }

      // 随机打乱当前层的位置顺序，消除方向偏向
      for (let i = layerPositions.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [layerPositions[i], layerPositions[j]] = [
          layerPositions[j],
          layerPositions[i],
        ];
      }

      positions.push(...layerPositions);

      if (scan?.(positions)) {
        return positions;
      }
    }

    return positions;
  }
  findSpecifyClass<T extends Cell>(Ctor: new () => T, range = 1) {
    const positions = this.scanNearPositions(range);
    const result: T[] = [];
    positions.forEach(pos => {
      const set = this.ocean.getEntitySet(pos.row, pos.col);
      if (!set) return;
      for (const e of set) {
        if (e instanceof Ctor) {
          result.push(e);
        }
      }
    });
    return result;
  }
  findNotSameFreePosition(adjacentPositions?: Positions) {
    if (!adjacentPositions) {
      adjacentPositions = this.scanNearPositions();
    }
    // 空闲位置定义：该格子中不存在与当前细胞同类的细胞（允许异类共址）
    const SelfCtor = this.constructor as new () => Cell;
    return adjacentPositions.filter(pos => {
      const set = this.ocean.getEntitySet(pos.row, pos.col);
      if (!set) return true;
      for (const e of set) {
        if (e instanceof SelfCtor) return false;
      }
      return true;
    });
  }
  setPosition(row: number, col: number): void {
    if (this.row === row && this.col === col) return;
    // 更新当前方向为实际移动的方向
    let r = row - this.row;
    r = r === 0 ? 0 : r > 0 ? 1 : -1;
    this.direction.row = r as Near;
    let c = col - this.col;
    c = c === 0 ? 0 : c > 0 ? 1 : -1;
    this.direction.col = c as Near;

    super.setPosition(row, col);
  }
  getNextMovePosition(): Position | void {
    // 根据当前方向获取目标位置
    const targetRow = this.row + this.direction.row;
    const targetCol = this.col + this.direction.col;

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
  moveToward(target: Position | Cell) {
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
    // 判断是否目标位置有同类
    const targetCell = this.ocean.getEntitySet(target.row, target.col);
    if (targetCell) {
      for (const e of targetCell) {
        if (e instanceof this.constructor) {
          const next = this.getNextMovePosition();
          if (next) {
            nextRow = next.row;
            nextCol = next.col;
          }
        }
      }
    }
    // 移动到新位置
    this.setPosition(nextRow, nextCol);
  }
  /** 分裂 */
  split() {
    // 获取相邻位置（周围8个方向）
    const nearPositions = this.scanNearPositions();

    // 过滤出空闲位置（没有其他细胞占据的位置）
    const freePositions = this.findNotSameFreePosition(nearPositions);

    // 仅统计周围相邻格子中“同类”细胞的数量（每格按是否存在同类计数一次）
    const SelfCtor = this.constructor as new () => Cell;
    const sameTypeNeighbors = nearPositions.filter(pos => {
      const set = this.ocean.getEntitySet(pos.row, pos.col);
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

      // 创建与当前细胞相同类型的新细胞（对象池）
      const NewCellClass = this.constructor as new () => Cell;
      const child = this.ocean.acquire(NewCellClass) as Cell;
      child.ancestor = this;
      child.setPosition(randomPos.row, randomPos.col);

      // 添加到海洋中（使用索引）
      this.ocean.registerEntity(child);

      this.generation++;
      return child;
    }
  }
}
