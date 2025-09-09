import { colord } from 'colord';

import Cell from './cell';

export default class CellCarniv extends Cell {
  constructor() {
    super();

    this.color = 'red';
  }

  /** 捕猎 */
  hunt() {}
}
