import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  color = 'green';
  constructor() {
    super();
    this.energy = this.energy + Math.floor(Math.random() * this.energyToSplit);
  }

  grow() {
    this.energy += this.deltaTime / 1000;
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
    if (!child) return;
    // 重置分裂计数
    child.generation = this.generation - 1;
  }
}
