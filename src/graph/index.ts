import {
  RootEl,
  GridSize,
  GridColor,
  WorldWidth,
  WorldHeight,
} from '../const/config';

export default class Graph {
  el: HTMLCanvasElement;
  constructor() {
    this.el = document.createElement('canvas') as HTMLCanvasElement;
    this.init();
  }
  init() {
    this.el.width = WorldWidth;
    this.el.height = WorldHeight;
    this.el.style.width = '100%';
    this.el.style.height = '100%';
    RootEl.appendChild(this.el);
  }
  drawGrid() {
    const ctx = this.el.getContext('2d')!;
    ctx.strokeStyle = GridColor;
    ctx.lineWidth = 1;
    for (let i = 0; i < this.el.width; i += GridSize) {
      ctx.moveTo(i, 0);
      ctx.lineTo(i, this.el.height);
    }
    for (let i = 0; i < this.el.height; i += GridSize) {
      ctx.moveTo(0, i);
      ctx.lineTo(this.el.width, i);
    }
    ctx.stroke();
  }
}
