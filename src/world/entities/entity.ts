import { drawRect } from '../../graph';

import type { Ocean } from './ocean';

export default class Entity {
  private _row: number;
  private _col: number;
  color: string;
  ocean!: Ocean;
  deltaTime = 0;
  constructor() {
    this._col = 0;
    this._row = 0;
    this.color = 'black';
  }

  get row() {
    return this._row;
  }
  set row(value: number) {
    if (this._row === value) return;
    const oldRow = this._row;
    this._row = value;
    if (this.ocean) {
      this.ocean.updateEntityPosition(
        this,
        oldRow,
        this._col,
        this._row,
        this._col
      );
    }
  }

  get col() {
    return this._col;
  }
  set col(value: number) {
    if (this._col === value) return;
    const oldCol = this._col;
    this._col = value;
    if (this.ocean) {
      this.ocean.updateEntityPosition(
        this,
        this._row,
        oldCol,
        this._row,
        this._col
      );
    }
  }

  // 一次性设置位置，避免分别设置row/col导致的双次索引更新
  setPosition(row: number, col: number) {
    const oldRow = this._row;
    const oldCol = this._col;
    if (oldRow === row && oldCol === col) return;
    this._row = row;
    this._col = col;
    if (this.ocean) {
      this.ocean.updateEntityPosition(
        this,
        oldRow,
        oldCol,
        this._row,
        this._col
      );
    }
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
