import { PlantCellConfig } from '../../const/config';

import Cell from './cell';

/** 植物细胞 */
export default class CellPlant extends Cell {
  private lastSplitTime = 0;
  private splitInterval = 0;
  constructor() {
    super();
    this.color = 'green';
    this.splitInterval =
      PlantCellConfig.splitMinInterval +
      Math.random() *
        (PlantCellConfig.splitMaxInterval - PlantCellConfig.splitMinInterval);
  }

  grow() {
    this.lastSplitTime += this.deltaTime;

    // 检查是否可以分裂
    if (this.lastSplitTime < this.splitInterval) {
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
    child.lastSplitTime = 0;
    this.lastSplitTime = 0;
  }
}
