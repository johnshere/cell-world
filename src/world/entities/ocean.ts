import { OceanConfig } from '../../const/config';
import { viewport } from '../../graph';

import CellCarniv from './cell-carniv';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';
import Entity from './entity';

const ocean = {
  entities: [] as Entity[],
  deltaTime: 0,
  creator(cellTypes?: (typeof Entity)[]) {
    // 随机生成三种细胞类型之一
    if (!cellTypes) {
      cellTypes = [CellPlant, CellHerbiv];
    }
    const randomType = cellTypes[Math.floor(Math.random() * cellTypes.length)];
    const newOne = new randomType();
    newOne.ocean = this;
    this.entities.push(newOne);
  },
  storm() {
    while (this.entities.length < OceanConfig.initEntities) {
      this.creator();
    }
    let time = 0;
    this.storm = function () {
      time += this.deltaTime;
      if (time > 1000) {
        time = 0;
        this.creator([CellPlant]);
      }
      if (this.entities.length > OceanConfig.maxEntities) {
        this.entities = this.entities.filter(entity => {
          if (entity instanceof CellPlant) {
            return Math.random() > 0.3;
          }
          return true;
        });
      }
    };
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
