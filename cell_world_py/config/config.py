import json
import os

class Config:
    """配置管理类"""
    
    def __init__(self, config_path=None):
        """初始化配置
        
        Args:
            config_path: 配置文件路径，默认为None，使用默认路径
        """
        if config_path is None:
            # 获取当前文件所在目录
            current_dir = os.path.dirname(os.path.abspath(__file__))
            config_path = os.path.join(current_dir, 'config.json')
        
        self.config_path = config_path
        self.load()
    
    def load(self):
        """加载配置"""
        try:
            with open(self.config_path, 'r', encoding='utf-8') as f:
                config = json.load(f)
                
                # 设置配置属性
                self.title = config.get('title', 'Cell World')
                self.width = config.get('width', 800)
                self.height = config.get('height', 600)
                self.unit = config.get('unit', 10)
                self.grid = config.get('grid', True)
                self.grid_color = config.get('gridColor', '#333333')
                self.stroke_width = config.get('strokeWidth', 1)
                self.cell_colors = config.get('cellColors', ['#FF0000', '#00FF00', '#0000FF'])
                self.refresh_rate = config.get('refreshRate', 60)
                self.cell_max_count = config.get('cellMaxCount', 300)
                self.creature_max_lines = config.get('creatureMaxLines', 6)
                self.creature_aging_age = config.get('creatureAgingAge', 25)
                self.creature_aging_cells = config.get('creatureAgingCells', 100)
                
        except Exception as e:
            error_msg = f"加载配置文件失败: {e}"
            print(error_msg)
            raise RuntimeError(error_msg)