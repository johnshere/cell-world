import { FrameRate } from '../const/config.ts';
import Graph from '../graph/index.ts';
import Panel from '../panel/index.ts';

export default class World {
  graph: Graph;
  panel: Panel;
  constructor() {
    this.panel = new Panel();
    this.graph = new Graph();
    this.init();
  }
  init() {
    console.log('init world');
    setInterval(() => {
      this.storm();
    }, 1000 / FrameRate);
  }
  storm() {
    this.graph.drawGrid();
  }
}
