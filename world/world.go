package world

import (
	"cellworld/config"
	"fmt"
	"image/color"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/vector"
)

type Window struct {
	width  int
	height int
	title  string
}

type Game struct {
	window *Window
	config *config.Config
}

func Run() *Window {
	config := config.LoadConfig()
	if config == nil {
		return nil
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

	var window = &Window{
		width:  width,
		height: height,
		title:  config.JsonConfig.Title,
	}

	game := &Game{
		window: window,
		config: config,
	}

	ebiten.SetWindowSize(window.width, window.height)
	ebiten.SetWindowTitle(window.title)
	if err := ebiten.RunGame(game); err != nil {
		fmt.Println("Error running game:", err)
	}

	return window
}

func (w *Window) DrawGrid(dst *ebiten.Image, config *config.Config) {
	if config.JsonConfig.Grid {
		var unit = config.JsonConfig.Unit

		// 从配置中获取网格颜色
		clr := color.RGBA{
			R: config.GridColor[0],
			G: config.GridColor[1],
			B: config.GridColor[2],
			A: config.GridColor[3],
		}

		// 绘制网格
		for x := unit; x < w.width; x += unit {
			vector.StrokeLine(dst, float32(x), 0, float32(x), float32(w.height), 1, clr, false)
		}
		for y := unit; y < w.height; y += unit {
			vector.StrokeLine(dst, 0, float32(y), float32(w.width), float32(y), 1, clr, false)
		}
	} else {
		fmt.Println("Grid is disabled in config")
	}
}

func (g *Game) Update() error {
	return nil
}

func (g *Game) Draw(screen *ebiten.Image) {
	g.window.DrawGrid(screen, g.config)
}

func (g *Game) Layout(outsideWidth, outsideHeight int) (int, int) {
	return g.window.width, g.window.height
}
