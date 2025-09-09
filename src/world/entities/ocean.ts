import { GridSize } from '../../const/config';
import { viewport } from '../../graph';

import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import Entity from './entity';

export default {
  entities: [] as Entity[],
  deltaTime: 0,
  lastSpawnTime: 0,
  spawnInterval: 1000, // 1秒 = 1000毫秒
  storm() {
    this.lastSpawnTime += this.deltaTime;

    if (this.lastSpawnTime >= this.spawnInterval) {
      // 随机生成三种细胞类型之一
      const cellTypes = [CellPlant, CellHerbiv, CellCarniv];
      const randomType =
        cellTypes[Math.floor(Math.random() * cellTypes.length)];
      this.entities.push(new randomType());
      this.lastSpawnTime = 0;
    }
  },
  update(deltaTime: number) {
    this.deltaTime = deltaTime;
    this.storm();

    // 更新所有实体
    this.entities.forEach(entity => entity.update(deltaTime));
  },
  render() {
    // 渲染所有实体，超过视窗范围不渲染
    this.entities.forEach(entity => {
      if (
        entity.col < viewport.col ||
        entity.col > viewport.col + viewport.cols ||
        entity.row < viewport.row ||
        entity.row > viewport.row + viewport.rows
      ) {
        return;
      }
      entity.render();
    });
  },
};
