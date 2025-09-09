import { OceanConfig } from '../../const/config';
import { viewport } from '../../graph';

import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import Entity from './entity';

const ocean = {
  entities: [] as Entity[],
  deltaTime: 0,
  lastSpawnTime: 0,
  storm() {
    this.lastSpawnTime += this.deltaTime;

    // 根据当前细胞数量动态计算生成间隔
    // 细胞越多，生成间隔越长（生成越慢）
    const ratio = this.entities.length / OceanConfig.maxEntities;
    const currentSpawnInterval =
      OceanConfig.baseSpawnInterval +
      (OceanConfig.maxSpawnInterval - OceanConfig.baseSpawnInterval) * ratio;

    if (
      this.lastSpawnTime >= currentSpawnInterval &&
      this.entities.length < OceanConfig.maxEntities
    ) {
      // 随机生成三种细胞类型之一
      const cellTypes = [
        CellPlant,
        CellPlant,
        CellPlant,
        CellHerbiv,
        CellCarniv,
      ];
      const randomType =
        cellTypes[Math.floor(Math.random() * cellTypes.length)];
      const newOne = new randomType();
      newOne.ocean = this;
      this.entities.push(newOne);
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

export type Ocean = typeof ocean;
export default ocean;
