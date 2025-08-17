package config

import (
	"encoding/json"
	"fmt"
	"image/color"
	"os"
	"strconv"
	"strings"

	"github.com/hajimehoshi/ebiten/v2"
)

const ConfigFilename = "config.json"

type JsonConfig struct {
	Width            int      `json:"width"`
	Height           int      `json:"height"`
	Title            string   `json:"title"`
	Unit             int      `json:"unit"`
	RefreshRate      int      `json:"refreshRate"`
	Grid             bool     `json:"grid"`
	GridColor        string   `json:"gridColor"`
	StrokeWidth      int      `json:"strokeWidth"`
	CreatureMaxLines int      `json:"creatureMaxLines"`
	CellMaxCount     int      `json:"cellMaxCount"`
	CellColors       []string `json:"cellColors"`
}

type Config struct {
	JsonConfig JsonConfig
	Width      int
	Height     int
	GridColor  color.RGBA
	CellColors []color.RGBA
}

// Global configuration variable
var GlobalConfig *Config

// InitConfig initializes the global configuration
func InitConfig() error {
	if GlobalConfig != nil {
		return nil // Already initialized
	}

	GlobalConfig = loadConfig()
	if GlobalConfig == nil {
		return fmt.Errorf("failed to load configuration")
	}

	return nil
}

// GetConfig returns the global configuration
func GetConfig() *Config {
	if GlobalConfig == nil {
		InitConfig()
	}
	return GlobalConfig
}

func translateColor(colorStr string) color.RGBA {
	if colorStr == "" {
		return color.RGBA{R: 0, G: 0, B: 0, A: 0} // Default gray color
	}
	vals := strings.Split(colorStr, ",")
	rgba := color.RGBA{}
	if len(vals[0]) > 0 {
		if val, err := strconv.Atoi(vals[0]); err == nil {
			rgba.R = uint8(val)
		}
	}
	if len(vals[1]) > 0 {
		if val, err := strconv.Atoi(vals[1]); err == nil {
			rgba.G = uint8(val)
		}
	}
	if len(vals[2]) > 0 {
		if val, err := strconv.Atoi(vals[2]); err == nil {
			rgba.B = uint8(val)
		}
	}
	if len(vals[3]) > 0 {
		if val, err := strconv.Atoi(vals[3]); err == nil {
			rgba.A = uint8(val)
		}
	}
	return rgba
}

// loadConfig loads the configuration from a JSON file
func loadConfig() *Config {
	jsonData, err := os.ReadFile(ConfigFilename)

	if err != nil {
		fmt.Println("Error loading config:", err)
		return nil
	}

	var jsonConfig JsonConfig
	err = json.Unmarshal(jsonData, &jsonConfig)

	if err != nil {
		fmt.Println("Error loading config:", err)
		return nil
	}

	cellColors := make([]color.RGBA, 0)
	for _, cellColor := range jsonConfig.CellColors {
		cellColors = append(cellColors, translateColor(cellColor))
	}

	config := &Config{
		JsonConfig: jsonConfig,
		GridColor:  translateColor(jsonConfig.GridColor),
		CellColors: cellColors,
	}

	// 获取屏幕分辨率宽高
	screenWidth, screenHeight := ebiten.ScreenSizeInFullscreen()
	// 窗口宽度和高度默认值
	width := 0
	height := 0
	// 窗口宽度和高度从配置文件中获取
	if config.JsonConfig.Width > 0 {
		width = config.JsonConfig.Width
	} else {
		width = screenWidth - 200
	}
	// 取整百
	width = (width / 100) * 100
	if config.JsonConfig.Height > 0 {
		height = config.JsonConfig.Height
	} else {
		height = screenHeight - 200
	}
	// 取整百
	height = (height / 100) * 100
	fmt.Println("Screen width:", screenWidth, "Screen height:", screenHeight)
	fmt.Println("Window width:", width, "Window height:", height)

	config.Width = width
	config.Height = height

	return config
}
