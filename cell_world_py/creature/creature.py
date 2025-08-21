import random
import numpy as np

class Creature:
    """生物类，由多个细胞组成"""
    
    def __init__(self, x, y, size=3, color=None, config=None):
        """初始化生物
        
        Args:
            x: 生物初始位置x坐标
            y: 生物初始位置y坐标
            size: 生物初始大小
            color: 生物颜色
            config: 配置对象
        """
        self.x = x
        self.y = y
        self.age = 0
        self.color = color
        self.config = config
        self.cells = np.zeros((size, size), dtype=bool)
        
        # 随机初始化细胞
        for i in range(size):
            for j in range(size):
                self.cells[i, j] = random.random() > 0.5
    
    def update(self):
        """更新生物状态"""
        # 应用生命游戏规则
        new_cells = np.copy(self.cells)
        rows, cols = self.cells.shape
        
        for i in range(rows):
            for j in range(cols):
                # 计算周围活细胞数量
                neighbors = 0
                for di in [-1, 0, 1]:
                    for dj in [-1, 0, 1]:
                        if di == 0 and dj == 0:
                            continue
                        ni, nj = i + di, j + dj
                        if 0 <= ni < rows and 0 <= nj < cols and self.cells[ni, nj]:
                            neighbors += 1
                
                # 应用规则
                if self.cells[i, j]:  # 活细胞
                    if neighbors < 2 or neighbors > 3:
                        new_cells[i, j] = False  # 死亡
                else:  # 死细胞
                    if neighbors == 3:
                        new_cells[i, j] = True  # 复活
        
        self.cells = new_cells
        self.age += 1
        
        # 老化处理
        aging_age = 100 if self.config is None else self.config.creature_aging_age
        if self.age > aging_age:
            # 随机失去细胞
            aging_probability = 0.1 if self.config is None else (1 / self.config.creature_aging_cells)
            if random.random() < aging_probability and rows > 0 and cols > 0:
                i, j = random.randint(0, rows-1), random.randint(0, cols-1)
                if self.cells[i, j]:
                    self.cells[i, j] = False
    
    def check_split(self):
        """检查是否可以分裂，返回分裂后的生物列表"""
        rows, cols = self.cells.shape
        
        # 检查是否有连续的空列
        for j in range(cols-1):
            if not np.any(self.cells[:, j]) and not np.any(self.cells[:, j+1]):
                # 分裂为两个生物
                creature1 = Creature(self.x, self.y, 1, self.color)
                creature2 = Creature(self.x + j + 1, self.y, 1, self.color)
                
                creature1.cells = np.copy(self.cells[:, :j])
                creature2.cells = np.copy(self.cells[:, j+2:])
                
                return [creature1, creature2]
        
        # 检查是否有连续的空行
        for i in range(rows-1):
            if not np.any(self.cells[i, :]) and not np.any(self.cells[i+1, :]):
                # 分裂为两个生物
                creature1 = Creature(self.x, self.y, 1, self.color)
                creature2 = Creature(self.x, self.y + i + 1, 1, self.color)
                
                creature1.cells = np.copy(self.cells[:i, :])
                creature2.cells = np.copy(self.cells[i+2:, :])
                
                return [creature1, creature2]
        
        return None
    
    def can_eat(self, other):
        """检查是否可以吃掉另一个生物
        
        Args:
            other: 另一个生物
            
        Returns:
            bool: 是否可以吃掉
        """
        # 检查是否重叠
        self_cells_count = np.sum(self.cells)
        other_cells_count = np.sum(other.cells)
        
        # 检查位置是否重叠
        self_rows, self_cols = self.cells.shape
        other_rows, other_cols = other.cells.shape
        
        # 简化的碰撞检测
        if (self.x <= other.x + other_cols and 
            self.x + self_cols >= other.x and 
            self.y <= other.y + other_rows and 
            self.y + self_rows >= other.y):
            # 重叠且自己更大
            return self_cells_count > other_cells_count
        
        return False
    
    def eat(self, other):
        """吃掉另一个生物
        
        Args:
            other: 被吃的生物
        """
        # 简单处理：获得对方的细胞数量
        self_cells_count = np.sum(self.cells)
        other_cells_count = np.sum(other.cells)
        
        # 扩展自己的细胞矩阵（如果需要）
        rows, cols = self.cells.shape
        new_size = max(rows, cols) + 1
        
        if new_size > rows or new_size > cols:
            new_cells = np.zeros((new_size, new_size), dtype=bool)
            new_cells[:rows, :cols] = self.cells
            self.cells = new_cells
        
        # 随机添加新细胞
        cells_to_add = int(other_cells_count * 0.5)  # 只获得一半的细胞
        rows, cols = self.cells.shape
        
        for _ in range(cells_to_add):
            i, j = random.randint(0, rows-1), random.randint(0, cols-1)
            if not self.cells[i, j]:
                self.cells[i, j] = True
    
    def get_cell_count(self):
        """获取细胞数量"""
        return np.sum(self.cells)