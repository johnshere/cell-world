import { RootEl, GridSize, GridColor } from '../const/config';
import { GraphConfig } from '../const/graph-config';
import { updateMousePosition, updateViewportPosition } from '../panel';

interface Point {
  x: number;
  y: number;
}

export interface ViewPort {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

let el: HTMLCanvasElement;
let ctx: CanvasRenderingContext2D;
export const viewport: ViewPort = { x: 0, y: 0, width: 0, height: 0, scale: 1 };
let isDragging = false;
let lastMousePos: Point = { x: 0, y: 0 };
const rulerSize = GraphConfig.ruler.size;

export const resizeCanvas = () => {
  const rect = RootEl.getBoundingClientRect();
  el.width = rect.width;
  el.height = rect.height;
  el.style.width = rect.width + 'px';
  el.style.height = rect.height + 'px';
  viewport.width = rect.width / viewport.scale;
  viewport.height = rect.height / viewport.scale;
  render();
};

export const init = () => {
  resizeCanvas();
  el.style.cursor = GraphConfig.drag.defaultCursor;
  RootEl.appendChild(el);

  // 监听窗口大小变化
  window.addEventListener('resize', resizeCanvas);
};

export const bindEvents = () => {
  // 鼠标按下事件
  el.addEventListener('mousedown', e => {
    isDragging = true;
    lastMousePos = { x: e.clientX, y: e.clientY };
    el.style.cursor = GraphConfig.drag.draggingCursor;
  });

  // 鼠标移动事件
  el.addEventListener('mousemove', e => {
    const rect = el.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    // 报告鼠标位置
    if (updateMousePosition) {
      updateMousePosition(mouseX, mouseY);
    }

    if (isDragging) {
      const deltaX = e.clientX - lastMousePos.x;
      const deltaY = e.clientY - lastMousePos.y;

      viewport.x += deltaX;
      viewport.y += deltaY;

      lastMousePos = { x: e.clientX, y: e.clientY };
      render();

      // 报告视窗变化
      if (updateViewportPosition) {
        updateViewportPosition();
      }
    }
  });

  // 鼠标抬起事件
  el.addEventListener('mouseup', () => {
    isDragging = false;
    el.style.cursor = GraphConfig.drag.defaultCursor;
  });

  // 鼠标离开画布事件
  el.addEventListener('mouseleave', () => {
    isDragging = false;
    el.style.cursor = GraphConfig.drag.defaultCursor;
  });

  // 滚轮缩放事件
  el.addEventListener('wheel', e => {
    e.preventDefault();
    const rect = el.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    const scaleFactor =
      e.deltaY > 0
        ? GraphConfig.zoom.scaleFactor.zoomOut
        : GraphConfig.zoom.scaleFactor.zoomIn;
    const newScale = Math.max(
      GraphConfig.zoom.minScale,
      Math.min(GraphConfig.zoom.maxScale, viewport.scale * scaleFactor)
    );

    // 以鼠标位置为中心进行缩放
    viewport.x = mouseX - (mouseX - viewport.x) * (newScale / viewport.scale);
    viewport.y = mouseY - (mouseY - viewport.y) * (newScale / viewport.scale);

    viewport.scale = newScale;
    // 更新缩放后的视窗在世界坐标系中的宽高
    viewport.width = el.width / newScale;
    viewport.height = el.height / newScale;
    render();

    // 报告视窗变化
    if (updateViewportPosition) {
      updateViewportPosition();
    }
  });
};

export const render = () => {
  ctx.clearRect(0, 0, el.width, el.height);

  // 绘制主画布区域（留出刻度尺空间）
  ctx.save();
  ctx.rect(rulerSize, rulerSize, el.width - rulerSize, el.height - rulerSize);
  ctx.clip();

  // 应用视口变换
  ctx.translate(viewport.x + rulerSize, viewport.y + rulerSize);
  ctx.scale(viewport.scale, viewport.scale);

  // 绘制网格
  drawGrid();

  ctx.restore();

  // 绘制刻度尺
  drawRulers();
};

export const drawGrid = () => {
  ctx.strokeStyle = GridColor;
  ctx.lineWidth = 1 / viewport.scale;

  // 计算可见区域
  const startX =
    Math.floor((-viewport.x - rulerSize) / viewport.scale / GridSize) *
    GridSize;
  const endX =
    Math.ceil((el.width - viewport.x - rulerSize) / viewport.scale / GridSize) *
    GridSize;
  const startY =
    Math.floor((-viewport.y - rulerSize) / viewport.scale / GridSize) *
    GridSize;
  const endY =
    Math.ceil(
      (el.height - viewport.y - rulerSize) / viewport.scale / GridSize
    ) * GridSize;

  ctx.beginPath();

  // 绘制垂直线
  for (let x = startX; x <= endX; x += GridSize) {
    ctx.moveTo(x, startY);
    ctx.lineTo(x, endY);
  }

  // 绘制水平线
  for (let y = startY; y <= endY; y += GridSize) {
    ctx.moveTo(startX, y);
    ctx.lineTo(endX, y);
  }

  ctx.stroke();
};

export const drawRulers = () => {
  // 设置刻度尺背景样式
  ctx.fillStyle = GraphConfig.ruler.backgroundColor;
  ctx.strokeStyle = GraphConfig.ruler.borderColor;
  ctx.lineWidth = GraphConfig.ruler.lineWidth;

  // 绘制顶部刻度尺背景
  ctx.fillRect(0, 0, el.width, rulerSize);
  ctx.strokeRect(0, 0, el.width, rulerSize);

  // 绘制左侧刻度尺背景
  ctx.fillRect(0, 0, rulerSize, el.height);
  ctx.strokeRect(0, 0, rulerSize, el.height);

  // 绘制左上角方块
  ctx.fillRect(0, 0, rulerSize, rulerSize);

  // 设置文字样式
  ctx.font = GraphConfig.ruler.textFont;
  ctx.fillStyle = GraphConfig.ruler.textColor;

  // 计算刻度间距
  const scaleStep = getScaleStep();

  // 绘制顶部刻度
  drawHorizontalRuler(scaleStep);

  // 绘制左侧刻度
  drawVerticalRuler(scaleStep);
};

export const getScaleStep = (): number => {
  const baseStep = GraphConfig.scale.baseStep;
  const scaledStep = baseStep * viewport.scale;
  const thresholds = GraphConfig.scale.thresholds;

  if (scaledStep < thresholds.step10) return baseStep * 10;
  if (scaledStep < thresholds.step5) return baseStep * 5;
  if (scaledStep < thresholds.step2) return baseStep * 2;
  if (scaledStep > thresholds.stepHalf) return baseStep / 2;
  if (scaledStep > thresholds.stepFifth) return baseStep / 5;
  if (scaledStep > thresholds.stepTenth) return baseStep / 10;

  return baseStep;
};

export const drawHorizontalRuler = (step: number) => {
  ctx.strokeStyle = GraphConfig.ruler.scaleLineColor;
  ctx.fillStyle = GraphConfig.ruler.textColor;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  const startX =
    Math.floor((-viewport.x - rulerSize) / viewport.scale / step) * step;
  const endX =
    Math.ceil((el.width - viewport.x - rulerSize) / viewport.scale / step) *
    step;

  for (let x = startX; x <= endX; x += step) {
    const screenX = x * viewport.scale + viewport.x + rulerSize;

    if (screenX >= rulerSize && screenX <= el.width) {
      // 绘制刻度线
      ctx.beginPath();
      ctx.moveTo(screenX, rulerSize - 10);
      ctx.lineTo(screenX, rulerSize);
      ctx.stroke();

      // 绘制刻度数字
      ctx.fillText(x.toString(), screenX, rulerSize / 2);
    }
  }
};

export const drawVerticalRuler = (step: number) => {
  ctx.strokeStyle = GraphConfig.ruler.scaleLineColor;
  ctx.fillStyle = GraphConfig.ruler.textColor;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  const startY =
    Math.floor((-viewport.y - rulerSize) / viewport.scale / step) * step;
  const endY =
    Math.ceil((el.height - viewport.y - rulerSize) / viewport.scale / step) *
    step;

  ctx.save();

  for (let y = startY; y <= endY; y += step) {
    const screenY = y * viewport.scale + viewport.y + rulerSize;

    if (screenY >= rulerSize && screenY <= el.height) {
      // 绘制刻度线
      ctx.beginPath();
      ctx.moveTo(rulerSize - 10, screenY);
      ctx.lineTo(rulerSize, screenY);
      ctx.stroke();

      // 绘制刻度数字（旋转90度）
      ctx.save();
      ctx.translate(rulerSize / 2, screenY);
      ctx.rotate(Math.PI / 2);
      ctx.fillText(y.toString(), 0, 0);
      ctx.restore();
    }
  }

  ctx.restore();
};

// 世界坐标转屏幕坐标
export const worldToScreen = (worldX: number, worldY: number): Point => {
  return {
    x: worldX * viewport.scale + viewport.x + rulerSize,
    y: worldY * viewport.scale + viewport.y + rulerSize,
  };
};

// 屏幕坐标转世界坐标
export const screenToWorld = (screenX: number, screenY: number): Point => {
  return {
    x: (screenX - viewport.x - rulerSize) / viewport.scale,
    y: (screenY - viewport.y - rulerSize) / viewport.scale,
  };
};

// 销毁函数
export const destroy = () => {
  el.remove();
  window.removeEventListener('resize', resizeCanvas);
};

export function createGraph() {
  el = document.createElement('canvas') as HTMLCanvasElement;
  ctx = el.getContext('2d')!;

  // 初始化
  init();
  bindEvents();
}

export const drawRect = (
  x: number,
  y: number,
  width: number,
  height: number,
  color?: string
) => {
  ctx.fillStyle = color || 'white';
  ctx.fillRect(x, y, width, height);
};

// 保持向后兼容
export default createGraph;
