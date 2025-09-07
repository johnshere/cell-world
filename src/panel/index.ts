import { RootEl } from '../const/config';

interface PanelData {
  mousePosition: { x: number; y: number };
  viewportPosition: { x: number; y: number; scale: number };
}

const data: PanelData = {
  mousePosition: { x: 0, y: 0 },
  viewportPosition: { x: 0, y: 0, scale: 1 },
};

let el: HTMLDivElement;
let toggleBtn: HTMLButtonElement;
let content: HTMLDivElement;

let isExpanded = true;

export const init = () => {
  // 创建主容器
  el.className = 'debug-panel';
  el.style.cssText = `
      position: fixed;
      top: 0;
      right: 0;
      height: 100vh;
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
      top: 20px;
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

  // 创建内容区域
  content.style.cssText = `
      padding: 20px;
      width: 280px;
      flex: 1;
      overflow-y: auto;
      margin-top: 60px;
    `;

  el.appendChild(content);
  RootEl.appendChild(el);
  RootEl.appendChild(toggleBtn);

  bindEvents();
  updateContent();
};

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
};

export const toggle = () => {
  isExpanded = !isExpanded;

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

export const updateContent = () => {
  const { mousePosition, viewportPosition } = data;

  content.innerHTML = `
      <div style="margin-bottom: 12px;">
        <div style="font-weight: bold; color: #333; margin-bottom: 6px;">🖱️ 鼠标位置</div>
        <div style="color: #666; line-height: 1.4;">
          X: <span style="color: #007acc; font-weight: bold;">${mousePosition.x.toFixed(0)}</span><br>
          Y: <span style="color: #007acc; font-weight: bold;">${mousePosition.y.toFixed(0)}</span>
        </div>
      </div>

      <div>
        <div style="font-weight: bold; color: #333; margin-bottom: 6px;">🔍 视窗信息</div>
        <div style="color: #666; line-height: 1.4;">
          X: <span style="color: #28a745; font-weight: bold;">${viewportPosition.x.toFixed(0)}</span><br>
          Y: <span style="color: #28a745; font-weight: bold;">${viewportPosition.y.toFixed(0)}</span><br>
          缩放: <span style="color: #dc3545; font-weight: bold;">${viewportPosition.scale.toFixed(2)}</span>
        </div>
      </div>
    `;
};

// 更新鼠标位置
export const updateMousePosition = (x: number, y: number) => {
  data.mousePosition = { x, y };
  if (isExpanded) {
    updateContent();
  }
};

// 更新视窗位置
export const updateViewportPosition = (x: number, y: number, scale: number) => {
  data.viewportPosition = { x, y, scale };
  if (isExpanded) {
    updateContent();
  }
};

// 销毁函数
export const destroy = () => {
  el.remove();
  toggleBtn.remove();
};

export function createPanel() {
  el = document.createElement('div');
  toggleBtn = document.createElement('button');
  content = document.createElement('div');

  // 初始化
  init();
}

// 保持向后兼容
export default createPanel;
