import { FrameRate } from '../const/config.ts';
import { createGraph, render } from '../graph/index.ts';
import { createPanel } from '../panel/index.ts';

import ocean from './objects/ocean.ts';

const init = () => {
  console.log('init world');

  let lastTime = performance.now();

  const intervalId = setInterval(() => {
    const currentTime = performance.now();
    const deltaTime = currentTime - lastTime;
    lastTime = currentTime;

    render();
    ocean.update(deltaTime);
    ocean.render(deltaTime);
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
