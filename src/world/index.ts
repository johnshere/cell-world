import { WorldConfig } from '../const/config.ts';
import { createGraph, render } from '../graph/index.ts';
import { createPanel } from '../panel/index.ts';

import ocean from './entities/ocean.ts';

const init = () => {
  console.log('init world');

  let lastTime = performance.now();
  let frameCount = 0;
  let totalRenderTime = 0;
  let totalEntityTime = 0;

  const intervalId = setInterval(() => {
    const currentTime = performance.now();
    const deltaTime = currentTime - lastTime;
    lastTime = currentTime;

    const renderStartTime = performance.now();

    ocean.update(deltaTime * WorldConfig.Accelerate);

    if (!document.hidden) {
      render();
    }

    if (frameCount < 1001) {
      const renderEndTime = performance.now();

      // 计算渲染耗时
      const frameRenderTime = renderEndTime - renderStartTime;
      const entityCount = ocean.entities.length;
      const avgEntityTime = entityCount > 0 ? frameRenderTime / entityCount : 0;

      // 累计统计
      frameCount++;
      totalRenderTime += frameRenderTime;
      totalEntityTime += avgEntityTime;

      // 每100帧输出一次性能统计
      if (frameCount % 100 === 0) {
        const avgFrameTime = totalRenderTime / 100;
        const avgEntityTimePerFrame = totalEntityTime / 100;

        console.log(`性能统计 (${frameCount}帧):`);
        console.log(`  平均帧渲染耗时: ${avgFrameTime.toFixed(3)}ms`);
        console.log(
          `  平均每个实体耗时: ${(avgEntityTimePerFrame * 1000).toFixed(3)}μs`
        );
        console.log(`  当前实体数量: ${entityCount}`);
        console.log(`  当前帧渲染耗时: ${frameRenderTime.toFixed(3)}ms`);
        console.log(
          `  当前帧每个实体耗时: ${(avgEntityTime * 1000).toFixed(3)}μs`
        );

        // 重置统计
        totalRenderTime = 0;
        totalEntityTime = 0;
      }
    }
  }, 1000 / WorldConfig.FrameRate);

  return intervalId;
};

export function createWorld() {
  createPanel();
  createGraph();

  init();
}

// 保持向后兼容
export default createWorld;
