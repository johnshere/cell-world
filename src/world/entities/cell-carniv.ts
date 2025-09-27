import Cell from './cell';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import type { Ocean } from './ocean';

/** 食肉细胞（以植食细胞为食） */
export default class CellCarniv extends Cell {
  declare color: string;
  private moveTimer = 0;
  private moveInterval = 0;
  private prey?: CellHerbiv; // 处于狩猎状态

  declare maxGeneration: number; // 最大分裂次数
  declare maxNearingCells: number; // 周围同类细胞数量超过此值时不分裂
  moveMinInterval!: number; // 移动间隔时间（毫秒）
  moveMaxInterval!: number; // 移动间隔时间（毫秒）

  /** 感知范围 */
  senseRange!: number;
  /** 低能量感知范围 */
  senseRangeLowEnergy!: number;
  /** 捕猎失败被反杀的概率（比目标能量低时） */
  huntFailedRatio!: number;

  declare energy: number;
  declare energyToSplit: number; // 分裂所需的能量
  energyToMove!: number; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed!: number;
  /** 低能量阈值（低于等于该值时进入待机：不移动不消耗能量） */
  lowEnergyThreshold!: number;
  /** 低能量状态下的能量消耗 */
  lowEnergyConsumption!: number;

  // 对象池复用初始化：重置字段，保持与构造器随机化一致
  override init() {
    super.init();
    this.name = '肉食'; // 设置name属性
    this.color = 'DeepPink';
    this.maxGeneration = 3;
    this.maxNearingCells = 1;
    this.moveMinInterval = 50;
    this.moveMaxInterval = 800;
    this.senseRange = 7;
    this.senseRangeLowEnergy = 3;
    this.huntFailedRatio = 0.5;
    this.energyToSplit = 1000;
    this.energyToMove = 40;
    this.energyToSpeed = 1;
    this.lowEnergyConsumption = 1;
    // 能量与阈值随机化
    this.energy = 100;
    this.energy = this.energy * (Math.random() + 1);
    this.lowEnergyThreshold = 0.1;
    this.lowEnergyThreshold =
      (1.5 - Math.random()) * this.lowEnergyThreshold * this.energyToSplit;
    // 移动节奏与计时
    this.moveTimer = 0;
    this.moveInterval =
      Math.random() * (this.moveMaxInterval - this.moveMinInterval) +
      this.moveMinInterval;
    // 清理捕食目标
    this.prey = undefined;
  }
  override releaseToPool() {
    super.releaseToPool();
    this.prey = undefined;
    this.moveTimer = 0;
  }
  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      if (this.findSpecifyClass(CellPlant).length > 5) {
        if (this.findSpecifyClass(CellCarniv, 2).length === 0) {
          const newSelf = this.ocean.acquire(CellHerbiv);
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
      }
    }
    // 移动后检查当前位置是否有植食细胞并吃掉它们
    this.attackAndEat();

    this.energy -= this.energyToMove;
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

      // 增加能量（根据猎物能量值）
      this.energy += prey.energy;
      this.breath();
    }
  }

  /** 感知 - 在前方一定范围内寻找植食细胞作为目标 */
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

    // 速度方向上，距离是senseRange+1的位置
    const dir = this.direction;
    const center = {
      row: this.row + dir.row * (this.senseRange + 1),
      col: this.col + dir.col * (this.senseRange + 1),
    };

    // 先用索引收集候选植食集合
    const preys = [] as CellHerbiv[];
    this.scanNearPositions(this.senseRange, center, posis => {
      for (const pos of posis) {
        const set = this.ocean.getEntitySet(pos.row, pos.col);
        if (!set) continue;
        for (const e of set) {
          if (e instanceof CellHerbiv) {
            preys.push(e);
          }
        }
      }
      return preys.length > 0;
    });

    if (preys.length === 0) return;

    // 随机选择一个候选作为目标
    this.prey = preys[Math.floor(Math.random() * preys.length)];
  }
}
