import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  color = 'green';
  energy = 10;
  /** 每秒光合获取的能量 */
  energyToGrow = 0.3;
  /** 分裂所需的能量 */
  energyToSplit = 22;
  constructor() {
    super();
    this.energy = Math.random() * this.energyToSplit;
  }

  grow() {
    this.energy += (this.energyToGrow * this.deltaTime) / 1000;
    // 检查是否可以分裂
    if (this.energy < this.energyToSplit) {
      return;
    }

    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }

    // 调用父类的分裂方法
    const child = super.split() as CellPlant;
    if (child) {
      const halfEnergy = this.energy / 2;
      child.energy = halfEnergy;
      this.energy = halfEnergy;
    }
  }
}
