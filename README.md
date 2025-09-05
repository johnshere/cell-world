# Grid Game UI - C# 版本

一个基于 Windows Forms 的简单网格游戏UI，支持网格绘制、坐标系显示、面板管理和鼠标交互。

## 功能特性

- ✅ **窗口管理**: 1200x800 分辨率，支持全屏切换 (F11)
- ✅ **右侧面板**: 400像素宽度，支持展开/收起
- ✅ **网格绘制**: 10像素网格，带坐标系显示
- ✅ **鼠标交互**: 支持拖拽移动坐标系，无限滚动
- ✅ **配置管理**: 独立的JSON配置文件
- ✅ **键盘快捷键**: ESC退出，F11全屏切换

## 系统要求

- Windows 10/11
- .NET 8.0 或更高版本
- Visual Studio 2022 或 Visual Studio Code

## 安装开发环境

### 方法1: 安装 .NET SDK
1. 访问 [.NET 下载页面](https://dotnet.microsoft.com/download)
2. 下载并安装 .NET 8.0 SDK
3. 重启命令行工具

### 方法2: 使用 Visual Studio
1. 下载 [Visual Studio 2022 Community](https://visualstudio.microsoft.com/vs/community/)
2. 安装时选择 ".NET 桌面开发" 工作负载

## 编译和运行

### 使用命令行 (.NET SDK)
```bash
# 编译项目
dotnet build

# 运行项目
dotnet run
```

### 使用 Visual Studio
1. 双击 `GridGameUI.csproj` 打开项目
2. 按 F5 或点击"开始调试"运行

### 使用 Visual Studio Code
1. 安装 C# 扩展
2. 打开项目文件夹
3. 按 F5 运行

## 项目结构

```
cell-world/
├── GridGameUI.csproj    # 项目文件
├── Program.cs           # 程序入口点
├── MainForm.cs          # 主窗口类
├── Config.cs            # 配置管理类
├── config.json          # 配置文件
└── README.md            # 说明文档
```

## 配置说明

配置文件 `config.json` 包含以下设置：

### 窗口配置 (Window)
- `Width/Height`: 窗口尺寸 (默认: 1200x800)
- `Fullscreen`: 是否全屏 (默认: false)
- `Title`: 窗口标题
- `Fps`: 刷新率 (默认: 60)

### 面板配置 (Panel)
- `Width`: 面板宽度 (默认: 400)
- `Expanded`: 是否展开 (默认: true)
- `BackgroundColor`: 背景色
- `BorderColor/BorderWidth`: 边框样式

### 网格配置 (Grid)
- `CellSize`: 网格大小 (默认: 10像素)
- `LineColor/LineWidth`: 网格线样式
- `BackgroundColor`: 背景色

### 坐标系配置 (Coordinate)
- `AxisColor/AxisWidth`: 坐标轴样式
- `LabelColor/LabelSize`: 标签样式
- `GridOffsetX/GridOffsetY`: 网格偏移量

## 操作说明

### 键盘快捷键
- **F11**: 切换全屏模式
- **ESC**: 退出程序 (窗口模式) 或退出全屏 (全屏模式)

### 鼠标操作
- **左键拖拽**: 移动网格和坐标系
- **点击 < > 按钮**: 展开/收起右侧面板

### 面板功能
- 右侧面板用于后续的数据展示功能
- 支持动态展开和收起
- 展开时宽度为400像素，收起时为30像素

## 开发说明

### 主要类说明

1. **Program.cs**: 应用程序入口点，负责初始化和启动
2. **MainForm.cs**: 主窗口类，包含所有UI逻辑和绘制功能
3. **Config.cs**: 配置管理，包含所有配置类和序列化逻辑

### 扩展建议

- 在右侧面板添加控件和数据展示
- 添加网格上的对象绘制功能
- 实现缩放功能
- 添加更多键盘快捷键
- 实现撤销/重做功能

## 故障排除

### 编译错误
1. 确保安装了 .NET 8.0 SDK
2. 检查项目文件路径是否正确
3. 尝试清理并重新编译: `dotnet clean && dotnet build`

### 运行时错误
1. 检查配置文件格式是否正确
2. 确保有足够的系统权限
3. 查看错误消息并检查相关配置

## 许可证

本项目仅供学习和验证想法使用。