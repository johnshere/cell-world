import { PlantCellConfig } from '../../const/config';

import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  energy = PlantCellConfig.basedEnergy;
  constructor() {
    super();
    this.color = 'green';
    this.energy = PlantCellConfig.basedEnergy * (1 + Math.random() * 4);
  }

  grow() {
    this.energy += this.deltaTime / 1000;
    // 检查是否可以分裂
    if (this.energy < PlantCellConfig.energyToSplit) {
      return;
    }

    if (this.generation >= PlantCellConfig.maxGeneration) {
      this.die();
    }

    // 调用父类的分裂方法
    const child = super.split() as CellPlant;
    if (!child) return;
    // 重置分裂计数
    child.generation = this.generation - 1;
  }
}
