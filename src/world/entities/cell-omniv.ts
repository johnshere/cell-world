import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';

export default class CellOmniv extends CellHerbiv {
  preyClasses = [CellPlant, CellHerbiv];
  init() {
    super.init();
    this.name = '杂食'; // 设置name属性
    this.color = 'hotpink';
  }
}
