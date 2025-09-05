@echo off
chcp 65001 > nul

echo ===================================
echo 正在检查依赖项...
echo ===================================

pip list | findstr pygame > nul
if %errorlevel% neq 0 (
    echo pygame未安装，正在安装依赖项...
    pip install -r requirements.txt
    if %errorlevel% neq 0 (
        echo 依赖项安装失败，请手动运行: pip install -r requirements.txt
        pause
        exit /b 1
    )
    echo 依赖项安装完成！
) else (
    echo pygame已安装，继续启动...
)

echo ===================================
echo 正在启动Cell World优化版本...
echo ===================================
python main_optimized.py

if %errorlevel% neq 0 (
    echo 程序运行出错，请检查错误信息。
)

pause