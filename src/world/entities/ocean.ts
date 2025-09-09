import { OceanConfig } from '../../const/config';
import { viewport } from '../../graph';

import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import Entity from './entity';

const ocean = {
  entities: [] as Entity[],
  deltaTime: 0,
  creator() {
    // 随机生成三种细胞类型之一
    const cellTypes = [CellPlant, CellHerbiv];
    const randomType = cellTypes[Math.floor(Math.random() * cellTypes.length)];
    const newOne = new randomType();
    newOne.ocean = this;
    this.entities.push(newOne);
  },
  storm() {
    while (this.entities.length < OceanConfig.maxEntities) {
      this.creator();
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
