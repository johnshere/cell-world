import Cell from './cell';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';

/** 食肉细胞（以植食细胞为食） */
export default class CellCarniv extends Cell {
  color = 'DeepPink';
  private moveTimer = 0;
  private moveInterval = 0;
  private prey?: CellHerbiv; // 处于狩猎状态

  maxGeneration = 3; // 最大分裂次数
  maxNearingCells = 1; // 周围同类细胞数量超过此值时不分裂
  moveMinInterval = 50; // 移动间隔时间（毫秒）
  moveMaxInterval = 800; // 移动间隔时间（毫秒）

  /** 感知范围 */
  senseRange = 7;
  /** 低能量感知范围 */
  senseRangeLowEnergy = 3;
  /** 捕猎失败被反杀的概率（比目标能量低时） */
  huntFailedRatio = 0.5;

  energy = 30;
  energyToSplit = 10000; // 分裂所需的能量
  energyToMove = -12; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed = 1;
  /** 低能量阈值（低于等于该值时进入待机：不移动不消耗能量） */
  lowEnergyThreshold = 0.1;
  /** 低能量状态下的能量消耗 */
  lowEnergyConsumption = 1;
  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() * (this.moveMaxInterval - this.moveMinInterval) +
      this.moveMinInterval;
    this.energy = this.energy * (Math.random() + 1);
    this.lowEnergyThreshold =
      (1.5 - Math.random()) * this.lowEnergyThreshold * this.energyToSplit;
  }
  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      if (this.findSpecifyClassPositions(CellPlant).length > 6) {
        if (this.findSpecifyClassPositions(CellCarniv, 2).length === 0) {
          const newSelf = new CellHerbiv();
          newSelf.ocean = this.ocean;
          newSelf.setPosition(this.row, this.col);
          this.ocean.registerEntity(newSelf);
        }
      }
      this.die();
      return;
    }
    // 检查是否可以分裂
    if (this.energy >= this.energyToSplit) {
      const child = this.split() as CellCarniv;
      const energy = this.energy / 2;
      if (child) {
        child.energy = energy;
        this.energy = energy; // 分裂后重置能量
      }
    }
  }

  /** 移动到相邻位置 */
  move() {
    // 所有状态下都先累积移动计时，便于低能量追击同样遵循节奏
    this.moveTimer += this.deltaTime;

    if (this.moveTimer < this.moveInterval - this.energy * this.energyToSpeed) {
      return;
    }
    this.moveTimer = 0;

    if (this.prey && this.ocean.isExist(this.prey)) {
      // 处于狩猎状态则向目标移动
      this.moveToward(this.prey);
    } else {
      // 不处于狩猎状态，随机移动
      this.directionSense();
      const next = this.getNextMovePosition();
      if (next) {
        this.setPosition(next.row, next.col);
      } else {
        return;
      }
    }
    // 移动后检查当前位置是否有植食细胞并吃掉它们
    this.attackAndEat();

    this.energy += this.energyToMove;
  }
  /** 吃掉当前位置的植食细胞 */
  private attackAndEat() {
    // 用索引快速取出当前格子的植食细胞
    const herbivSet = this.ocean.getEntitySet(this.row, this.col);
    if (!herbivSet) return;
    const herbivsAtCurrentPosition: CellHerbiv[] = [];
    for (const e of herbivSet) {
      if (e instanceof CellHerbiv) herbivsAtCurrentPosition.push(e);
    }

    // 吃掉所有找到的植食细胞
    for (const prey of herbivsAtCurrentPosition) {
      if (this.energy < prey.energy) {
        if (Math.random() < this.huntFailedRatio) {
          this.die();
          return;
        }
      }
      // 从海洋中移除猎物
      prey.die();

      // 增加能量：用被吃猎物的代际数作为增量
      this.energy += prey.energy;

      // 如果吃掉的是当前目标，清除目标
      if (prey === this.prey) {
        this.prey = undefined;
      }

      this.breath();
    }
  }

  /** 感知 - 在一定范围内寻找植食细胞作为目标（正常状态下使用全范围） */
  sense() {
    const prey = this.prey;
    const isExist = prey && this.ocean.isExist(prey);
    let range = this.senseRange;
    if (isExist) {
      if (this.energy <= this.lowEnergyThreshold) {
        range = this.senseRangeLowEnergy;
        // 检查现有目标是否仍在半径内
        const withinHalf =
          Math.abs(prey.row - this.row) <= range &&
          Math.abs(prey.col - this.col) <= range;
        if (withinHalf) return;
      } else {
        if (
          Math.abs(prey.row - this.row) <= range &&
          Math.abs(prey.col - this.col) <= range
        ) {
          return;
        }
      }
    }
    this.prey = undefined;

    // 用索引收集候选猎物集合
    const preys: CellHerbiv[] = [];
    this.scanNearPositions(range, posis => {
      posis.forEach(pos => {
        const set = this.ocean.getEntitySet(pos.row, pos.col);
        if (!set) return;
        for (const e of set) {
          if (e instanceof CellHerbiv) {
            preys.push(e);
          }
        }
      });
      return !!preys.length;
    });

    if (preys.length === 0) return;
    this.prey = preys[Math.floor(Math.random() * preys.length)];
  }
}
