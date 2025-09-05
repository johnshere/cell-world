import pygame
import sys
import os

def test_dependencies():
    """测试依赖项是否正确安装"""
    try:
        import pygame_gui
        print("✓ 依赖项检查通过")
        return True
    except ImportError as e:
        print(f"✗ 依赖项检查失败: {e}")
        print("请运行 'pip install -r requirements.txt' 安装所需依赖")
        return False

def test_config_file():
    """测试配置文件是否存在且可加载"""
    try:
        from config import WINDOW_CONFIG, PANEL_CONFIG, GRID_CONFIG, COORDINATE_CONFIG
        print("✓ 配置文件检查通过")
        return True
    except ImportError as e:
        print(f"✗ 配置文件检查失败: {e}")
        return False
    except Exception as e:
        print(f"✗ 配置文件格式错误: {e}")
        return False

def test_main_file():
    """测试主程序文件是否存在"""
    if os.path.exists("main.py"):
        print("✓ 主程序文件检查通过")
        return True
    else:
        print("✗ 主程序文件不存在")
        return False

def run_tests():
    """运行所有测试"""
    print("开始测试 Cell World 程序...\n")
    
    tests_passed = 0
    tests_total = 3
    
    if test_dependencies():
        tests_passed += 1
    print()
    
    if test_config_file():
        tests_passed += 1
    print()
    
    if test_main_file():
        tests_passed += 1
    print()
    
    print(f"测试完成: {tests_passed}/{tests_total} 通过")
    
    if tests_passed == tests_total:
        print("\n所有测试通过！可以运行 'python main.py' 启动程序")
    else:
        print("\n测试未全部通过，请修复上述问题后再运行程序")

if __name__ == "__main__":
    run_tests()