import pygame
import pygame_gui
import sys
import time
from config import WINDOW_CONFIG, PANEL_CONFIG, GRID_CONFIG, COORDINATE_CONFIG

# 性能监控变量
frame_times = []
MAX_FRAME_SAMPLES = 60

# 初始化pygame
pygame.init()

# 设置窗口
if WINDOW_CONFIG['fullscreen']:
    screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.FULLSCREEN)
else:
    screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.RESIZABLE)

pygame.display.set_caption(WINDOW_CONFIG['title'])

# 创建UI管理器
ui_manager = pygame_gui.UIManager((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']))

# 创建时钟
clock = pygame.time.Clock()

# 初始化变量
panel_expanded = PANEL_CONFIG['expanded']
panel_width = PANEL_CONFIG['width'] if panel_expanded else 30
grid_offset_x = COORDINATE_CONFIG['grid_offset_x']
grid_offset_y = COORDINATE_CONFIG['grid_offset_y']
dragging = False
drag_start_pos = None

# 预渲染网格（性能优化）
def create_grid_surface(width, height, cell_size, line_color, line_width, bg_color):
    """创建预渲染的网格表面"""
    surface = pygame.Surface((cell_size, cell_size))
    surface.fill(bg_color)
    # 绘制右边和下边的线
    pygame.draw.line(surface, line_color, (cell_size-1, 0), (cell_size-1, cell_size), line_width)
    pygame.draw.line(surface, line_color, (0, cell_size-1), (cell_size, cell_size-1), line_width)
    return surface

# 创建预渲染的网格表面
grid_cell = create_grid_surface(
    GRID_CONFIG['cell_size'],
    GRID_CONFIG['cell_size'],
    GRID_CONFIG['cell_size'],
    GRID_CONFIG['line_color'],
    GRID_CONFIG['line_width'],
    GRID_CONFIG['background_color']
)

# 创建面板展开/收起按钮
panel_toggle_button = pygame_gui.elements.UIButton(
    relative_rect=pygame.Rect(WINDOW_CONFIG['width'] - panel_width, 10, 20, 20),
    text='<' if panel_expanded else '>',
    manager=ui_manager
)

# 创建字体对象（避免重复创建）
font = pygame.font.SysFont(None, COORDINATE_CONFIG['label_size'])

# 预渲染常用标签（性能优化）
label_cache = {}
for i in range(-100, 101, 5):
    if i != 0:  # 跳过0
        label_cache[i] = font.render(str(i), True, COORDINATE_CONFIG['label_color'])

# 主循环
running = True
last_time = time.time()
while running:
    # 计算时间增量
    current_time = time.time()
    time_delta = current_time - last_time
    last_time = current_time
    
    # 记录帧时间（用于FPS计算）
    frame_times.append(time_delta)
    if len(frame_times) > MAX_FRAME_SAMPLES:
        frame_times.pop(0)
    
    # 处理事件
    for event in pygame.event.get():
        # 退出事件
        if event.type == pygame.QUIT:
            running = False
        
        # 键盘事件
        if event.type == pygame.KEYDOWN:
            # ESC键退出全屏或退出程序
            if event.key == pygame.K_ESCAPE:
                if WINDOW_CONFIG['fullscreen']:
                    WINDOW_CONFIG['fullscreen'] = False
                    screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.RESIZABLE)
                else:
                    running = False
            # F11键切换全屏
            elif event.key == pygame.K_F11:
                WINDOW_CONFIG['fullscreen'] = not WINDOW_CONFIG['fullscreen']
                if WINDOW_CONFIG['fullscreen']:
                    screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.FULLSCREEN)
                else:
                    screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.RESIZABLE)
        
        # 鼠标事件
        if event.type == pygame.MOUSEBUTTONDOWN:
            if event.button == 1:  # 左键
                # 检查是否在网格区域内（非面板区域）
                if event.pos[0] < WINDOW_CONFIG['width'] - panel_width:
                    dragging = True
                    drag_start_pos = event.pos
        
        if event.type == pygame.MOUSEBUTTONUP:
            if event.button == 1:  # 左键
                dragging = False
        
        if event.type == pygame.MOUSEMOTION:
            if dragging and drag_start_pos:
                # 计算拖拽偏移量
                dx = event.pos[0] - drag_start_pos[0]
                dy = event.pos[1] - drag_start_pos[1]
                grid_offset_x += dx
                grid_offset_y += dy
                drag_start_pos = event.pos
        
        # 窗口大小改变事件
        if event.type == pygame.VIDEORESIZE:
            if not WINDOW_CONFIG['fullscreen']:
                WINDOW_CONFIG['width'] = event.w
                WINDOW_CONFIG['height'] = event.h
                screen = pygame.display.set_mode((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']), pygame.RESIZABLE)
                ui_manager.set_window_resolution((WINDOW_CONFIG['width'], WINDOW_CONFIG['height']))
                # 更新面板按钮位置
                panel_toggle_button.kill()
                panel_toggle_button = pygame_gui.elements.UIButton(
                    relative_rect=pygame.Rect(WINDOW_CONFIG['width'] - panel_width, 10, 20, 20),
                    text='<' if panel_expanded else '>',
                    manager=ui_manager
                )
        
        # UI事件
        if event.type == pygame.USEREVENT:
            if event.user_type == pygame_gui.UI_BUTTON_PRESSED:
                if event.ui_element == panel_toggle_button:
                    panel_expanded = not panel_expanded
                    panel_width = PANEL_CONFIG['width'] if panel_expanded else 30
                    # 更新面板按钮
                    panel_toggle_button.kill()
                    panel_toggle_button = pygame_gui.elements.UIButton(
                        relative_rect=pygame.Rect(WINDOW_CONFIG['width'] - panel_width, 10, 20, 20),
                        text='<' if panel_expanded else '>',
                        manager=ui_manager
                    )
        
        # 处理UI事件
        ui_manager.process_events(event)
    
    # 更新UI
    ui_manager.update(time_delta)
    
    # 清空屏幕
    screen.fill(GRID_CONFIG['background_color'])
    
    # 绘制网格（使用预渲染的表面）
    draw_area_width = WINDOW_CONFIG['width'] - panel_width
    
    # 计算网格起始和结束位置
    start_x = (grid_offset_x % GRID_CONFIG['cell_size'])
    start_y = (grid_offset_y % GRID_CONFIG['cell_size'])
    
    # 使用预渲染的网格单元格绘制网格（性能优化）
    for y in range(int(start_y) - GRID_CONFIG['cell_size'], WINDOW_CONFIG['height'], GRID_CONFIG['cell_size']):
        for x in range(int(start_x) - GRID_CONFIG['cell_size'], draw_area_width, GRID_CONFIG['cell_size']):
            screen.blit(grid_cell, (x, y))
    
    # 绘制坐标轴
    # X轴
    origin_y = (WINDOW_CONFIG['height'] // 2) + grid_offset_y
    if 0 <= origin_y <= WINDOW_CONFIG['height']:
        pygame.draw.line(screen, COORDINATE_CONFIG['axis_color'], 
                        (0, origin_y), 
                        (draw_area_width, origin_y), 
                        COORDINATE_CONFIG['axis_width'])
    
    # Y轴
    origin_x = (draw_area_width // 2) + grid_offset_x
    if 0 <= origin_x <= draw_area_width:
        pygame.draw.line(screen, COORDINATE_CONFIG['axis_color'], 
                        (origin_x, 0), 
                        (origin_x, WINDOW_CONFIG['height']), 
                        COORDINATE_CONFIG['axis_width'])
    
    # 绘制坐标标签（使用缓存的标签）
    # X轴标签
    for i in range(int(-draw_area_width/2/GRID_CONFIG['cell_size']), int(draw_area_width/2/GRID_CONFIG['cell_size']) + 1):
        if i != 0 and i % 5 == 0:  # 每5个单位显示一个标签
            x_pos = origin_x + i * GRID_CONFIG['cell_size']
            if 0 <= x_pos <= draw_area_width:
                # 使用缓存的标签
                if i in label_cache:
                    label = label_cache[i]
                else:
                    label = font.render(str(i), True, COORDINATE_CONFIG['label_color'])
                    label_cache[i] = label
                
                screen.blit(label, (x_pos - label.get_width() // 2, 
                                    origin_y + COORDINATE_CONFIG['label_offset']))
    
    # Y轴标签
    for i in range(int(-WINDOW_CONFIG['height']/2/GRID_CONFIG['cell_size']), int(WINDOW_CONFIG['height']/2/GRID_CONFIG['cell_size']) + 1):
        if i != 0 and i % 5 == 0:  # 每5个单位显示一个标签
            y_pos = origin_y - i * GRID_CONFIG['cell_size']
            if 0 <= y_pos <= WINDOW_CONFIG['height']:
                # 使用缓存的标签
                if i in label_cache:
                    label = label_cache[i]
                else:
                    label = font.render(str(i), True, COORDINATE_CONFIG['label_color'])
                    label_cache[i] = label
                
                screen.blit(label, (origin_x + COORDINATE_CONFIG['label_offset'], 
                                    y_pos - label.get_height() // 2))
    
    # 绘制面板背景
    pygame.draw.rect(screen, PANEL_CONFIG['background_color'], 
                    (WINDOW_CONFIG['width'] - panel_width, 0, panel_width, WINDOW_CONFIG['height']))
    
    # 绘制面板边框
    pygame.draw.line(screen, PANEL_CONFIG['border_color'], 
                    (WINDOW_CONFIG['width'] - panel_width, 0), 
                    (WINDOW_CONFIG['width'] - panel_width, WINDOW_CONFIG['height']), 
                    PANEL_CONFIG['border_width'])
    
    # 计算并显示FPS
    if frame_times:
        fps = len(frame_times) / sum(frame_times)
        fps_text = font.render(f"FPS: {fps:.1f}", True, (0, 0, 0))
        screen.blit(fps_text, (10, 10))
    
    # 绘制UI
    ui_manager.draw_ui(screen)
    
    # 更新屏幕
    pygame.display.flip()
    
    # 限制帧率
    clock.tick(WINDOW_CONFIG['fps'])

# 退出pygame
pygame.quit()
sys.exit()