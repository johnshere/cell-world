import { FrameRate } from '../const/config.ts';
import { createGraph, render } from '../graph/index.ts';
import {
  createPanel,
  destroy,
  updateMousePosition,
  updateViewportPosition,
} from '../panel/index.ts';

const init = () => {
  console.log('init world');

  const intervalId = setInterval(() => {
    render();
  }, 1000 / FrameRate);

  return intervalId;
};

export function createWorld() {
  createPanel();
  createGraph();

  init();
}

// 保持向后兼容
export default createWorld;
