import { GridSize } from '../../const/config';
import { viewport } from '../../graph';
import { randomColor } from '../../utils';

import Entity from './entity';

export default class Creature extends Entity {
  constructor() {
    super();

    // 取当前视窗范围，随机生成逻辑位置
    const x = (Math.random() * viewport.width + viewport.x) / GridSize;
    this.col = Math.floor(x);
    const y = (Math.random() * viewport.height + viewport.y) / GridSize;
    this.row = Math.floor(y);

    this.color = randomColor();
  }
}
