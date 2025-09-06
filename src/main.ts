import { app, BrowserWindow, Menu, MenuItemConstructorOptions } from 'electron';
import * as path from 'path';
import { fileURLToPath } from 'url';
import config from '../config.json' with { type: 'json' };

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

let mainWindow: BrowserWindow | null;

function createWindow(): void {
  // 创建浏览器窗口
  mainWindow = new BrowserWindow({
    width: config.window.width,
    height: config.window.height,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    },
    icon: path.join(__dirname, '../assets/icon.png'), // 可选图标
    show: false // 先不显示，等准备好后再显示
  });

  // 加载应用的 index.html
  mainWindow.loadFile(path.join(__dirname, '../index.html'));

  // 当窗口准备好时显示
  mainWindow.once('ready-to-show', () => {
    mainWindow?.show();
  });

  // 当窗口被关闭时
  mainWindow.on('closed', () => {
    mainWindow = null;
  });

  // 开发环境下打开开发者工具
  if (process.env.NODE_ENV === 'development') {
    mainWindow.webContents.openDevTools();
  }
}

// 当 Electron 完成初始化并准备创建浏览器窗口时调用此方法
app.whenReady().then(createWindow);

// 当所有窗口都被关闭时退出应用
app.on('window-all-closed', () => {
  // 在 macOS 上，应用和菜单栏通常会保持活跃状态
  // 直到用户使用 Cmd + Q 明确退出
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('activate', () => {
  // 在 macOS 上，当点击 dock 图标并且没有其他窗口打开时
  // 通常会重新创建一个窗口
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow();
  }
});

// 设置应用菜单（包含全屏选项）
const template: MenuItemConstructorOptions[] = [
  {
    label: '视图',
    submenu: [
      {
        label: '切换全屏',
        accelerator: 'F11',
        click: () => {
          if (mainWindow) {
            mainWindow.setFullScreen(!mainWindow.isFullScreen());
          }
        }
      },
      {
        label: '重新加载',
        accelerator: 'CmdOrCtrl+R',
        click: () => {
          if (mainWindow) {
            mainWindow.reload();
          }
        }
      },
      {
        label: '开发者工具',
        accelerator: 'F12',
        click: () => {
          if (mainWindow) {
            mainWindow.webContents.toggleDevTools();
          }
        }
      }
    ]
  }
];

const menu = Menu.buildFromTemplate(template);
Menu.setApplicationMenu(menu);