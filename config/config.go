package config

import (
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"strings"
)

const ConfigFilename = "config.json"

type JsonConfig struct {
	Width       int     `json:"width"`
	Height      int     `json:"height"`
	Title       string  `json:"title"`
	Grid        bool    `json:"grid"`
	Unit        int     `json:"unit"`
	GridColor   string  `json:"gridColor"`
	StrokeWidth float32 `json:"strokeWidth"`
}

type Config struct {
	JsonConfig JsonConfig
	GridColor  []uint8
}

// LoadConfig loads the configuration from a JSON file
func LoadConfig() *Config {
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

	var config = &Config{
		JsonConfig: jsonConfig,
		GridColor:  make([]uint8, 4),
	}
	if jsonConfig.GridColor != "" {
		var color = strings.Split(jsonConfig.GridColor, ",")
		if len(color) == 4 {
			if len(color[0]) > 0 {
				if val, err := strconv.Atoi(color[0]); err == nil {
					config.GridColor[0] = uint8(val)
				}
			}
			if len(color[1]) > 0 {
				if val, err := strconv.Atoi(color[1]); err == nil {
					config.GridColor[1] = uint8(val)
				}
			}
			if len(color[2]) > 0 {
				if val, err := strconv.Atoi(color[2]); err == nil {
					config.GridColor[2] = uint8(val)
				}
			}
			if len(color[3]) > 0 {
				if val, err := strconv.Atoi(color[3]); err == nil {
					config.GridColor[3] = uint8(val)
				}
			}
		}
	}

	return config
}
