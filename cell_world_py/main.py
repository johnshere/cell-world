import sys
import platform
from config import Config
from world import World

def check_version():
    """检查Python版本"""
    required_version = (3, 13, 7)
    current_version = sys.version_info[:3]
    
    if current_version < required_version:
        print(f"警告: 当前Python版本 {platform.python_version()} 低于推荐版本 3.13.7")
        print("程序可能无法正常运行，建议升级Python版本")
        response = input("是否继续运行? (y/n): ")
        if response.lower() != 'y':
            sys.exit(0)

def main():
    """程序入口"""
    # 检查Python版本
    check_version()
    
    # 加载配置
    config = Config()
    
    # 创建游戏世界
    world = World(config)
    
    # 运行游戏
    world.run()

if __name__ == "__main__":
    main()