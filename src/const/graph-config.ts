// 图形组件配置
export const GraphConfig = {
  panel: {
    defaultExpanded: true,
  },
  // 网格配置
  grid: {
    size: 4,
    color: '#ccc',
    backgroundColor: '#dcdcdc',
  },

  // 刻度尺配置
  ruler: {
    size: 20, // 刻度尺的宽度/高度
    backgroundColor: '#f5f5f5',
    borderColor: '#ddd',
    lineWidth: 1,
    textFont: '12px Arial',
    textColor: '#333',
    scaleLineColor: '#999',

    // 刻度配置
    scale: {
      thresholds: {
        step10: 20,
        step5: 50,
        step2: 100,
        stepHalf: 200,
        stepFifth: 400,
        stepTenth: 800,
      },
    },
  },

  // 缩放配置
  zoom: {
    minScale: 0.5,
    maxScale: 3,
    scaleFactor: {
      zoomIn: 1.1,
      zoomOut: 0.9,
    },
  },

  // 拖拽配置
  drag: {
    defaultCursor: 'grab',
    draggingCursor: 'grabbing',
  },
};
