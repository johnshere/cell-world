using System.Drawing.Drawing2D;

namespace GridGameUI
{
    public partial class MainForm : Form
    {
        private GameConfig config;
        private bool panelExpanded;
        private int panelWidth;
        private int gridOffsetX;
        private int gridOffsetY;
        private bool isDragging;
        private Point lastMousePos;
        private Button toggleButton;
        private Panel rightPanel;
        private Timer renderTimer;

        public MainForm()
        {
            config = ConfigManager.Config;
            panelExpanded = config.Panel.Expanded;
            panelWidth = panelExpanded ? config.Panel.Width : 30;
            gridOffsetX = config.Coordinate.GridOffsetX;
            gridOffsetY = config.Coordinate.GridOffsetY;
            
            InitializeComponent();
            SetupWindow();
            SetupPanel();
            SetupTimer();
        }

        private void InitializeComponent()
        {
            this.SuspendLayout();
            
            // Form properties
            this.AutoScaleDimensions = new SizeF(7F, 15F);
            this.AutoScaleMode = AutoScaleMode.Font;
            this.ClientSize = new Size(config.Window.Width, config.Window.Height);
            this.Text = config.Window.Title;
            this.BackColor = config.Grid.BackgroundColor;
            this.DoubleBuffered = true;
            this.KeyPreview = true;
            
            // Events
            this.Paint += MainForm_Paint;
            this.MouseDown += MainForm_MouseDown;
            this.MouseMove += MainForm_MouseMove;
            this.MouseUp += MainForm_MouseUp;
            this.KeyDown += MainForm_KeyDown;
            this.Resize += MainForm_Resize;
            this.FormClosing += MainForm_FormClosing;
            
            this.ResumeLayout(false);
        }

        private void SetupWindow()
        {
            if (config.Window.Fullscreen)
            {
                this.WindowState = FormWindowState.Maximized;
                this.FormBorderStyle = FormBorderStyle.None;
            }
            else
            {
                this.WindowState = FormWindowState.Normal;
                this.FormBorderStyle = FormBorderStyle.Sizable;
            }
        }

        private void SetupPanel()
        {
            // Right panel
            rightPanel = new Panel
            {
                Width = panelWidth,
                Height = this.ClientSize.Height,
                Left = this.ClientSize.Width - panelWidth,
                Top = 0,
                BackColor = config.Panel.BackgroundColor,
                BorderStyle = BorderStyle.None
            };
            this.Controls.Add(rightPanel);

            // Toggle button
            toggleButton = new Button
            {
                Width = 20,
                Height = 20,
                Left = rightPanel.Left + (panelExpanded ? panelWidth - 25 : 5),
                Top = 10,
                Text = panelExpanded ? "<" : ">",
                BackColor = Color.LightBlue,
                FlatStyle = FlatStyle.Flat
            };
            toggleButton.Click += ToggleButton_Click;
            this.Controls.Add(toggleButton);
        }

        private void SetupTimer()
        {
            renderTimer = new Timer
            {
                Interval = 1000 / config.Window.Fps
            };
            renderTimer.Tick += (s, e) => this.Invalidate();
            renderTimer.Start();
        }

        private void MainForm_Paint(object sender, PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;

            // Calculate drawing area (excluding panel)
            int drawAreaWidth = this.ClientSize.Width - panelWidth;

            // Draw grid
            DrawGrid(g, drawAreaWidth);

            // Draw coordinate axes
            DrawCoordinateAxes(g, drawAreaWidth);

            // Draw coordinate labels
            DrawCoordinateLabels(g, drawAreaWidth);

            // Draw panel border
            DrawPanelBorder(g);
        }

        private void DrawGrid(Graphics g, int drawAreaWidth)
        {
            using (Pen gridPen = new Pen(config.Grid.LineColor, config.Grid.LineWidth))
            {
                // Vertical lines
                int startX = (gridOffsetX % config.Grid.CellSize) - config.Grid.CellSize;
                for (int x = startX; x < drawAreaWidth; x += config.Grid.CellSize)
                {
                    if (x >= 0)
                    {
                        g.DrawLine(gridPen, x, 0, x, this.ClientSize.Height);
                    }
                }

                // Horizontal lines
                int startY = (gridOffsetY % config.Grid.CellSize) - config.Grid.CellSize;
                for (int y = startY; y < this.ClientSize.Height; y += config.Grid.CellSize)
                {
                    if (y >= 0)
                    {
                        g.DrawLine(gridPen, 0, y, drawAreaWidth, y);
                    }
                }
            }
        }

        private void DrawCoordinateAxes(Graphics g, int drawAreaWidth)
        {
            using (Pen axisPen = new Pen(config.Coordinate.AxisColor, config.Coordinate.AxisWidth))
            {
                // Y axis (vertical)
                int originX = (drawAreaWidth / 2) + gridOffsetX;
                if (originX >= 0 && originX <= drawAreaWidth)
                {
                    g.DrawLine(axisPen, originX, 0, originX, this.ClientSize.Height);
                }

                // X axis (horizontal)
                int originY = (this.ClientSize.Height / 2) + gridOffsetY;
                if (originY >= 0 && originY <= this.ClientSize.Height)
                {
                    g.DrawLine(axisPen, 0, originY, drawAreaWidth, originY);
                }
            }
        }

        private void DrawCoordinateLabels(Graphics g, int drawAreaWidth)
        {
            using (Font font = new Font("Arial", config.Coordinate.LabelSize))
            using (Brush brush = new SolidBrush(config.Coordinate.LabelColor))
            {
                int originX = (drawAreaWidth / 2) + gridOffsetX;
                int originY = (this.ClientSize.Height / 2) + gridOffsetY;

                // X axis labels
                for (int i = -drawAreaWidth / 2 / config.Grid.CellSize; i <= drawAreaWidth / 2 / config.Grid.CellSize; i++)
                {
                    if (i == 0) continue;
                    if (i % 5 == 0) // Show label every 5 units
                    {
                        int xPos = originX + i * config.Grid.CellSize;
                        if (xPos >= 0 && xPos <= drawAreaWidth)
                        {
                            string label = i.ToString();
                            SizeF labelSize = g.MeasureString(label, font);
                            g.DrawString(label, font, brush, 
                                xPos - labelSize.Width / 2, 
                                originY + config.Coordinate.LabelOffset);
                        }
                    }
                }

                // Y axis labels
                for (int i = -this.ClientSize.Height / 2 / config.Grid.CellSize; i <= this.ClientSize.Height / 2 / config.Grid.CellSize; i++)
                {
                    if (i == 0) continue;
                    if (i % 5 == 0) // Show label every 5 units
                    {
                        int yPos = originY - i * config.Grid.CellSize;
                        if (yPos >= 0 && yPos <= this.ClientSize.Height)
                        {
                            string label = i.ToString();
                            SizeF labelSize = g.MeasureString(label, font);
                            g.DrawString(label, font, brush, 
                                originX + config.Coordinate.LabelOffset, 
                                yPos - labelSize.Height / 2);
                        }
                    }
                }
            }
        }

        private void DrawPanelBorder(Graphics g)
        {
            using (Pen borderPen = new Pen(config.Panel.BorderColor, config.Panel.BorderWidth))
            {
                int borderX = this.ClientSize.Width - panelWidth;
                g.DrawLine(borderPen, borderX, 0, borderX, this.ClientSize.Height);
            }
        }

        private void MainForm_MouseDown(object sender, MouseEventArgs e)
        {
            if (e.Button == MouseButtons.Left)
            {
                // Only start dragging if click is in the grid area (not on panel)
                if (e.X < this.ClientSize.Width - panelWidth)
                {
                    isDragging = true;
                    lastMousePos = e.Location;
                    this.Cursor = Cursors.Hand;
                }
            }
        }

        private void MainForm_MouseMove(object sender, MouseEventArgs e)
        {
            if (isDragging)
            {
                int dx = e.X - lastMousePos.X;
                int dy = e.Y - lastMousePos.Y;
                
                gridOffsetX += dx;
                gridOffsetY += dy;
                
                lastMousePos = e.Location;
                this.Invalidate();
            }
        }

        private void MainForm_MouseUp(object sender, MouseEventArgs e)
        {
            if (e.Button == MouseButtons.Left)
            {
                isDragging = false;
                this.Cursor = Cursors.Default;
            }
        }

        private void MainForm_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.KeyCode == Keys.Escape)
            {
                if (config.Window.Fullscreen)
                {
                    ToggleFullscreen();
                }
                else
                {
                    this.Close();
                }
            }
            else if (e.KeyCode == Keys.F11)
            {
                ToggleFullscreen();
            }
        }

        private void ToggleFullscreen()
        {
            config.Window.Fullscreen = !config.Window.Fullscreen;
            
            if (config.Window.Fullscreen)
            {
                this.WindowState = FormWindowState.Maximized;
                this.FormBorderStyle = FormBorderStyle.None;
            }
            else
            {
                this.WindowState = FormWindowState.Normal;
                this.FormBorderStyle = FormBorderStyle.Sizable;
                this.ClientSize = new Size(config.Window.Width, config.Window.Height);
            }
            
            UpdatePanelLayout();
        }

        private void ToggleButton_Click(object sender, EventArgs e)
        {
            panelExpanded = !panelExpanded;
            panelWidth = panelExpanded ? config.Panel.Width : 30;
            
            UpdatePanelLayout();
        }

        private void UpdatePanelLayout()
        {
            rightPanel.Width = panelWidth;
            rightPanel.Left = this.ClientSize.Width - panelWidth;
            rightPanel.Height = this.ClientSize.Height;
            
            toggleButton.Left = rightPanel.Left + (panelExpanded ? panelWidth - 25 : 5);
            toggleButton.Text = panelExpanded ? "<" : ">";
            
            this.Invalidate();
        }

        private void MainForm_Resize(object sender, EventArgs e)
        {
            if (!config.Window.Fullscreen)
            {
                config.Window.Width = this.ClientSize.Width;
                config.Window.Height = this.ClientSize.Height;
            }
            
            UpdatePanelLayout();
        }

        private void MainForm_FormClosing(object sender, FormClosingEventArgs e)
        {
            // Save current state to config
            config.Panel.Expanded = panelExpanded;
            config.Coordinate.GridOffsetX = gridOffsetX;
            config.Coordinate.GridOffsetY = gridOffsetY;
            
            ConfigManager.SaveConfig();
            
            renderTimer?.Stop();
            renderTimer?.Dispose();
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                renderTimer?.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}