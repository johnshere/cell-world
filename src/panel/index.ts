import { RootEl, WorldConfig } from '../const/config';
import { GraphConfig } from '../const/graph-config';
import { viewport } from '../graph';

interface PanelData {
  mousePosition: { x: number; y: number };
  perf: {
    frameCount: number;
    currentFrameTime: number; // ms
    currentEntityTime: number; // ms per entity (frameRenderTime/entityCount)
    avgFrameTime100: number; // ms (avg of last 100 frames)
    avgEntityTimePerFrame100: number; // ms per entity (avg of last 100 frames)
    currentEntityCount: number;
    fpsCurrent: number; // frames per second (based on current frame time)
    avgFps100: number; // average fps over last 100 frames
  };
  world: {
    worldTime: number; // 世界时长
    accelerate: number; // 当前加速比率
    entityCount: number; // 当前实体数量（世界维度）
    distribution: {
      plant: number;
      herbiv: number;
      carniv: number;
    };
  };
}

const data: PanelData = {
  mousePosition: { x: 0, y: 0 },
  perf: {
    frameCount: 0,
    currentFrameTime: 0,
    currentEntityTime: 0,
    avgFrameTime100: 0,
    avgEntityTimePerFrame100: 0,
    currentEntityCount: 0,
    fpsCurrent: 0,
    avgFps100: 0,
  },
  world: {
    worldTime: 0,
    accelerate: 1,
    entityCount: 0,
    distribution: {
      plant: 0,
      herbiv: 0,
      carniv: 0,
    },
  },
};

let el: HTMLDivElement;
let toggleBtn: HTMLButtonElement;
let accelerateBtn: HTMLButtonElement;
let content: HTMLDivElement;

// 缓存面板中各数值节点的引用，避免每次重绘
let mouseXEl: HTMLSpanElement;
let mouseYEl: HTMLSpanElement;
let frameCountEl: HTMLSpanElement;
let frameTimeEl: HTMLSpanElement;
let fpsEl: HTMLSpanElement;
let avgFrameTimeEl: HTMLSpanElement;
let avgFpsEl: HTMLSpanElement;
let entityTimeEl: HTMLSpanElement;
let avgEntityTimeEl: HTMLSpanElement;
let entityCountEl: HTMLSpanElement;
let viewportXEl: HTMLSpanElement;
let viewportYEl: HTMLSpanElement;
let viewportWEl: HTMLSpanElement;
let viewportHEl: HTMLSpanElement;
let viewportScaleEl: HTMLSpanElement;
let worldTimeEl: HTMLSpanElement;
let worldAccelerateEl: HTMLSpanElement;
let worldEntityCountEl: HTMLSpanElement;
let worldPlantCountEl: HTMLSpanElement;
let worldHerbivCountEl: HTMLSpanElement;
let worldCarnivCountEl: HTMLSpanElement;

let isExpanded = true;

export const init = () => {
  // 创建主容器
  el.className = 'debug-panel';
  el.style.cssText = `
      position: fixed;
      top: 20px;
      right: 0;
      background: rgba(255, 255, 255, 0.95);
      border-left: 1px solid #ddd;
      box-shadow: -4px 0 12px rgba(0, 0, 0, 0.15);
      font-family: 'Consolas', 'Monaco', monospace;
      font-size: 12px;
      z-index: 1000;
      transition: transform 0.3s ease;
      backdrop-filter: blur(10px);
      transform: translateX(0);
      display: flex;
      flex-direction: column;
    `;

  // 创建切换按钮
  toggleBtn.innerHTML = 'x';
  toggleBtn.style.cssText = `
      position: fixed;
      top: 40px;
      right: 20px;
      width: 40px;
      height: 40px;
      border: none;
      background: #007acc;
      color: white;
      border-radius: 8px;
      cursor: pointer;
      font-size: 16px;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.2s ease;
      z-index: 1001;
    `;

  // 创建加速控制按钮
  accelerateBtn.innerHTML = '⚡';
  accelerateBtn.style.cssText = `
      position: fixed;
      top: 40px;
      right: 70px;
      width: 40px;
      height: 40px;
      border: none;
      background: #28a745;
      color: white;
      border-radius: 8px;
      cursor: pointer;
      font-size: 16px;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.2s ease;
      z-index: 1001;
    `;

  // 创建内容区域
  content.style.cssText = `
      padding: 10px;
      width: 280px;
      flex: 1;
      overflow-y: auto;
      margin-top: 30px;
    `;

  // 一次性构建结构并缓存节点
  content.append(
    createSection('🌍 世界信息', [
      ['世界时长: ', (worldTimeEl = createValueSpan('red'))],
      ['加速比率: ', (worldAccelerateEl = createValueSpan('#ff3d00'))],
      ['实体数量: ', (worldEntityCountEl = createValueSpan('#d73a49'))],
      ['植物: ', (worldPlantCountEl = createValueSpan('#2e7d32'))],
      ['草食: ', (worldHerbivCountEl = createValueSpan('#20a4f3'))],
      ['肉食: ', (worldCarnivCountEl = createValueSpan('#d73a49'))],
    ]),
    createSection('⚡ 性能统计', [
      ['帧数: ', (frameCountEl = createValueSpan('#6f42c1'))],
      ['当前帧耗时: ', (frameTimeEl = createValueSpan('#ff6b35')), ' ms'],
      ['FPS(当前): ', (fpsEl = createValueSpan('#2e7d32'))],
      [
        '平均帧耗时(每100): ',
        (avgFrameTimeEl = createValueSpan('#ff6b35')),
        ' ms',
      ],
      ['FPS(平均每100): ', (avgFpsEl = createValueSpan('#2e7d32'))],
      [
        '每个实体耗时(当前): ',
        (entityTimeEl = createValueSpan('#20a4f3')),
        ' μs',
      ],
      [
        '每个实体耗时(平均每帧): ',
        (avgEntityTimeEl = createValueSpan('#20a4f3')),
        ' μs',
      ],
      ['实体数量: ', (entityCountEl = createValueSpan('#d73a49'))],
    ]),
    createSection('🔍 视窗信息', [
      ['X: ', (viewportXEl = createValueSpan('#28a745'))],
      ['Y: ', (viewportYEl = createValueSpan('#28a745'))],
      ['宽度: ', (viewportWEl = createValueSpan('#28a745'))],
      ['高度: ', (viewportHEl = createValueSpan('#28a745'))],
      ['缩放: ', (viewportScaleEl = createValueSpan('#dc3545'))],
      ['鼠标位置X: ', (mouseXEl = createValueSpan('#007acc'))],
      ['鼠标位置Y: ', (mouseYEl = createValueSpan('#007acc'))],
    ])
  );

  el.appendChild(content);
  RootEl.appendChild(el);
  RootEl.appendChild(toggleBtn);
  RootEl.appendChild(accelerateBtn);

  bindEvents();
  updateContent();
  toggle(GraphConfig.panel.defaultExpanded);

  // 初始化加速按钮状态
  if (WorldConfig.IsAutoAccelerate) {
    accelerateBtn.style.background = '#28a745';
    accelerateBtn.innerHTML = '⚡';
    accelerateBtn.title = '自动加速已开启，点击关闭';
  } else {
    accelerateBtn.style.background = '#dc3545';
    accelerateBtn.innerHTML = '⏸';
    accelerateBtn.title = '自动加速已关闭，点击开启';
  }
};

function createSection(
  title: string,
  rows: Array<[string, HTMLSpanElement, string?]>
) {
  const section = document.createElement('div');
  section.style.marginBottom = '6px';

  const titleEl = document.createElement('div');
  titleEl.style.fontWeight = 'bold';
  titleEl.style.color = '#333';
  titleEl.style.marginBottom = '6px';
  titleEl.textContent = title;
  section.appendChild(titleEl);

  const bodyEl = document.createElement('div');
  bodyEl.style.color = '#666';
  bodyEl.style.lineHeight = '1.4';

  rows.forEach(([label, valueEl, unit]) => {
    const line = document.createElement('div');
    const labelEl = document.createElement('span');
    labelEl.textContent = label;

    line.appendChild(labelEl);
    line.appendChild(valueEl);
    if (unit) {
      const unitEl = document.createElement('span');
      unitEl.textContent = unit;
      line.appendChild(unitEl);
    }

    bodyEl.appendChild(line);
  });

  section.appendChild(bodyEl);
  return section;
}

function createValueSpan(color: string) {
  const span = document.createElement('span');
  span.style.color = color;
  span.style.fontWeight = 'bold';
  span.textContent = '-';
  return span as HTMLSpanElement;
}

export const bindEvents = () => {
  // 切换按钮事件
  toggleBtn.addEventListener('click', () => {
    toggle();
  });

  // 按钮悬停效果
  toggleBtn.addEventListener('mouseenter', () => {
    toggleBtn.style.background = '#005a9e';
    toggleBtn.style.transform = 'scale(1.05)';
  });

  toggleBtn.addEventListener('mouseleave', () => {
    toggleBtn.style.background = '#007acc';
    toggleBtn.style.transform = 'scale(1)';
  });

  // 加速按钮事件
  accelerateBtn.addEventListener('click', () => {
    toggleAccelerate();
  });

  // 加速按钮悬停效果
  accelerateBtn.addEventListener('mouseenter', () => {
    accelerateBtn.style.transform = 'scale(1.05)';
  });

  accelerateBtn.addEventListener('mouseleave', () => {
    accelerateBtn.style.transform = 'scale(1)';
  });
};

export const toggle = (toExpanded?: boolean) => {
  if (toExpanded !== undefined) {
    isExpanded = toExpanded;
  } else {
    isExpanded = !isExpanded;
  }

  if (isExpanded) {
    el.style.transform = 'translateX(0)';
    toggleBtn.innerHTML = '✕';
    el.style.background = 'rgba(255, 255, 255, 0.98)';
  } else {
    el.style.transform = 'translateX(100%)';
    toggleBtn.innerHTML = '📊';
    el.style.background = 'rgba(255, 255, 255, 0.95)';
  }
};

export const toggleAccelerate = () => {
  WorldConfig.IsAutoAccelerate = !WorldConfig.IsAutoAccelerate;

  if (WorldConfig.IsAutoAccelerate) {
    accelerateBtn.style.background = '#28a745';
    accelerateBtn.innerHTML = '⚡';
    accelerateBtn.title = '自动加速已开启，点击关闭';
  } else {
    accelerateBtn.style.background = '#dc3545';
    accelerateBtn.innerHTML = '⏸';
    accelerateBtn.title = '自动加速已关闭，点击开启';
  }
};

/** 时间单位进位功能（毫秒->秒->分钟->小时）。 */
export const formatTime = (ms: number) => {
  if (ms < 1000) {
    return `${ms.toFixed(0)} ms`;
  }
  const s = ms / 1000;
  if (s < 60) {
    return `${s.toFixed(1)} sec`;
  }
  const m = s / 60;
  if (m < 60) {
    return `${m.toFixed(1)} min`;
  }
  const h = m / 60;
  return `${h.toFixed(1)} hour`;
};
export const updateContent = () => {
  const { mousePosition, perf, world } = data;

  // 只更新文本，避免整块重绘
  if (mouseXEl) mouseXEl.textContent = mousePosition.x.toFixed(0);
  if (mouseYEl) mouseYEl.textContent = mousePosition.y.toFixed(0);

  if (frameCountEl) frameCountEl.textContent = `${perf.frameCount}`;
  if (frameTimeEl) frameTimeEl.textContent = perf.currentFrameTime.toFixed(3);
  if (fpsEl) fpsEl.textContent = perf.fpsCurrent.toFixed(1);
  if (avgFrameTimeEl)
    avgFrameTimeEl.textContent = perf.avgFrameTime100.toFixed(3);
  if (avgFpsEl) avgFpsEl.textContent = perf.avgFps100.toFixed(1);
  if (entityTimeEl)
    entityTimeEl.textContent = (perf.currentEntityTime * 1000).toFixed(3);
  if (avgEntityTimeEl)
    avgEntityTimeEl.textContent = (
      perf.avgEntityTimePerFrame100 * 1000
    ).toFixed(3);
  if (entityCountEl) entityCountEl.textContent = `${perf.currentEntityCount}`;

  if (viewportXEl) viewportXEl.textContent = viewport.x.toFixed(0);
  if (viewportYEl) viewportYEl.textContent = viewport.y.toFixed(0);
  if (viewportWEl) viewportWEl.textContent = viewport.width.toFixed(0);
  if (viewportHEl) viewportHEl.textContent = viewport.height.toFixed(0);
  if (viewportScaleEl) viewportScaleEl.textContent = viewport.scale.toFixed(2);

  // 世界信息
  if (worldTimeEl) {
    worldTimeEl.textContent = formatTime(world.worldTime);
  }
  if (worldAccelerateEl)
    worldAccelerateEl.textContent = world.accelerate.toFixed(2);
  if (worldEntityCountEl)
    worldEntityCountEl.textContent = `${world.entityCount}`;
  if (worldPlantCountEl)
    worldPlantCountEl.textContent = `${world.distribution.plant}`;
  if (worldHerbivCountEl)
    worldHerbivCountEl.textContent = `${world.distribution.herbiv}`;
  if (worldCarnivCountEl)
    worldCarnivCountEl.textContent = `${world.distribution.carniv}`;
};
setInterval(updateContent, 500);

// 更新鼠标位置（不再主动触发重绘）
export const updateMousePosition = (x: number, y: number) => {
  data.mousePosition = { x, y };
};

// 更新视窗位置（不再主动触发重绘）
export const updateViewportPosition = () => {
  // 仅依赖定时器刷新
};

// 更新性能数据（不再主动触发重绘）
export const updatePerformance = (perf: Partial<PanelData['perf']>) => {
  data.perf = { ...data.perf, ...perf };
};

// 更新世界信息（不再主动触发重绘）
export const updateWorldInfo = (world: Partial<PanelData['world']>) => {
  data.world = { ...data.world, ...world };
};

// 销毁函数
export const destroy = () => {
  el.remove();
  toggleBtn.remove();
};

export function createPanel() {
  el = document.createElement('div');
  toggleBtn = document.createElement('button');
  accelerateBtn = document.createElement('button');
  content = document.createElement('div');

  // 初始化
  init();
}

// 保持向后兼容
export default createPanel;
