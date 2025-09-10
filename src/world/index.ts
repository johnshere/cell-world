import { WorldConfig } from '../const/config.ts';
import { createGraph, render } from '../graph/index.ts';
import { createPanel, updatePerformance } from '../panel/index.ts';

import ocean from './entities/ocean.ts';

const init = () => {
  console.log('init world');

  let lastTime = performance.now();
  let frameCount = 0;
  let totalRenderTime = 0;
  let totalEntityTime = 0;
  let avgFrameTime100 = 0;
  let avgEntityTimePerFrame100 = 0;

  let accelerate = 1;

  const targetMs = 1000 / WorldConfig.FrameRate;

  const tick = () => {
    const currentTime = performance.now();
    const deltaTime = currentTime - lastTime;
    lastTime = currentTime;

    const frameStartTime = performance.now();

    ocean.update(deltaTime * accelerate);

    if (!document.hidden) {
      render();
    }

    // 结束计时（包含update+render耗时）
    const frameEndTime = performance.now();
    const frameRenderTime = frameEndTime - frameStartTime;

    const entityCount = ocean.entities.length;
    const currentEntityTime =
      entityCount > 0 ? frameRenderTime / entityCount : 0;

    // 统计与滚动平均（每100帧）
    frameCount++;
    totalRenderTime += frameRenderTime;
    totalEntityTime += currentEntityTime;

    if (frameCount % 100 === 0) {
      avgFrameTime100 = totalRenderTime / 100;
      avgEntityTimePerFrame100 = totalEntityTime / 100;
      // 重置统计
      totalRenderTime = 0;
      totalEntityTime = 0;
    }

    // 计算 FPS
    const fpsCurrent = frameRenderTime > 0 ? 1000 / frameRenderTime : 0;
    const avgFps100 = avgFrameTime100 > 0 ? 1000 / avgFrameTime100 : 0;

    if (WorldConfig.isAccelerate) {
      if (fpsCurrent > 10) {
        accelerate++;
      } else if (fpsCurrent < 5) {
        accelerate--;
      }
      accelerate = Math.max(1, Math.min(10, accelerate));
    }

    // 推送到面板
    updatePerformance({
      frameCount,
      currentFrameTime: frameRenderTime,
      currentEntityTime,
      avgFrameTime100,
      avgEntityTimePerFrame100,
      currentEntityCount: entityCount,
      fpsCurrent,
      avgFps100,
    });

    // 根据上一帧耗时动态调度下一帧：休眠 = 目标帧时间 - 本帧耗时
    const sleep = Math.max(0, targetMs - frameRenderTime);
    setTimeout(tick, sleep);
  };

  tick();
};

export function createWorld() {
  createPanel();
  createGraph();

  init();
}

// 保持向后兼容
export default createWorld;
