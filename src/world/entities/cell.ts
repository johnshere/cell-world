import { colord } from 'colord';

import { GridSize } from '../../const/config';
import { viewport } from '../../graph';

import Entity from './entity';

export default class Cell extends Entity {
  constructor() {
    super();

    // 取当前视窗范围，随机生成逻辑位置
    const x = (Math.random() * viewport.width + viewport.x) / GridSize;
    this.col = Math.floor(x);
    const y = (Math.random() * viewport.height + viewport.y) / GridSize;
    this.row = Math.floor(y);

    this.color = 'pink';
  }
  /** 分裂 */
  split() {}
  /** 移动 */
  move() {}
  /** 分离 */
  separate() {}
  /** 对齐 */
  align() {}
  /** 聚集 */
  cohesion() {}
}
