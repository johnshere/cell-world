import Cell from './cell';

/** 植物细胞（自动生长） */
export default class CellPlant extends Cell {
  declare color: string;
  declare energy: number;
  energyToGrow!: number; // 生长所需的能量

  declare energyToSplit: number;
  declare maxGeneration: number;
  declare maxNearingCells: number; // 周围同类细胞数量超过此值时不分裂

  splitInterval!: number; // 分裂间隔时间（毫秒）
  splitTimer = 0;

  // 对象池复用初始化：重置字段，保持与构造器随机化一致
  override init() {
    super.init();
    this.color = 'green';
    this.energy = 40;
    this.energyToGrow = 3;
    this.energyToSplit = 110;
    this.maxGeneration = 200;
    this.maxNearingCells = 2;
    this.splitInterval = 1000;
    // 随机化能量、分裂能量和分裂间隔
    this.energy = (1.5 - Math.random()) * this.energy;
    this.energyToSplit = (1.5 - Math.random()) * this.energyToSplit;
    this.splitInterval = (1.5 - Math.random()) * this.splitInterval;
    this.splitTimer = 0;
  }

  override releaseToPool() {
    super.releaseToPool();
    // 清理计时器，避免下次复用残留
    this.splitTimer = 0;
  }

  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy < 0) {
      this.die();
      return;
    }
    this.energy += (this.energyToGrow * this.deltaTime) / 1000;
    this.splitTimer += this.deltaTime;

    // 检查是否可以分裂
    // 检查是否到了分裂时间
    if (
      this.energy > this.energyToSplit &&
      this.splitTimer > this.splitInterval
    ) {
      this.splitTimer = 0;
      // 调用父类的分裂方法
      const child = super.split() as CellPlant;
      if (child) {
        const halfEnergy = this.energy / 2;
        child.energy = halfEnergy;
        this.energy = halfEnergy;
        child.generation = this.generation;
        this.generation += 1;
      }
    }
  }
}
