import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  color = 'green';
  energy = 40;
  /** 每秒光合获取的能量 */
  energyToGrow = 3;
  /** 分裂所需的能量 */
  energyToSplit = 110;
  maxGeneration = 200;
  maxNearingCells = 2; // 周围同类细胞数量超过此值时不分裂
  splitInterval = 1000; // 分裂间隔时间（毫秒）
  splitTimer = 0;

  constructor() {
    super();
    this.energy = (1.5 - Math.random()) * this.energy;
    this.energyToSplit = (1.5 - Math.random()) * this.energyToSplit;
    this.splitInterval = (1.5 - Math.random()) * this.splitInterval;
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
