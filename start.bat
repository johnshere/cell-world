@echo off
chcp 65001 > nul

echo ===================================
echo Cell World 启动器
echo ===================================

python start.py

if %errorlevel% neq 0 (
    echo 程序运行出错，请检查错误信息。
)

pause