import { colord } from 'colord';

import Cell from './cell';

export default class CellHerbiv extends Cell {
  constructor() {
    super();
    this.color = 'brown';
  }
  /** 觅食 */
  eat() {}
}
