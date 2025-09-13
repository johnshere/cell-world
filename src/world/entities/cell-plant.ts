import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  color = 'green';
  /** 每秒光合获取的能量 */
  energyToGrow = 0.1;
  /** 分裂所需的能量 */
  energyToSplit = 30;
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
    super.split();
  }
}
