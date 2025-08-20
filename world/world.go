package world

import (
	"cellworld/config"
	"cellworld/creature"
	"fmt"
	"image/color"
	"sort"
	"strconv"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/text"
	"github.com/hajimehoshi/ebiten/v2/vector"
	"golang.org/x/image/font/basicfont"
)

type Game struct{}

var Ocean []*creature.Creature = make([]*creature.Creature, 0)

func Born() {
	conf := config.GetConfig()
	max := conf.JsonConfig.CellMaxCount
	count := 0
	for _, c := range Ocean {
		if c != nil {
			count += len(c.Cells)
		}
	}
	if count >= max {
		return
	} else if count > max/2 {
		creature.Generate(&Ocean)
	} else {
		creature.Generate(&Ocean)
		creature.Generate(&Ocean)
		creature.Generate(&Ocean)
	}
}

func Run() error {

	game := &Game{}
	conf := config.GetConfig()

	ebiten.SetWindowSize(conf.Width, conf.Height)
	ebiten.SetWindowTitle(conf.JsonConfig.Title)
	if err := ebiten.RunGame(game); err != nil {
		fmt.Println("Error running game:", err)
	}

	return nil
}

func DrawGrid(dst *ebiten.Image) {
	conf := config.GetConfig()
	if conf.JsonConfig.Grid {
		clr := conf.GridColor
		unit := int(conf.JsonConfig.Unit)

		// 绘制网格
		for x := unit; x < conf.Width; x += unit {
			vector.StrokeLine(dst, float32(x), 0, float32(x), float32(conf.Height), 1, clr, false)
		}
		for y := unit; y < conf.Height; y += unit {
			vector.StrokeLine(dst, 0, float32(y), float32(conf.Width), float32(y), 1, clr, false)
		}
	}
}

var frameCount = 0

func (g *Game) Update() error {
	frameCount++
	conf := config.GetConfig()
	if frameCount < conf.JsonConfig.RefreshRate {
		return nil
	}
	frameCount = 0

	Born()

	// 过滤掉nil的生物
	var validCreatures []*creature.Creature
	for _, c := range Ocean {
		if c != nil {
			validCreatures = append(validCreatures, c)
		}
	}

	// 只对有效的生物进行排序
	sort.Slice(validCreatures, func(i, j int) bool {
		return len(validCreatures[i].Cells) > len(validCreatures[j].Cells)
	})

	// 更新Ocean切片，只包含有效的生物
	Ocean = append(Ocean[:0], validCreatures...)

	// 更新所有生物
	for _, c := range Ocean {
		c.Update()
	}

	return nil
}

func showCount(screen *ebiten.Image) {
	conf := config.GetConfig()
	// 在右上角，绘制生物数量、细胞数量

	// 计算生物数量和细胞数量
	creatureCount := 0
	cellCount := 0
	for _, c := range Ocean {
		if c != nil {
			creatureCount++
			cellCount += len(c.Cells)
		}
	}

	// 设置文本样式
	face := basicfont.Face7x13
	textColor := color.RGBA{255, 255, 255, 255} // 白色文字

	// 绘制生物数量
	creatureText := "Creatures: " + strconv.Itoa(creatureCount)
	text.Draw(screen, creatureText, face, conf.Width-150, 30, textColor)

	// 绘制细胞数量
	cellText := "Cells: " + strconv.Itoa(cellCount)
	text.Draw(screen, cellText, face, conf.Width-150, 50, textColor)
}
func (g *Game) Draw(screen *ebiten.Image) {
	DrawGrid(screen)
	for _, creature := range Ocean {
		creature.Draw(screen)
	}

	// 绘制生物数量
	showCount(screen)
}

func (g *Game) Layout(outsideWidth, outsideHeight int) (int, int) {
	conf := config.GetConfig()
	return conf.Width, conf.Height
}
