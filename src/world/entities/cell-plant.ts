import { colord } from 'colord';

import Cell from './cell';

export default class CellPlant extends Cell {
  constructor() {
    super();
    this.color = 'green';
  }
}
