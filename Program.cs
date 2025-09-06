using System;
using System.Windows.Forms;

namespace GridGameUI
{
    internal static class Program
    {
        /// <summary>
        /// 应用程序的主入口点。
        /// </summary>
        [STAThread]
        static void Main()
        {
            // 启用应用程序的视觉样式
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            
            try
            {
                // 加载配置
                ConfigManager.LoadConfig();
                
                // 创建并运行主窗口
                Application.Run(new MainForm());
            }
            catch (Exception ex)
            {
                string errorMessage = $"应用程序启动失败:\n\n错误信息: {ex.Message}\n\n堆栈跟踪:\n{ex.StackTrace}";
                MessageBox.Show(errorMessage, "错误", 
                    MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
    }
}