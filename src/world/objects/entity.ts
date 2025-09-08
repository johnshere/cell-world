import type { Colord } from 'colord';

import { GridSize } from '../../const/config';
import { drawRect, viewport } from '../../graph';
import { randomColor } from '../../utils';

export default class Entity {
  row: number;
  col: number;
  color: Colord;
  constructor() {
    // 取当前视窗范围，随机生成逻辑位置
    const x = (Math.random() * viewport.width + viewport.x) / GridSize;
    this.col = Math.floor(x);
    const y = (Math.random() * viewport.height + viewport.y) / GridSize;
    this.row = Math.floor(y);

    this.color = randomColor();
  }
  update(deltaTime: number) {}
  render(deltaTime: number) {
    // 传递逻辑坐标，让drawRect内部处理真实坐标转换
    drawRect(this.col, this.row, this.color.toHex());
  }
}
