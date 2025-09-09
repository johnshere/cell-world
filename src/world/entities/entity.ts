import { colord, Colord } from 'colord';

import { drawRect } from '../../graph';

export default class Entity {
  row: number;
  col: number;
  color: Colord | string;
  constructor() {
    this.col = 0;
    this.row = 0;
    this.color = 'black';
  }

  update(deltaTime: number) {}
  render() {
    let color = '';
    if (typeof this.color === 'string') {
      color = this.color;
    } else {
      color = this.color.toHex();
    }
    drawRect(this.col, this.row, color);
  }
}
