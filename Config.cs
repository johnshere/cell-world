using Newtonsoft.Json;
using Newtonsoft.Json.Converters;
using System.Drawing;

namespace GridGameUI
{
    public class ColorConverter : JsonConverter<Color>
    {
        public override void WriteJson(JsonWriter writer, Color value, JsonSerializer serializer)
        {
            writer.WriteValue(value.Name);
        }

        public override Color ReadJson(JsonReader reader, Type objectType, Color existingValue, bool hasExistingValue, JsonSerializer serializer)
        {
            string? colorName = reader.Value?.ToString();
            if (string.IsNullOrEmpty(colorName))
                return Color.Black;
            
            return Color.FromName(colorName);
        }
    }

    public class GameConfig
    {
        public WindowConfig Window { get; set; } = new WindowConfig();
        public PanelConfig Panel { get; set; } = new PanelConfig();
        public GridConfig Grid { get; set; } = new GridConfig();
        public CoordinateConfig Coordinate { get; set; } = new CoordinateConfig();
    }

    public class WindowConfig
    {
        public int Width { get; set; } = 1200;
        public int Height { get; set; } = 800;
        public bool Fullscreen { get; set; } = false;
        public string Title { get; set; } = "Grid Game UI";
        public int Fps { get; set; } = 60;
    }

    public class PanelConfig
    {
        public int Width { get; set; } = 400;
        public bool Expanded { get; set; } = true;
        [JsonConverter(typeof(ColorConverter))]
        public Color BackgroundColor { get; set; } = Color.LightGray;
        [JsonConverter(typeof(ColorConverter))]
        public Color BorderColor { get; set; } = Color.Black;
        public int BorderWidth { get; set; } = 2;
    }

    public class GridConfig
    {
        public int CellSize { get; set; } = 10;
        [JsonConverter(typeof(ColorConverter))]
        public Color LineColor { get; set; } = Color.Gray;
        public int LineWidth { get; set; } = 1;
        [JsonConverter(typeof(ColorConverter))]
        public Color BackgroundColor { get; set; } = Color.White;
    }

    public class CoordinateConfig
    {
        [JsonConverter(typeof(ColorConverter))]
        public Color AxisColor { get; set; } = Color.Red;
        public int AxisWidth { get; set; } = 2;
        [JsonConverter(typeof(ColorConverter))]
        public Color LabelColor { get; set; } = Color.Black;
        public int LabelSize { get; set; } = 12;
        public int LabelOffset { get; set; } = 5;
        public int GridOffsetX { get; set; } = 0;
        public int GridOffsetY { get; set; } = 0;
    }

    public static class ConfigManager
    {
        private static readonly string ConfigPath = "config.json";
        private static GameConfig? _config;

        public static GameConfig Config
        {
            get
            {
                if (_config == null)
                {
                    LoadConfig();
                }
                return _config!;
            }
        }

        public static void LoadConfig()
        {
            try
            {
                if (File.Exists(ConfigPath))
                {
                    string json = File.ReadAllText(ConfigPath);
                    _config = JsonConvert.DeserializeObject<GameConfig>(json) ?? new GameConfig();
                }
                else
                {
                    _config = new GameConfig();
                    SaveConfig();
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"加载配置文件失败: {ex.Message}", "错误", MessageBoxButtons.OK, MessageBoxIcon.Error);
                _config = new GameConfig();
            }
        }

        public static void SaveConfig()
        {
            try
            {
                if (_config != null)
                {
                    string json = JsonConvert.SerializeObject(_config, Formatting.Indented);
                    File.WriteAllText(ConfigPath, json);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"保存配置文件失败: {ex.Message}", "错误", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
    }
}