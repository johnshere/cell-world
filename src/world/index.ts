import { WorldConfig } from '../const/config.ts';
import { createGraph, render } from '../graph/index.ts';
import {
  createPanel,
  updatePerformance,
  updateWorldInfo,
} from '../panel/index.ts';

import ocean from './entities/ocean.ts';

const init = () => {
  console.log('init world');

  let worldTime = 0;
  let lastTime = performance.now();
  let frameCount = 0;
  let totalRenderTime = 0;
  let totalEntityTime = 0;
  let avgFrameTime100 = 0;
  let avgEntityTimePerFrame100 = 0;

  let accelerate = 1;

  // 新增：面板推送节流（与面板500ms刷新节奏对齐）
  const PANEL_UPDATE_INTERVAL_MS = 500;
  let lastPanelUpdateTime = performance.now();

  const targetMs = 1000 / WorldConfig.FrameRate;

  const tick = () => {
    const currentTime = performance.now();
    let deltaTime = currentTime - lastTime;
    lastTime = currentTime;

    const frameStartTime = performance.now();

    deltaTime = deltaTime * accelerate;
    worldTime += deltaTime;
    ocean.update(deltaTime);

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

    if (WorldConfig.IsAutoAccelerate) {
      if (fpsCurrent > WorldConfig.FpsToAccelerate) {
        accelerate += WorldConfig.AccelerateStep;
      } else if (fpsCurrent < WorldConfig.FpsToDecelerate) {
        accelerate -= WorldConfig.AccelerateStep;
      }
      accelerate = Math.max(1, Math.min(WorldConfig.AccelerateMax, accelerate));
    } else {
      accelerate = 1;
    }

    // 新增：对面板推送进行节流（减少跨线程/消息 & 避免每帧全量统计）
    if (currentTime - lastPanelUpdateTime >= PANEL_UPDATE_INTERVAL_MS) {
      lastPanelUpdateTime = currentTime;

      // 使用增量统计，避免遍历所有实体
      const stats = ocean.getEntityStats();

      // 推送到面板（性能 + 世界信息）
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

      updateWorldInfo({
        worldTime,
        accelerate,
        entityCount,
        distribution: {
          plant: stats.plant,
          herbiv: stats.herbiv,
          carniv: stats.carniv,
        },
      });
    }

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
