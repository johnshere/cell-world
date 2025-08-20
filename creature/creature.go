package creature

import (
	"cellworld/config"
	"image/color"
	"math"
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
	Id         int
	X          int
	Y          int
	Color      color.RGBA
	Rows       int
	Cols       int
	Width      int
	Height     int
	Age        int
	AgingAge   int
	AgingCells int
	Cells      []*Cell
	Ocean      *[]*Creature
}

func (c *Creature) Update() {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	c.Hunt()

	c.Grow()

	c.Divide()

	c.Die()
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
	// 绘制creature cell 数量
	face := basicfont.Face7x13
	text.Draw(screen, "len:"+strconv.Itoa(len(c.Cells)), face, c.X, c.Y-8, color.White)
	// 绘制creature 年龄
	text.Draw(screen, "age:"+strconv.Itoa(c.Age), face, c.X, c.Y-20, color.White)
}

func (c *Creature) Die() {
	// 防御性编程：检查nil指针
	if c == nil || c.Ocean == nil {
		return
	}

	conf := config.GetConfig()

	// 判断是否越界
	isOut := c.X < 0 || c.Y < 0 || c.X+c.Width > conf.Width || c.Y+c.Height > conf.Height

	if len(c.Cells) == 0 || isOut {
		index := slices.Index(*c.Ocean, c)
		if index != -1 {
			*c.Ocean = slices.Delete(*c.Ocean, index, index+1)
		}
	}
}

func (c *Creature) Divide() {
	// 防御性编程：检查nil指针
	if c == nil || c.Ocean == nil {
		return
	}

	conf := config.GetConfig()

	// 当内部连续两行或两列全部为空时，分裂

	existingCols := []int{}
	for _, cell := range c.Cells {
		existingCols = append(existingCols, cell.Col)
	}

	existingRows := []int{}
	for _, cell := range c.Cells {
		existingRows = append(existingRows, cell.Row)
	}

	emptyCol := -1
	emptyRow := -1
	for i := range c.Cols - 1 {
		if !slices.Contains(existingCols, i) && !slices.Contains(existingCols, i+1) {
			emptyCol = i
			break
		}
	}
	for i := range c.Rows - 1 {
		if !slices.Contains(existingRows, i) && !slices.Contains(existingRows, i+1) {
			emptyRow = i
			break
		}
	}

	// 优先按列分裂
	if emptyCol > 0 {
		// 创建左半部分新creature
		leftCreature := New(c.Ocean)

		leftCreature.X = c.X
		leftCreature.Y = c.Y
		leftCreature.Color = c.Color

		// 创建右半部分新creature
		rightCreature := New(c.Ocean)
		rightCreature.X = c.X + (emptyCol+1)*conf.JsonConfig.Unit
		rightCreature.Y = c.Y
		rightCreature.Color = c.Color

		// 分配细胞到两个新creature
		leftCells := make([]*Cell, 0)
		rightCells := make([]*Cell, 0)

		for _, cell := range c.Cells {
			if cell.Col <= emptyCol {
				leftCells = append(leftCells, cell)
			} else {
				rightCells = append(rightCells, cell)
			}
		}

		if len(leftCells) > 0 {
			leftCreature.Cells = leftCells
			leftCreature.fix()
			*c.Ocean = append(*c.Ocean, leftCreature)
		}
		if len(rightCells) > 0 {
			rightCreature.Cells = rightCells
			rightCreature.fix()
			*c.Ocean = append(*c.Ocean, rightCreature)
		}

		// 从ocean中移除原creature，添加两个新creature
		index := slices.Index(*c.Ocean, c)
		if index != -1 {
			*c.Ocean = slices.Delete(*c.Ocean, index, index+1)
		}
	}
	if emptyRow > 0 {
		// 创建上半部分新creature
		topCreature := New(c.Ocean)

		topCreature.X = c.X
		topCreature.Y = c.Y
		topCreature.Color = c.Color

		// 创建下半部分新creature
		bottomCreature := New(c.Ocean)

		bottomCreature.X = c.X
		bottomCreature.Y = c.Y + (emptyRow+1)*conf.JsonConfig.Unit
		bottomCreature.Color = c.Color

		// 分配细胞到两个新creature
		topCells := make([]*Cell, 0)
		bottomCells := make([]*Cell, 0)

		for _, cell := range c.Cells {
			if cell.Row <= emptyRow {
				topCells = append(topCells, cell)
			} else {
				bottomCells = append(bottomCells, cell)
			}
		}
		if len(topCells) > 0 {
			topCreature.Cells = topCells
			topCreature.fix()
			*c.Ocean = append(*c.Ocean, topCreature)
		}
		if len(bottomCells) > 0 {
			bottomCreature.Cells = bottomCells
			bottomCreature.fix()
			*c.Ocean = append(*c.Ocean, bottomCreature)
		}

		// 从ocean中移除原creature，添加两个新creature
		index := slices.Index(*c.Ocean, c)
		if index != -1 {
			*c.Ocean = slices.Delete(*c.Ocean, index, index+1)
		}
	}
}

func (c *Creature) Grow() {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	conf := config.GetConfig()

	neighbors2Born := 3
	neighbors2Stay := 2
	// if c.AgingAge < c.Age {
	// 	neighbors2Born = 4
	// 	neighbors2Stay = 3
	// }

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
			if neighbors == neighbors2Born {
				newCells = append(newCells, &Cell{
					Col:   col,
					Row:   row,
					Color: c.Color,
				})
			}
			// 规则2：如果一个细胞周围有2个细胞，它就会保持不变
			if neighbors == neighbors2Stay && self != nil {
				newCells = append(newCells, self)
			}
			// 规则3：其他，即周围细胞数量小于2个或大于3个，它就会死亡
		}
	}

	size := len(newCells)
	if size > conf.JsonConfig.CellMaxCount {
		c.Cells = []*Cell{}
		c.Die()
		return
	}
	if size > 0 && (c.Age > c.AgingAge || c.AgingCells < len(newCells)) {
		// 衰老时，随机移除十分之一的细胞，至少移除一个
		removeSize := int(math.Ceil(float64(size) / 10))
		if removeSize < 1 {
			removeSize = 1
		}
		for i := 0; i < removeSize; i++ {
			index := rand.Intn(size)
			newCells = slices.Delete(newCells, index, index+1)
			size--
		}
	}

	c.Cells = newCells

	// 年龄增加
	c.Age++

	c.fix()
}

func (c *Creature) Eat(target *Creature) {
	// 防御性编程：检查nil指针
	if c == nil || target == nil || c.Ocean == nil {
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
	i := slices.Index(*c.Ocean, target)
	if i != -1 {
		*c.Ocean = slices.Delete(*c.Ocean, i, i+1)
	}
}

func (c *Creature) Hunt() {
	// 防御性编程：检查nil指针
	if c == nil {
		return
	}

	overlays := c.isOverlay()
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
		c.Eat(overlay)
	}
	c.fix()
}

func (c *Creature) isOverlay() []*Creature {
	// 防御性编程：检查nil指针
	if c == nil {
		return make([]*Creature, 0)
	}

	conf := config.GetConfig()

	mx := c.X + c.Cols*conf.JsonConfig.Unit
	my := c.Y + c.Rows*conf.JsonConfig.Unit

	overlays := make([]*Creature, 0)

	for _, target := range *c.Ocean {
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

func New(ocean *[]*Creature) *Creature {
	conf := config.GetConfig()
	return &Creature{
		Id:         rand.Intn(1000000),
		Cells:      make([]*Cell, 0),
		Age:        0,
		AgingAge:   conf.JsonConfig.CreatureAgingAge,
		AgingCells: conf.JsonConfig.CreatureAgingCells,
		Ocean:      ocean,
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

		creature = New(ocean)
		creature.X = x
		creature.Y = y
		creature.Cols = cols
		creature.Rows = rows
		creature.Width = width
		creature.Height = height
		creature.Color = color

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
		if len(creature.Cells) == 0 {
			continue
		}

		if len(creature.isOverlay()) == 0 {
			break
		}
	}
	creature.fix()
	*ocean = append(*ocean, creature)

	return creature
}
