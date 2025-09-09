import Cell from './cell';

/** 食肉细胞 */
export default class CellCarniv extends Cell {
  constructor() {
    super();

    this.color = 'red';
  }

  /** 捕猎 */
  hunt() {}
}
