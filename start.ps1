# Cell World 启动脚本 (PowerShell版本)

# 设置控制台编码为UTF-8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "===================================" -ForegroundColor Cyan
Write-Host "Cell World 启动器 (PowerShell版本)" -ForegroundColor Cyan
Write-Host "===================================" -ForegroundColor Cyan

# 检查Python是否安装
try {
    $pythonVersion = python --version
    Write-Host "检测到Python: $pythonVersion" -ForegroundColor Green
} catch {
    Write-Host "错误: 未检测到Python，请安装Python 3.6或更高版本" -ForegroundColor Red
    Read-Host "按Enter键退出"
    exit 1
}

# 检查依赖项
function Check-Dependencies {
    $dependencies = @("pygame", "pygame_gui")
    $missingDeps = @()
    
    foreach ($dep in $dependencies) {
        $checkCmd = "python -c \"import $dep\" 2>$null"
        $result = Invoke-Expression $checkCmd
        if ($LASTEXITCODE -ne 0) {
            $missingDeps += $dep
        }
    }
    
    if ($missingDeps.Count -gt 0) {
        Write-Host "缺少以下依赖: $($missingDeps -join ', ')" -ForegroundColor Yellow
        $install = Read-Host "是否自动安装这些依赖? (y/n)"
        if ($install -eq "y") {
            Write-Host "正在安装依赖..." -ForegroundColor Cyan
            python -m pip install $missingDeps
            if ($LASTEXITCODE -eq 0) {
                Write-Host "依赖安装完成!" -ForegroundColor Green
                return $true
            } else {
                Write-Host "依赖安装失败，请手动安装: pip install $($missingDeps -join ' ')" -ForegroundColor Red
                return $false
            }
        } else {
            Write-Host "请手动安装依赖: pip install $($missingDeps -join ' ')" -ForegroundColor Yellow
            return $false
        }
    }
    return $true
}

# 运行标准版本
function Run-Standard {
    if (Test-Path "main.py") {
        Write-Host "\n启动标准版本..." -ForegroundColor Cyan
        python main.py
    } else {
        Write-Host "错误: 找不到main.py文件" -ForegroundColor Red
    }
}

# 运行优化版本
function Run-Optimized {
    if (Test-Path "main_optimized.py") {
        Write-Host "\n启动优化版本..." -ForegroundColor Cyan
        python main_optimized.py
    } else {
        Write-Host "错误: 找不到main_optimized.py文件" -ForegroundColor Red
        Write-Host "尝试运行标准版本..." -ForegroundColor Yellow
        Run-Standard
    }
}

# 运行测试脚本
function Run-Test {
    if (Test-Path "test.py") {
        Write-Host "\n运行测试脚本..." -ForegroundColor Cyan
        python test.py
    } else {
        Write-Host "错误: 找不到test.py文件" -ForegroundColor Red
    }
}

# 主函数
function Main {
    if (-not (Check-Dependencies)) {
        return
    }
    
    Write-Host "\n===== Cell World 启动器 =====" -ForegroundColor Green
    Write-Host "1. 运行标准版本 (main.py)" -ForegroundColor White
    Write-Host "2. 运行优化版本 (main_optimized.py)" -ForegroundColor White
    Write-Host "3. 运行测试脚本 (test.py)" -ForegroundColor White
    Write-Host "0. 退出" -ForegroundColor White
    
    $choice = Read-Host "\n请选择 [1-3, 0]"
    
    switch ($choice) {
        "1" { Run-Standard }
        "2" { Run-Optimized }
        "3" { Run-Test }
        "0" { Write-Host "退出程序" -ForegroundColor Cyan }
        default { 
            Write-Host "无效选择，默认运行标准版本" -ForegroundColor Yellow
            Run-Standard 
        }
    }
}

# 执行主函数
Main

Read-Host "按Enter键退出"