use rustc_hash::FxHashMap;

/// 空间索引网格，用于加速邻居查询
pub struct SpatialGrid {
    cell_size: f64,
    cells: FxHashMap<(i32, i32), Vec<usize>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
            cells: FxHashMap::default(),
        }
    }

    /// 清空网格
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// 获取坐标对应的网格单元
    fn cell_key(&self, x: f64, y: f64) -> (i32, i32) {
        (
            (x / self.cell_size).floor() as i32,
            (y / self.cell_size).floor() as i32,
        )
    }

    /// 插入实体
    pub fn insert(&mut self, index: usize, x: f64, y: f64) {
        let key = self.cell_key(x, y);
        self.cells.entry(key).or_default().push(index);
    }

    /// 查询范围内的实体索引
    pub fn query(&self, x: f64, y: f64, range: f64) -> Vec<usize> {
        let mut result = Vec::new();

        let cells_range = (range / self.cell_size).ceil() as i32 + 1;
        let center = self.cell_key(x, y);

        for dx in -cells_range..=cells_range {
            for dy in -cells_range..=cells_range {
                let key = (center.0 + dx, center.1 + dy);
                if let Some(indices) = self.cells.get(&key) {
                    result.extend(indices.iter().copied());
                }
            }
        }

        result
    }
}
