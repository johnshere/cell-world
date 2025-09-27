import { viewport } from '../../graph';
import type { Ctor } from '../types';

import Cell, { type Position } from './cell';
import CellCarniv from './cell-carniv';
import CellPlant from './cell-plant';
import type { Ocean } from './ocean';

/** 植食细胞（以植物细胞为食） */
export default class CellHerbiv extends Cell {
  declare color: string;
  declare maxGeneration: number; // 最大分裂次数
  declare maxNearingCells: number; // 周围同类细胞数量超过此值时不分裂

  declare energy: number;
  declare energyToSplit: number; // 分裂所需的能量
  splitInterval!: number; // 分裂间隔时间（毫秒）
  splitTimer = 0;

  energyToMove!: number; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed!: number;
  moveTimer = 0;
  moveInterval = 0;
  preyClasses: Ctor<Cell>[] = [CellPlant];
  prey?: Cell; // 处于狩猎状态
  pioneers: Cell[] = []; // 先行细胞

  /** 饥饿状态变成肉食细胞的概率 */
  starvationToCarnivProb!: number;

  // 鸟群算法相关属性
  /** 感知范围(感知范围/鸟群算法) */
  senseRange!: number;
  /** 分离权重 */
  separationWeight!: number;
  /** 对齐权重 */
  alignmentWeight!: number;
  /** 聚集权重 */
  cohesionWeight!: number;
  /** 捕食向量权重 */
  huntingWeight!: number;
  mates: Cell[] = [];

  // 对象池复用初始化：重置字段，保持与构造器随机化一致
  override init() {
    super.init();
    this.name = '草食'; // 设置name属性
    this.color = 'sandybrown';
    this.maxGeneration = 2;
    this.maxNearingCells = 1;
    this.energy = 60;
    this.energyToSplit = 150;
    this.energyToMove = 4;
    this.energyToSpeed = 0.5;
    this.starvationToCarnivProb = 0.15;
    this.senseRange = 4;
    this.separationWeight = 0.3;
    this.alignmentWeight = 30;
    this.cohesionWeight = 9;
    this.huntingWeight = 150;
    // 分裂计时与间隔
    this.splitTimer = 0;
    this.splitInterval = 400;
    this.splitInterval = (1.5 - Math.random()) * this.splitInterval;
    // 移动节奏与计时
    this.moveTimer = 0;
    const moveMinInterval = 700;
    const moveMaxInterval = 900;
    this.moveInterval =
      Math.random() * (moveMaxInterval - moveMinInterval) + moveMinInterval;
    // 清理捕食与群体状态
    this.prey = undefined;
    this.velocity = { x: 0, y: 0 };
    this.mates = [];
  }
  override releaseToPool() {
    super.releaseToPool();
    this.prey = undefined;
    this.mates = [];
    this.moveTimer = 0;
    this.splitTimer = 0;
  }

  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      const nearSameCells = this.findSpecifyClass(
        this.constructor as Ctor<Cell>,
        2
      );
      if (nearSameCells.length > 0) {
        const nearPreyCells = [] as Cell[];
        for (const cls of this.preyClasses) {
          nearPreyCells.push(...this.findSpecifyClass(cls, 2));
        }
        if (
          nearPreyCells.length === 0 &&
          Math.random() < this.starvationToCarnivProb
        ) {
          // 转换为肉食细胞（对象池获取）
          const newSelf = this.ocean.acquire(CellCarniv);
          newSelf.setPosition(this.row, this.col);
          this.ocean.registerEntity(newSelf);
          this.die();
          return;
        }
      }
      this.die();
      return;
    }
    this.splitTimer += this.deltaTime;
    // 检查是否可以分裂
    if (
      this.energy >= this.energyToSplit &&
      this.splitTimer > this.splitInterval
    ) {
      this.splitTimer = 0;
      const child = this.split() as Cell;
      const energy = this.energy / 2;
      if (child) {
        child.energy = energy;
        this.energy = energy; // 分裂后重置能量
      }
    }
  }

  /** 对齐 - 与邻近同类细胞保持相同的移动方向 */
  align() {
    const steer = { x: 0, y: 0 };
    let count = 0;

    for (const mate of this.mates) {
      // 计算距离 (x对应col，y对应row)
      const dx = this.col - mate.col;
      const dy = this.row - mate.row;
      const distance = Math.sqrt(dx * dx + dy * dy);

      if (distance > 0 && distance < this.senseRange) {
        // 累加邻居的速度向量
        steer.x += mate.velocity.x;
        steer.y += mate.velocity.y;
        count++;
      }
    }

    // 只有当有邻居时才计算平均速度
    if (count > 0) {
      steer.x /= count;
      steer.y /= count;

      // 归一化并应用权重
      const magnitude = Math.sqrt(steer.x * steer.x + steer.y * steer.y);
      if (magnitude > 0) {
        steer.x = (steer.x / magnitude) * this.alignmentWeight;
        steer.y = (steer.y / magnitude) * this.alignmentWeight;
      }
    }

    return steer;
  }

  /** 分离 - 避免与邻近同类细胞过于拥挤 */
  separate() {
    const steer = { x: 0, y: 0 };
    let count = 0;

    for (const mate of this.mates) {
      // 计算距离 (x对应col，y对应row)
      const dx = this.col - mate.col;
      const dy = this.row - mate.row;
      const distance = Math.sqrt(dx * dx + dy * dy);

      if (distance > 0 && distance < this.senseRange) {
        // 计算分离向量（远离邻居）
        const separateX = dx / distance;
        const separateY = dy / distance;

        // 距离越近，分离力越强
        const force = 1 / distance;
        steer.x += separateX * force;
        steer.y += separateY * force;
        count++;
      }
    }

    // 只有当有邻居时才平均化分离向量
    if (count > 0) {
      steer.x /= count;
      steer.y /= count;

      // 归一化并应用权重
      const magnitude = Math.sqrt(steer.x * steer.x + steer.y * steer.y);
      if (magnitude > 0) {
        steer.x = (steer.x / magnitude) * this.separationWeight;
        steer.y = (steer.y / magnitude) * this.separationWeight;
      }
    }

    return steer;
  }

  /** 聚集 - 向邻近同类细胞的中心位置移动 */
  cohesion() {
    const center = { x: 0, y: 0 };
    let count = 0;

    for (const mate of this.mates) {
      // 计算距离 (x对应col，y对应row)
      const dx = this.col - mate.col;
      const dy = this.row - mate.row;
      const distance = Math.sqrt(dx * dx + dy * dy);

      if (distance > 0 && distance < this.senseRange) {
        // 累加邻居位置
        center.x += mate.col;
        center.y += mate.row;
        count++;
      }
    }

    const steer = { x: 0, y: 0 };

    // 只有当有邻居时才计算聚集行为
    if (count > 0) {
      // 计算邻居的中心位置
      center.x /= count;
      center.y /= count;

      // 计算向中心的方向向量
      const dx = center.x - this.col;
      const dy = center.y - this.row;

      // 归一化并应用权重
      const magnitude = Math.sqrt(dx * dx + dy * dy);
      if (magnitude > 0) {
        steer.x = (dx / magnitude) * this.cohesionWeight;
        steer.y = (dy / magnitude) * this.cohesionWeight;
      }
    }

    return steer;
  }

  /** 捕食向量 - 向捕食目标移动 */
  hunting() {
    const steer = { x: 0, y: 0 };

    if (this.prey && this.ocean.isExist(this.prey)) {
      // 计算向目标的方向向量
      const dx = this.prey.col - this.col;
      const dy = this.prey.row - this.row;

      // 归一化并应用权重
      const magnitude = Math.sqrt(dx * dx + dy * dy);
      const hunger = 1 - this.energy / this.energyToSplit;
      if (magnitude > 0) {
        steer.x = (dx / magnitude) * this.huntingWeight * hunger;
        steer.y = (dy / magnitude) * this.huntingWeight * hunger;
      }
    }

    return steer;
  }

  /** 群体移动 - 整合分离、对齐、聚集、捕食行为 */
  groupMove() {
    // 整合所有行为
    const acceleration = { x: 0, y: 0 };

    // 使用鸟群算法计算移动方向
    const separation = this.separate();
    acceleration.x += separation.x;
    acceleration.y += separation.y;

    const alignment = this.align();
    acceleration.x += alignment.x;
    acceleration.y += alignment.y;

    const cohesion = this.cohesion();
    acceleration.x += cohesion.x;
    acceleration.y += cohesion.y;

    const hunting = this.hunting();
    acceleration.x += hunting.x;
    acceleration.y += hunting.y;

    // 更新速度
    this.velocity.x += acceleration.x;
    this.velocity.y += acceleration.y;

    // 限制最大速度
    const sum =
      this.velocity.x * this.velocity.x + this.velocity.y * this.velocity.y;
    if (sum > 1) {
      const speed = Math.sqrt(sum);
      this.velocity.x = this.velocity.x / speed;
      this.velocity.y = this.velocity.y / speed;
    }

    let col = this.col;
    let row = this.row;
    if (Math.abs(this.velocity.x) > 0.5) {
      col += this.velocity.x > 0 ? 1 : -1;
    }
    if (Math.abs(this.velocity.y) > 0.5) {
      row += this.velocity.y > 0 ? 1 : -1;
    }
    if (this.col !== col || this.row !== row) {
      this.setPosition(row, col);
      return true;
    }
    return false;
  }
  /** 集群移动时，向前方细胞吸取能量 */
  absorb() {
    if (!this.pioneers.length || this.energy >= this.energyToSplit) return;

    // 当前能量比例控制
    const sourceRate = 1 - this.energy / this.energyToSplit;
    let energyToAbsorb = 0;

    // 从先代中吸取能量
    this.pioneers.forEach(pioneer => {
      if (pioneer.energy > pioneer.energyToSplit) {
        const energyOver = pioneer.energy - pioneer.energyToSplit;
        const rate = Math.min(energyOver / pioneer.energyToSplit, 1);
        const delta = energyOver * sourceRate * rate;

        energyToAbsorb += delta;
        pioneer.energy -= delta;
      }
    });

    this.energy += energyToAbsorb;
  }
  /** 移动到相邻位置 */
  move() {
    this.moveTimer += this.deltaTime;
    // 检查是否可以移动
    if (this.moveTimer < this.moveInterval - this.energy * this.energyToSpeed) {
      return;
    }
    this.moveTimer = 0;
    if (
      this.col < 0 ||
      this.row < 0 ||
      this.col >= viewport.cols ||
      this.row >= viewport.rows
    ) {
      this.energy -= this.energyToMove;
      return;
    }

    this.mates = this.findSpecifyClass(
      this.constructor as Ctor<Cell>,
      this.senseRange
    );
    const sum = this.senseRange * this.senseRange + 3;
    if (this.mates.length > 1) {
      this.groupMove();
      this.absorb();
      if (this.pioneers.length > 1) {
        this.energy -= this.energyToMove * (1 - this.pioneers.length / sum);
      } else {
        this.energy -= this.energyToMove * (1 - this.mates.length / sum);
      }
    } else {
      this.velocity = { x: 0, y: 0 };
      if (this.prey && this.ocean.isExist(this.prey)) {
        this.moveToward(this.prey);
      } else {
        // 没有集群移动，则随机移动
        this.directionSense();
        const next = this.getNextMovePosition();
        if (next) {
          this.setPosition(next.row, next.col);
        }
      }
      this.energy -= this.energyToMove * (1 - this.mates.length / sum);
    }

    // 移动后检查当前位置是否有植物细胞并吃掉它们
    this.eatPreyAtCurrentPosition();
  }

  /** 吃掉当前位置的植物细胞 */
  eatPreyAtCurrentPosition() {
    // 用索引快速取出当前格子的植物（避免数组分配，先收集后处理）
    const set = this.ocean.getEntitySet(this.row, this.col);
    if (!set) return;
    const preysAtCurrentPosition: Cell[] = [];
    for (const e of set) {
      if (e === this) continue;
      if (this.preyClasses.some(cls => e instanceof cls)) {
        preysAtCurrentPosition.push(e as Cell);
      }
    }

    // 吃掉所有找到的植物细胞
    preysAtCurrentPosition.forEach(prey => {
      // 从海洋中移除植物
      prey.die();

      // 增加能量
      this.energy += prey.energy;

      // 如果吃掉的是当前目标，清除目标
      if (prey === this.prey) {
        this.prey = undefined;
      }
    });

    if (preysAtCurrentPosition.length) {
      this.breath();
    }
  }

  /** 感知 - 在前方一定范围内寻找植物细胞作为目标 */
  sense() {
    const prey = this.prey;
    const range = this.senseRange;
    const isExist = prey && this.ocean.isExist(prey);
    if (
      isExist &&
      Math.abs(prey.row - this.row) <= range &&
      Math.abs(prey.col - this.col) <= range
    ) {
      return;
    }
    this.prey = undefined;
    this.pioneers = [];
    // 速度方向上，距离是senseRange+1的位置
    const velocityMagnitude = Math.sqrt(
      this.velocity.x * this.velocity.x + this.velocity.y * this.velocity.y
    );
    const normalizedVelocity = {
      x: this.velocity.x / velocityMagnitude,
      y: this.velocity.y / velocityMagnitude,
    };

    const center = {
      row: this.row + Math.round(normalizedVelocity.y * (this.senseRange + 1)),
      col: this.col + Math.round(normalizedVelocity.x * (this.senseRange + 1)),
    };
    // 先用索引收集候选植物集合
    const preys = [] as Cell[];
    const pioneers = [] as Cell[];
    this.scanNearPositions(range, center, posis => {
      for (const pos of posis) {
        const set = this.ocean.getEntitySet(pos.row, pos.col);
        if (!set) continue;
        for (const e of set) {
          if (e instanceof this.constructor) {
            pioneers.push(e as Cell);
          }
          if (this.preyClasses.some(cls => e instanceof cls)) {
            preys.push(e as Cell);
          }
        }
      }
      return !!pioneers.length || !!preys.length;
    });

    if (pioneers.length) {
      this.pioneers = pioneers;
    }
    if (preys.length) {
      // 随机选择一个候选作为目标
      this.prey = preys[Math.floor(Math.random() * preys.length)];
    }
  }
}
