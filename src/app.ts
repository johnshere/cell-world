// 加载配置
import configData from '../config.json' with { type: 'json' };

interface Config {
    window: { width: number; height: number; title: string };
    panel: { width: number; defaultExpanded: boolean; animationDuration: number };
    grid: { cellSize: number; lineColor: string; backgroundColor: string; axisColor: string; axisWidth: number };
    viewport: { initialX: number; initialY: number; zoomMin: number; zoomMax: number; zoomStep: number };
    interaction: { dragSensitivity: number; scrollSensitivity: number };
}

const config: Config = configData || {
        window: { width: 1200, height: 800, title: 'Cell World' },
        panel: { width: 400, defaultExpanded: true, animationDuration: 300 },
        grid: { cellSize: 10, lineColor: '#ddd', backgroundColor: '#fff', axisColor: '#333', axisWidth: 2 },
        viewport: { initialX: 0, initialY: 0, zoomMin: 0.1, zoomMax: 5.0, zoomStep: 0.1 },
        interaction: { dragSensitivity: 1.0, scrollSensitivity: 0.1 }
    };

interface Viewport {
    x: number;
    y: number;
    zoom: number;
}

interface Position {
    x: number;
    y: number;
}

class CellWorld {
    private canvas: HTMLCanvasElement;
    private ctx: CanvasRenderingContext2D;
    private sidePanel: HTMLElement;
    private panelToggleBtn: HTMLElement;
    private viewport: Viewport;
    private isDragging: boolean;
    private lastMousePos: Position;
    private mousePos: Position;
    private gridSize: number;

    constructor() {
        this.canvas = document.getElementById('grid-canvas') as HTMLCanvasElement;
        this.ctx = this.canvas.getContext('2d') as CanvasRenderingContext2D;
        this.sidePanel = document.getElementById('side-panel') as HTMLElement;
        this.panelToggleBtn = document.getElementById('panel-toggle-btn') as HTMLElement;
        
        // 视口状态
        this.viewport = {
            x: config.viewport.initialX,
            y: config.viewport.initialY,
            zoom: 1.0
        };
        
        // 拖拽状态
        this.isDragging = false;
        this.lastMousePos = { x: 0, y: 0 };
        this.mousePos = { x: 0, y: 0 };
        
        // 网格配置
        this.gridSize = config.grid.cellSize;
        
        this.init();
    }
    
    private init(): void {
        this.setupCanvas();
        this.setupEventListeners();
        this.setupPanel();
        this.draw();
        this.updateInfo();
    }
    
    private setupCanvas(): void {
        const container = document.getElementById('canvas-container');
        if (!container) return;
        const rect = container.getBoundingClientRect();
        
        this.canvas.width = rect.width;
        this.canvas.height = rect.height;
        
        // 监听窗口大小变化
        window.addEventListener('resize', () => {
            setTimeout(() => {
                if (!container) return;
                const newRect = container.getBoundingClientRect();
                this.canvas.width = newRect.width;
                this.canvas.height = newRect.height;
                this.draw();
            }, 100);
        });
    }
    
    private setupEventListeners(): void {
        // 鼠标事件
        this.canvas.addEventListener('mousedown', this.onMouseDown.bind(this));
        this.canvas.addEventListener('mousemove', this.onMouseMove.bind(this));
        this.canvas.addEventListener('mouseup', this.onMouseUp.bind(this));
        this.canvas.addEventListener('wheel', this.onWheel.bind(this));
        
        // 防止右键菜单
        this.canvas.addEventListener('contextmenu', (e: Event) => e.preventDefault());
        
        // 控制按钮事件
        const resetViewBtn = document.getElementById('reset-view') as HTMLButtonElement;
        const centerOriginBtn = document.getElementById('center-origin') as HTMLButtonElement;
        resetViewBtn.addEventListener('click', this.resetView.bind(this));
        centerOriginBtn.addEventListener('click', this.centerOrigin.bind(this));
        
        // 网格大小控制
        const gridSizeInput = document.getElementById('grid-size-input') as HTMLInputElement;
        const gridSizeValue = document.getElementById('grid-size-value') as HTMLElement;
        gridSizeInput.addEventListener('input', (e: Event) => {
            const target = e.target as HTMLInputElement;
            this.gridSize = parseInt(target.value);
            gridSizeValue.textContent = this.gridSize.toString();
            this.draw();
            this.updateInfo();
        });
    }
    
    private setupPanel(): void {
        // 面板切换功能
        this.panelToggleBtn.addEventListener('click', () => {
            this.sidePanel.classList.toggle('panel-collapsed');
            // 延迟重绘以等待CSS动画完成
            setTimeout(() => {
                this.setupCanvas();
                this.draw();
            }, config.panel.animationDuration);
        });
        
        // 设置初始状态
        if (!config.panel.defaultExpanded) {
            this.sidePanel.classList.add('panel-collapsed');
        }
    }
    
    private onMouseDown(e: MouseEvent): void {
        this.isDragging = true;
        this.lastMousePos = { x: e.clientX, y: e.clientY };
        this.canvas.style.cursor = 'grabbing';
    }
    
    private onMouseMove(e: MouseEvent): void {
        // 更新鼠标位置
        const rect = this.canvas.getBoundingClientRect();
        this.mousePos.x = e.clientX - rect.left;
        this.mousePos.y = e.clientY - rect.top;
        
        // 更新坐标显示
        this.updateCoordinatesDisplay();
        
        if (this.isDragging) {
            const deltaX = (e.clientX - this.lastMousePos.x) * config.interaction.dragSensitivity;
            const deltaY = (e.clientY - this.lastMousePos.y) * config.interaction.dragSensitivity;
            
            this.viewport.x += deltaX;
            this.viewport.y += deltaY;
            
            this.lastMousePos = { x: e.clientX, y: e.clientY };
            this.draw();
            this.updateInfo();
        }
    }
    
    private onMouseUp(e: MouseEvent): void {
        this.isDragging = false;
        this.canvas.style.cursor = 'grab';
    }
    
    private onWheel(e: WheelEvent): void {
        e.preventDefault();
        
        const zoomFactor = e.deltaY > 0 ? 0.9 : 1.1;
        const newZoom = this.viewport.zoom * zoomFactor;
        
        if (newZoom >= config.viewport.zoomMin && newZoom <= config.viewport.zoomMax) {
            // 以鼠标位置为中心缩放
            const rect = this.canvas.getBoundingClientRect();
            const mouseX = e.clientX - rect.left;
            const mouseY = e.clientY - rect.top;
            
            // 计算缩放前的世界坐标
            const worldX = (mouseX - this.viewport.x) / this.viewport.zoom;
            const worldY = (mouseY - this.viewport.y) / this.viewport.zoom;
            
            this.viewport.zoom = newZoom;
            
            // 调整视口位置以保持鼠标位置不变
            this.viewport.x = mouseX - worldX * this.viewport.zoom;
            this.viewport.y = mouseY - worldY * this.viewport.zoom;
            
            this.draw();
            this.updateInfo();
        }
    }
    
    private draw(): void {
        // 清空画布
        this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
        
        // 设置背景色
        this.ctx.fillStyle = config.grid.backgroundColor;
        this.ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
        
        // 绘制网格
        this.drawGrid();
        
        // 绘制坐标轴
        this.drawAxes();
    }
    
    private drawGrid(): void {
        const scaledGridSize = this.gridSize * this.viewport.zoom;
        
        // 如果网格太小，不绘制
        if (scaledGridSize < 2) return;
        
        this.ctx.strokeStyle = config.grid.lineColor;
        this.ctx.lineWidth = 1;
        this.ctx.beginPath();
        
        // 计算网格起始位置
        const startX = this.viewport.x % scaledGridSize;
        const startY = this.viewport.y % scaledGridSize;
        
        // 绘制垂直线
        for (let x = startX; x < this.canvas.width; x += scaledGridSize) {
            this.ctx.moveTo(x, 0);
            this.ctx.lineTo(x, this.canvas.height);
        }
        
        // 绘制水平线
        for (let y = startY; y < this.canvas.height; y += scaledGridSize) {
            this.ctx.moveTo(0, y);
            this.ctx.lineTo(this.canvas.width, y);
        }
        
        this.ctx.stroke();
    }
    
    private drawAxes(): void {
        this.ctx.strokeStyle = config.grid.axisColor;
        this.ctx.lineWidth = config.grid.axisWidth;
        this.ctx.beginPath();
        
        // 在视口边缘绘制坐标轴
        // 底部X轴
        this.ctx.moveTo(0, this.canvas.height);
        this.ctx.lineTo(this.canvas.width, this.canvas.height);
        
        // 左侧Y轴
        this.ctx.moveTo(0, 0);
        this.ctx.lineTo(0, this.canvas.height);
        
        this.ctx.stroke();
        
        // 绘制坐标标签和刻度
        this.drawAxisLabels();
    }
    
    private drawAxisLabels(): void {
        const scaledGridSize = this.gridSize * this.viewport.zoom;
        
        // 如果网格太小，不绘制标签
        if (scaledGridSize < 20) return;
        
        this.ctx.fillStyle = config.grid.axisColor;
        this.ctx.font = '12px Arial';
        this.ctx.strokeStyle = config.grid.axisColor;
        this.ctx.lineWidth = 1;
        
        // X轴标签和刻度（底部）
        this.ctx.textAlign = 'center';
        this.ctx.textBaseline = 'top';
        
        for (let x = this.viewport.x % scaledGridSize; x < this.canvas.width; x += scaledGridSize) {
            const worldX = Math.round((x - this.viewport.x) / this.viewport.zoom / this.gridSize) * this.gridSize;
            
            // 绘制刻度线
            this.ctx.beginPath();
            this.ctx.moveTo(x, this.canvas.height - 5);
            this.ctx.lineTo(x, this.canvas.height);
            this.ctx.stroke();
            
            // 绘制标签
            this.ctx.fillText(worldX.toString(), x, this.canvas.height - 18);
        }
        
        // Y轴标签和刻度（左侧）
        this.ctx.textAlign = 'left';
        this.ctx.textBaseline = 'middle';
        
        for (let y = this.viewport.y % scaledGridSize; y < this.canvas.height; y += scaledGridSize) {
            const worldY = -Math.round((y - this.viewport.y) / this.viewport.zoom / this.gridSize) * this.gridSize;
            
            // 绘制刻度线
            this.ctx.beginPath();
            this.ctx.moveTo(0, y);
            this.ctx.lineTo(5, y);
            this.ctx.stroke();
            
            // 绘制标签
            this.ctx.fillText(worldY.toString(), 8, y);
        }
    }
    
    private updateCoordinatesDisplay(): void {
        // 计算世界坐标
        const worldX = Math.round((this.mousePos.x - this.viewport.x) / this.viewport.zoom);
        const worldY = -Math.round((this.mousePos.y - this.viewport.y) / this.viewport.zoom);
        
        const mouseCoordsEl = document.getElementById('mouse-coords') as HTMLElement;
        const viewportCoordsEl = document.getElementById('viewport-coords') as HTMLElement;
        
        mouseCoordsEl.textContent = `坐标: (${worldX}, ${worldY})`;
        
        const viewportX = Math.round(-this.viewport.x / this.viewport.zoom);
        const viewportY = Math.round(this.viewport.y / this.viewport.zoom);
        viewportCoordsEl.textContent = `视口: (${viewportX}, ${viewportY})`;
    }
    
    private updateInfo(): void {
        const gridSizeEl = document.getElementById('grid-size') as HTMLElement;
        const viewportPositionEl = document.getElementById('viewport-position') as HTMLElement;
        const zoomLevelEl = document.getElementById('zoom-level') as HTMLElement;
        
        gridSizeEl.textContent = `${this.gridSize}px`;
        
        const viewportX = Math.round(-this.viewport.x / this.viewport.zoom);
        const viewportY = Math.round(this.viewport.y / this.viewport.zoom);
        viewportPositionEl.textContent = `(${viewportX}, ${viewportY})`;
        
        const zoomPercent = Math.round(this.viewport.zoom * 100);
        zoomLevelEl.textContent = `${zoomPercent}%`;
    }
    
    private resetView(): void {
        this.viewport.x = config.viewport.initialX;
        this.viewport.y = config.viewport.initialY;
        this.viewport.zoom = 1.0;
        this.draw();
        this.updateInfo();
    }
    
    private centerOrigin(): void {
        this.viewport.x = this.canvas.width / 2;
        this.viewport.y = this.canvas.height / 2;
        this.draw();
        this.updateInfo();
    }
}

// 初始化应用
document.addEventListener('DOMContentLoaded', () => {
    new CellWorld();
});