import type { Colord } from 'colord';

import { GridSize } from '../../const/config';
import { drawRect, viewport } from '../../graph';
import { randomColor } from '../../utils';

export default class Entity {
  x: number;
  y: number;
  row: number;
  col: number;
  width: number;
  height: number;
  color: Colord;
  constructor() {
    // 取当前视窗范围，随机生成位置
    this.width = GridSize;
    this.height = GridSize;

    const x = (Math.random() * viewport.width + viewport.x) / GridSize;
    this.col = Math.floor(x);
    this.x = this.col * GridSize;
    const y = (Math.random() * viewport.height + viewport.y) / GridSize;
    this.row = Math.floor(y);
    this.y = this.row * GridSize;

    this.color = randomColor();
  }
  update(deltaTime: number) {}
  render(deltaTime: number) {
    drawRect(this.x, this.y, this.width, this.height, this.color.toHex());
  }
}
