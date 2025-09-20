import { drawRect } from '../../graph';

import type { Ocean } from './ocean';
import ocean from './ocean';

export default class Entity {
  private _row = 0;
  private _col = 0;
  color = '';
  ocean!: Ocean;
  deltaTime = 0;
  // 标记对象是否处于激活状态（在世界中）
  alive = true;
  // 对象池初始化钩子（子类可覆盖）
  init() {
    this.ocean = ocean;
    this._col = 0;
    this._row = 0;
    this.deltaTime = 0;
    this.color = 'black';
    this.alive = true;
  }
  // 释放到对象池钩子（子类可覆盖）
  releaseToPool() {
    this.alive = false;
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
