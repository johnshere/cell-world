import Cell from './cell';
import CellCarniv from './cell-carniv';
import CellPlant from './cell-plant';

/** 植食细胞 */
export default class CellHerbiv extends Cell {
  maxGeneration = 2; // 最大分裂次数
  maxNearingCells = 1; // 周围同类细胞数量超过此值时不分裂
  moveMinInterval = 800; // 移动间隔时间（毫秒）
  moveMaxInterval = 1400; // 移动间隔时间（毫秒）

  energy = 90;
  energyToSplit = 100; // 分裂所需的能量
  energyToMove = -1; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed = 3;
  private lastMoveTime = 0;
  private moveInterval = 0;
  private target?: CellPlant; // 处于狩猎状态

  /** 饥饿状态变成肉食细胞的概率 */
  private starvationToCarnivProb = 0.3;

  /** 感知范围(觅食范围) */
  private senseRange = 4;

  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() * (this.moveMaxInterval - this.moveMinInterval) +
      this.moveMinInterval;

    this.color = 'sandybrown';
  }

  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      const nearSameCells = this.findSpecifyClassPositions(CellHerbiv, 2);
      if (nearSameCells.length > 0) {
        const nearPlantCells = this.findSpecifyClassPositions(CellPlant, 2);
        if (
          nearPlantCells.length === 0 &&
          Math.random() < this.starvationToCarnivProb
        ) {
          // 转换为肉食细胞
          const newSelf = new CellCarniv();
          newSelf.ocean = this.ocean;
          newSelf.setPosition(this.row, this.col);
          this.ocean.registerEntity(newSelf);
          this.die();
          return;
        }
      }
      this.die();
      return;
    }
    // 检查是否可以分裂
    if (this.energy >= this.energyToSplit) {
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
    this.lastMoveTime += this.deltaTime;
    // 检查是否可以移动
    if (
      this.lastMoveTime <
      this.moveInterval - this.energy * this.energyToSpeed
    ) {
      return;
    }
    this.lastMoveTime = 0;

    if (this.target && this.ocean.isExist(this.target)) {
      this.moveTowardsTarget(this.target);
    } else {
      this.directionSense();
      const next = this.getNextMovePosition();
      if (next) {
        this.setPosition(next.row, next.col);
      } else {
        return;
      }
    }

    // 移动后检查当前位置是否有植物细胞并吃掉它们
    this.eatPlantsAtCurrentPosition();

    this.energy += this.energyToMove;
  }

  /** 吃掉当前位置的植物细胞 */
  private eatPlantsAtCurrentPosition() {
    // 用索引快速取出当前格子的植物（避免数组分配，先收集后处理）
    const set = this.ocean.getCellSet(this.row, this.col);
    if (!set) return;
    const plantsAtCurrentPosition: CellPlant[] = [];
    for (const e of set) {
      if (e instanceof CellPlant) plantsAtCurrentPosition.push(e);
    }

    // 吃掉所有找到的植物细胞
    plantsAtCurrentPosition.forEach(plant => {
      // 从海洋中移除植物
      plant.die();

      // 增加能量
      this.energy += plant.energy;

      // 如果吃掉的是当前目标，清除目标
      if (plant === this.target) {
        this.target = undefined;
      }
    });

    if (plantsAtCurrentPosition.length) {
      this.breath();
    }
  }

  /** 觅食 - 在一定范围内寻找植物细胞作为目标 */
  hunt() {
    const tar = this.target;
    const range = this.senseRange;
    const isExist = tar && this.ocean.isExist(tar);
    if (isExist && tar.row - this.row <= range && tar.col - this.col <= range) {
      return;
    }

    // 先用索引收集候选植物集合
    const targets = [] as CellPlant[];
    this.scanNearPositions(range, posis => {
      posis.forEach(pos => {
        const set = this.ocean.getCellSet(pos.row, pos.col);
        if (!set) return;
        for (const e of set) {
          if (e instanceof CellPlant) {
            targets.push(e);
          }
        }
      });
      return !!targets.length;
    });

    if (targets.length === 0) return;

    // 随机选择一个候选作为目标
    this.target = targets[Math.floor(Math.random() * targets.length)];
  }
}
