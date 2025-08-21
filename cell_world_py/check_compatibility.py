#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
Python 3.13.7 兼容性检查脚本

此脚本用于检查当前环境是否满足Cell World项目的运行要求，
特别是检查Python版本是否为3.13.7以及必要的依赖包是否已安装。
"""

import sys
import importlib.util
import platform

def check_python_version():
    """检查Python版本"""
    required_version = (3, 13, 7)
    current_version = sys.version_info[:3]
    
    print(f"检查Python版本...")
    print(f"当前Python版本: {platform.python_version()}")
    print(f"要求Python版本: 3.13.7")
    
    if current_version >= required_version:
        print("✓ Python版本满足要求")
        return True
    else:
        print("✗ Python版本不满足要求，请升级到Python 3.13.7")
        return False

def check_package(package_name, min_version=None):
    """检查包是否已安装"""
    spec = importlib.util.find_spec(package_name)
    
    if spec is None:
        print(f"✗ {package_name} 未安装")
        return False
    
    if min_version:
        try:
            package = importlib.import_module(package_name)
            version = getattr(package, '__version__', '0.0.0')
            print(f"  {package_name} 版本: {version}")
            
            # 简单版本比较，实际应用中可能需要更复杂的版本比较逻辑
            if version < min_version:
                print(f"✗ {package_name} 版本过低，需要 {min_version} 或更高版本")
                return False
        except (ImportError, AttributeError):
            print(f"✗ 无法获取 {package_name} 版本信息")
            return False
    
    print(f"✓ {package_name} 已安装")
    return True

def main():
    """主函数"""
    print("===== Cell World 兼容性检查 =====")
    
    # 检查Python版本
    python_ok = check_python_version()
    
    print("\n检查依赖包...")
    # 检查必要的依赖包
    pygame_ok = check_package('pygame', '2.5.2')
    numpy_ok = check_package('numpy', '1.24.0')
    
    print("\n===== 检查结果 =====")
    if python_ok and pygame_ok and numpy_ok:
        print("✓ 所有检查通过，环境满足运行要求")
        print("  可以通过运行 'python main.py' 启动Cell World")
    else:
        print("✗ 环境检查未通过，请解决上述问题后再运行Cell World")

if __name__ == "__main__":
    main()