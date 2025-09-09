import { colord, Colord } from 'colord';

import { drawRect } from '../../graph';

export default class Entity {
  row: number;
  col: number;
  color: Colord;
  constructor() {
    this.col = 0;
    this.row = 0;
    this.color = colord('black');
  }

  update(deltaTime: number) {}
  render() {
    // 传递逻辑坐标，让drawRect内部处理真实坐标转换
    drawRect(this.col, this.row, this.color.toHex());
  }
}
