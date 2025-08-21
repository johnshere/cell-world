import random
import pygame
from creature.creature import Creature

class World:
    """游戏世界类"""
    
    def __init__(self, config):
        """初始化游戏世界
        
        Args:
            config: 配置对象
        """
        self.config = config
        self.creatures = []
        self.width = config.width // config.unit
        self.height = config.height // config.unit
        self.running = True
        
        # 初始化Pygame
        pygame.init()
        self.screen = pygame.display.set_mode((config.width, config.height))
        pygame.display.set_caption(config.title)
        self.clock = pygame.time.Clock()
        
        # 创建初始生物
        self.create_initial_creatures()
    
    def create_initial_creatures(self, count=5):
        """创建初始生物
        
        Args:
            count: 初始生物数量
        """
        for _ in range(count):
            x = random.randint(0, self.width - 5)
            y = random.randint(0, self.height - 5)
            size = random.randint(3, min(6, self.config.creature_max_lines))
            color = random.choice(self.config.cell_colors)
            
            creature = Creature(x, y, size, color, self.config)
            self.creatures.append(creature)
    
    def update(self):
        """更新游戏世界"""
        # 更新所有生物
        for creature in self.creatures:
            creature.update()
        
        # 检查分裂
        new_creatures = []
        for i, creature in enumerate(self.creatures):
            split_result = creature.check_split()
            if split_result:
                # 移除原生物，添加分裂后的生物
                self.creatures.pop(i)
                new_creatures.extend(split_result)
        
        self.creatures.extend(new_creatures)
        
        # 检查捕食
        for i, predator in enumerate(self.creatures):
            for j, prey in enumerate(self.creatures):
                if i != j and predator.can_eat(prey):
                    predator.eat(prey)
                    # 移除被吃的生物
                    self.creatures.pop(j)
                    break
        
        # 限制生物数量
        if len(self.creatures) > 20:
            # 移除一些老的或小的生物
            self.creatures.sort(key=lambda c: c.age * c.get_cell_count())
            self.creatures = self.creatures[:20]
    
    def render(self):
        """渲染游戏世界"""
        # 清空屏幕
        self.screen.fill((0, 0, 0))
        
        # 绘制网格
        if self.config.grid:
            grid_color = pygame.Color(self.config.grid_color)
            for x in range(0, self.config.width, self.config.unit):
                pygame.draw.line(self.screen, grid_color, (x, 0), (x, self.config.height))
            for y in range(0, self.config.height, self.config.unit):
                pygame.draw.line(self.screen, grid_color, (0, y), (self.config.width, y))
        
        # 绘制所有生物
        for creature in self.creatures:
            self.draw_creature(creature)
        
        # 更新显示
        pygame.display.flip()
    
    def draw_creature(self, creature):
        """绘制单个生物
        
        Args:
            creature: 要绘制的生物
        """
        rows, cols = creature.cells.shape
        cell_color = pygame.Color(creature.color)
        
        for i in range(rows):
            for j in range(cols):
                if creature.cells[i, j]:
                    rect = pygame.Rect(
                        (creature.x + j) * self.config.unit,
                        (creature.y + i) * self.config.unit,
                        self.config.unit,
                        self.config.unit
                    )
                    pygame.draw.rect(self.screen, cell_color, rect, width=self.config.stroke_width)
    
    def handle_events(self):
        """处理游戏事件"""
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                self.running = False
            elif event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    self.running = False
    
    def run(self):
        """运行游戏循环"""
        while self.running:
            self.handle_events()
            self.update()
            self.render()
            self.clock.tick(self.config.refresh_rate)
        
        pygame.quit()