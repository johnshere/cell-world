import { drawRect } from '../../graph';

import type { Ocean } from './ocean';

export default class Entity {
  row: number;
  col: number;
  color: string;
  ocean!: Ocean;
  deltaTime = 0;
  constructor() {
    this.col = 0;
    this.row = 0;
    this.color = 'black';
  }

  update(deltaTime: number) {
    this.deltaTime = deltaTime;
  }
  render() {
    let color = '';
    if (typeof this.color === 'string') {
      color = this.color;
    } else {
      // color = this.color.toHex();
    }
    drawRect(this.col, this.row, color);
  }
}
