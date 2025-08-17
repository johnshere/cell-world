package creature

import (
	"cellworld/config"
	"image/color"
	"math/rand"
	"slices"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/vector"
)

type Cell struct {
	Col   int
	Row   int
	Color color.RGBA
}

type Creature struct {
	X         int
	Y         int
	Color     color.RGBA
	Rows      int
	Cols      int
	Width     int
	Height    int
	Direction int
	Cells     []*Cell
}

func (c *Creature) Update(ocean *[]*Creature) error {
	// 防御性编程：检查nil指针
	if c == nil {
		return nil
	}

	// conf := config.GetConfig()

	c.Hunt(ocean)

	c.Grow()

	c.ToDeath(ocean)

	return nil
}

func (c *Creature) Draw(screen *ebiten.Image) {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	conf := config.GetConfig()
	unit := float32(conf.JsonConfig.Unit)
	for _, cell := range c.Cells {
		x := float32(cell.Col)*unit + float32(c.X)
		y := float32(cell.Row)*unit + float32(c.Y)
		vector.DrawFilledRect(screen, x, y, unit, unit, cell.Color, false)
	}
}

func (c *Creature) ToDeath(ocean *[]*Creature) {
	// 防御性编程：检查nil指针
	if c == nil || ocean == nil {
		return
	}

	if len(c.Cells) > 0 {
		return
	}

	index := slices.Index(*ocean, c)
	if index != -1 {
		*ocean = slices.Delete(*ocean, index, index+1)
	}
}

func (c *Creature) Grow() {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	newCells := make([]*Cell, 0)

	// 元胞自动机 规则处理
	for col := -1; col < c.Cols+1; col++ {
		for row := -1; row < c.Rows+1; row++ {
			// 统计邻居细胞数量
			neighbors := 0
			var self *Cell
			for _, cell := range c.Cells {
				if cell.Col == col && cell.Row == row {
					self = cell
					continue // 同一个细胞
				}
				if col-1 <= cell.Col && cell.Col <= col+1 && row-1 <= cell.Row && cell.Row <= row+1 {
					neighbors++
				}
			}
			// 规则1：如果一个细胞周围有3个细胞，它就会变成一个细胞
			if neighbors == 3 {
				newCells = append(newCells, &Cell{
					Col:   col,
					Row:   row,
					Color: c.Color,
				})
			}
			// 规则2：如果一个细胞周围有2个细胞，它就会保持不变
			if neighbors == 2 && self != nil {
				newCells = append(newCells, self)
			}
			// 规则3：其他，即周围细胞数量小于2个或大于3个，它就会死亡
		}
	}

	c.Cells = newCells

	c.fix()
}

func (c *Creature) Eat(target *Creature, ocean *[]*Creature) {
	c.Cells = append(c.Cells, target.Cells...)

	i := slices.Index(*ocean, target)
	if i != -1 {
		*ocean = slices.Delete(*ocean, i, i+1)
	}
}

func (c *Creature) Hunt(ocean *[]*Creature) {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	overlays := c.isOverlay(ocean)
	if len(overlays) == 0 {
		return
	}

	size := len(c.Cells)
	for _, overlay := range overlays {
		if len(overlay.Cells) > size {
			return
		}
	}
	for _, overlay := range overlays {
		c.Eat(overlay, ocean)
	}
	c.fix()
}

func (c *Creature) isOverlay(ocean *[]*Creature) []*Creature {
	// 防御性编程：检查nil指针
	if c == nil {
		return make([]*Creature, 0)
	}

	conf := config.GetConfig()

	mx := c.X + c.Cols*conf.JsonConfig.Unit
	my := c.Y + c.Rows*conf.JsonConfig.Unit

	overlays := make([]*Creature, 0)

	for _, target := range *ocean {
		if c == target {
			continue
		}

		tmx := target.X + target.Cols*conf.JsonConfig.Unit
		tmy := target.Y + target.Rows*conf.JsonConfig.Unit
		if c.X < tmx && mx > target.X && c.Y < tmy && my > target.Y {
			overlays = append(overlays, target)
		}
	}
	return overlays
}

func (c *Creature) fix() {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	if len(c.Cells) == 0 {
		return
	}

	minCol := c.Cols + 1
	minRow := c.Rows + 1
	maxCol := -1
	maxRow := -1
	for _, cell := range c.Cells {
		minCol = min(minCol, cell.Col)
		minRow = min(minRow, cell.Row)
		maxCol = max(maxCol, cell.Col)
		maxRow = max(maxRow, cell.Row)
	}
	conf := config.GetConfig()
	c.X = minCol*conf.JsonConfig.Unit + c.X
	c.Y = minRow*conf.JsonConfig.Unit + c.Y
	c.Width = (maxCol - minCol + 1) * conf.JsonConfig.Unit
	c.Height = (maxRow - minRow + 1) * conf.JsonConfig.Unit
	c.Rows = maxRow - minRow + 1
	c.Cols = maxCol - minCol + 1
	for _, cell := range c.Cells {
		cell.Col -= minCol
		cell.Row -= minRow
	}
}

func Generate(ocean *[]*Creature) *Creature {

	conf := config.GetConfig()

	var creature *Creature
	for {
		rows := rand.Intn(conf.JsonConfig.CreatureMaxLines)
		cols := rand.Intn(conf.JsonConfig.CreatureMaxLines)
		width := cols * conf.JsonConfig.Unit
		height := rows * conf.JsonConfig.Unit
		x := rand.Intn(conf.Width - cols*conf.JsonConfig.Unit)
		y := rand.Intn(conf.Height - rows*conf.JsonConfig.Unit)
		color := conf.CellColors[rand.Intn(len(conf.CellColors))]

		creature = &Creature{
			X:      x,
			Y:      y,
			Cols:   cols,
			Rows:   rows,
			Width:  width,
			Height: height,
			Color:  color,
		}

		if len(creature.isOverlay(ocean)) == 0 {
			break
		}
	}
	creature.Cells = make([]*Cell, 0)
	for col := range creature.Cols {
		for row := range creature.Rows {
			isBool := rand.Intn(2) == 1
			if isBool {
				creature.Cells = append(creature.Cells, &Cell{
					Col:   col,
					Row:   row,
					Color: creature.Color,
				})
			}
		}
	}
	creature.fix()
	*ocean = append(*ocean, creature)

	return creature
}
