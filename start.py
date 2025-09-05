#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
Cell World启动器
提供多种方式启动程序
"""

import os
import sys
import subprocess
import importlib.util


def check_dependencies():
    """检查必要的依赖是否已安装"""
    dependencies = ['pygame', 'pygame_gui']
    missing = []
    
    for dep in dependencies:
        if importlib.util.find_spec(dep) is None:
            missing.append(dep)
    
    if missing:
        print(f"缺少以下依赖: {', '.join(missing)}")
        install = input("是否自动安装这些依赖? (y/n): ").strip().lower()
        if install == 'y':
            subprocess.check_call([sys.executable, '-m', 'pip', 'install', *missing])
            print("依赖安装完成!")
            return True
        else:
            print(f"请手动安装依赖: pip install {' '.join(missing)}")
            return False
    return True


def run_standard():
    """运行标准版本"""
    if os.path.exists('main.py'):
        subprocess.call([sys.executable, 'main.py'])
    else:
        print("错误: 找不到main.py文件")


def run_optimized():
    """运行优化版本"""
    if os.path.exists('main_optimized.py'):
        subprocess.call([sys.executable, 'main_optimized.py'])
    else:
        print("错误: 找不到main_optimized.py文件")
        print("尝试运行标准版本...")
        run_standard()


def run_test():
    """运行测试脚本"""
    if os.path.exists('test.py'):
        subprocess.call([sys.executable, 'test.py'])
    else:
        print("错误: 找不到test.py文件")


def main():
    """主函数"""
    if not check_dependencies():
        return
    
    print("\n===== Cell World 启动器 =====")
    print("1. 运行标准版本 (main.py)")
    print("2. 运行优化版本 (main_optimized.py)")
    print("3. 运行测试脚本 (test.py)")
    print("0. 退出")
    
    choice = input("\n请选择 [1-3, 0]: ").strip()
    
    if choice == '1':
        run_standard()
    elif choice == '2':
        run_optimized()
    elif choice == '3':
        run_test()
    elif choice == '0':
        print("退出程序")
    else:
        print("无效选择，默认运行标准版本")
        run_standard()


if __name__ == "__main__":
    main()