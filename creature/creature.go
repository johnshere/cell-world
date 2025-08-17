package creature

import (
	"cellworld/config"
	"image/color"
	"math/rand"
	"slices"
	"strconv"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/text"
	"github.com/hajimehoshi/ebiten/v2/vector"
	"golang.org/x/image/font/basicfont"
)

type Cell struct {
	Col   int
	Row   int
	Color color.RGBA
}

type Creature struct {
	Id     int
	X      int
	Y      int
	Color  color.RGBA
	Rows   int
	Cols   int
	Width  int
	Height int
	Cells  []*Cell
}

func (c *Creature) Update(ocean *[]*Creature) error {
	// 防御性编程：检查nil指针
	if c == nil {
		return nil
	}

	// conf := config.GetConfig()

	c.Hunt(ocean)

	c.Grow()

	c.divide(ocean)

	c.ToDeath(ocean)

	return nil
}

func (c *Creature) Draw(screen *ebiten.Image) {
	// 防御性编程：检查nil指针
	if c == nil || len(c.Cells) == 0 {
		return
	}

	conf := config.GetConfig()
	unit := float32(conf.JsonConfig.Unit)

	for _, cell := range c.Cells {
		x := float32(cell.Col)*unit + float32(c.X)
		y := float32(cell.Row)*unit + float32(c.Y)
		vector.DrawFilledRect(screen, x, y, unit, unit, cell.Color, false)
	}

	// 绘制creature 的边界
	clr := conf.GridColor
	vector.StrokeRect(screen, float32(c.X), float32(c.Y), float32(c.Width), float32(c.Height), 1, clr, false)
	// 绘制creature 的id
	face := basicfont.Face7x13
	text.Draw(screen, strconv.Itoa(c.Id), face, c.X, c.Y-10, color.White)
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

func (c *Creature) divide(ocean *[]*Creature) {
	// 防御性编程：检查nil指针
	if c == nil || ocean == nil {
		return
	}

	conf := config.GetConfig()

	// 当内部连续两行或两列全部为空时，分裂
	emptyCols := []int{}
	emptyRows := []int{}
	for col := range c.Cols {
		empty := true
		for _, cell := range c.Cells {
			if cell.Col == col {
				empty = false
				break
			}
		}
		if empty {
			emptyCols = append(emptyCols, col)
		}
	}
	for row := range c.Rows {
		empty := true
		for _, cell := range c.Cells {
			if cell.Row == row {
				empty = false
				break
			}
		}
		if empty {
			emptyRows = append(emptyRows, row)
		}
	}

	// 检查连续的空列
	continuousCols := []int{}
	if len(emptyCols) >= 2 {
		for i := 0; i < len(emptyCols)-1; i++ {
			if emptyCols[i+1] == emptyCols[i]+1 {
				// 找到连续的空列，可以在此处分裂
				continuousCols = append(continuousCols, emptyCols[i])
				break
			}
		}
	}

	// 检查连续的空行
	continuousRows := []int{}
	if len(emptyRows) >= 2 {
		for i := 0; i < len(emptyRows)-1; i++ {
			if emptyRows[i+1] == emptyRows[i]+1 {
				// 找到连续的空行，可以在此处分裂
				continuousRows = append(continuousRows, emptyRows[i])
				break
			}
		}
	}

	// 优先按列分裂
	if len(continuousCols) > 0 {
		splitCol := continuousCols[0]

		// 创建左半部分新creature
		leftCreature := New()
		leftCreature.X = c.X
		leftCreature.Y = c.Y
		leftCreature.Color = c.Color

		// 创建右半部分新creature
		rightCreature := New()
		rightCreature.X = c.X + (splitCol+1)*conf.JsonConfig.Unit
		rightCreature.Y = c.Y
		rightCreature.Color = c.Color

		// 分配细胞到两个新creature
		leftCells := make([]*Cell, 0)
		rightCells := make([]*Cell, 0)

		for _, cell := range c.Cells {
			if cell.Col <= splitCol {
				leftCells = append(leftCells, cell)
			} else {
				rightCells = append(rightCells, cell)
			}
		}

		leftCreature.Cells = leftCells
		rightCreature.Cells = rightCells

		// 修正新creature的属性
		leftCreature.fix()
		rightCreature.fix()

		// 从ocean中移除原creature，添加两个新creature
		index := slices.Index(*ocean, c)
		if index != -1 {
			*ocean = slices.Delete(*ocean, index, index+1)
		}
		*ocean = append(*ocean, leftCreature, rightCreature)

	}
	if len(continuousRows) > 0 {
		// 按行分裂
		splitRow := continuousRows[0]

		// 创建上半部分新creature
		topCreature := New()
		topCreature.X = c.X
		topCreature.Y = c.Y
		topCreature.Color = c.Color

		// 创建下半部分新creature
		bottomCreature := New()
		bottomCreature.X = c.X
		bottomCreature.Y = c.Y + (splitRow+1)*conf.JsonConfig.Unit
		bottomCreature.Color = c.Color

		// 分配细胞到两个新creature
		topCells := make([]*Cell, 0)
		bottomCells := make([]*Cell, 0)

		for _, cell := range c.Cells {
			if cell.Row <= splitRow {
				topCells = append(topCells, cell)
			} else {
				bottomCells = append(bottomCells, cell)
			}
		}

		topCreature.Cells = topCells
		bottomCreature.Cells = bottomCells

		// 修正新creature的属性
		topCreature.fix()
		bottomCreature.fix()

		// 从ocean中移除原creature，添加两个新creature
		index := slices.Index(*ocean, c)
		if index != -1 {
			*ocean = slices.Delete(*ocean, index, index+1)
		}
		*ocean = append(*ocean, topCreature, bottomCreature)
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
	// 防御性编程：检查nil指针
	if c == nil || target == nil || ocean == nil {
		return
	}

	conf := config.GetConfig()

	// 计算坐标偏移量（以格子为单位）
	offsetCol := (c.X - target.X) / conf.JsonConfig.Unit
	offsetRow := (c.Y - target.Y) / conf.JsonConfig.Unit

	// 创建新的cell对象，避免直接修改target的cell
	for _, cell := range target.Cells {
		newCell := &Cell{
			Col:   cell.Col + offsetCol,
			Row:   cell.Row + offsetRow,
			Color: c.Color,
		}
		c.Cells = append(c.Cells, newCell)
	}

	// 从ocean中移除被吃的target
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
	conf := config.GetConfig()

	minCol := conf.Width
	minRow := conf.Height
	maxCol := -conf.Width
	maxRow := -conf.Height
	for _, cell := range c.Cells {
		minCol = min(minCol, cell.Col)
		minRow = min(minRow, cell.Row)
		maxCol = max(maxCol, cell.Col)
		maxRow = max(maxRow, cell.Row)
	}
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

func New() *Creature {
	return &Creature{
		Id:    rand.Intn(1000000),
		Cells: make([]*Cell, 0),
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

		creature = New()
		creature.X = x
		creature.Y = y
		creature.Cols = cols
		creature.Rows = rows
		creature.Width = width
		creature.Height = height
		creature.Color = color

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
