use rustc_hash::FxHashMap;

/// 空间索引网格，用于加速邻居查询
///
/// 双层 API：
/// - **粗筛** `query_into` / `query`：返回 bounding box 内的实体索引（可能含圆外伪命中）。
///   仅用于"动态阈值"等无法用统一圆半径筛选的场景（如咬合用 `mouth_stroke + other_radius`）。
/// - **精筛** `query_circle_into` / `query_circle`：返回真正在 `range` 圆内的实体索引。
///   一般情况请用此族 API——避免每个调用方各自重复写 dx²+dy² 过滤。
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

    /// 把内部 HashMap buckets 缩到当前 load 附近（hashbrown 原生 shrink_to_fit）。
    /// 仅压缩容量，不改任何 (key → value) 映射，查询/插入行为字节级一致。
    /// 用于周期性回收"历史峰值 grow 后保留的 buckets"，避免长跑后 cache locality 退化。
    pub fn shrink_to_fit(&mut self) {
        self.cells.shrink_to_fit();
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

    /// 【粗筛】返回 bounding box 内的实体索引（一次性分配版）。
    ///
    /// 注意：返回值包含 `range` 圆**外**的伪命中（最远到方形外接 box 的角）。
    /// 仅在调用方有"动态阈值"等无法用统一圆半径筛选的场景使用，否则请用 [`Self::query_circle`]。
    pub fn query(&self, x: f64, y: f64, range: f64) -> Vec<usize> {
        let mut result = Vec::new();
        self.query_into(x, y, range, &mut result);
        result
    }

    /// 【粗筛】返回 bounding box 内的实体索引（复用缓冲区版）。
    ///
    /// 注意：返回值包含 `range` 圆**外**的伪命中。一般请用 [`Self::query_circle_into`]。
    pub fn query_into(&self, x: f64, y: f64, range: f64, result: &mut Vec<usize>) {
        result.clear();

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
    }

    /// 【精筛】返回真正在半径 `range` 圆内的实体索引（一次性分配版）。
    ///
    /// 调用方提供位置访问器 `get_pos(idx) -> (x, y)`。
    /// 内部一次性完成 cell 粗筛 + dx²+dy² ≤ range² 精筛，无需调用方重复写过滤。
    pub fn query_circle<F>(&self, x: f64, y: f64, range: f64, get_pos: F) -> Vec<usize>
    where
        F: Fn(usize) -> (f64, f64),
    {
        let mut result = Vec::new();
        self.query_circle_into(x, y, range, &mut result, get_pos);
        result
    }

    /// 【精筛】返回真正在半径 `range` 圆内的实体索引（复用缓冲区版）。
    pub fn query_circle_into<F>(
        &self,
        x: f64,
        y: f64,
        range: f64,
        result: &mut Vec<usize>,
        get_pos: F,
    ) where
        F: Fn(usize) -> (f64, f64),
    {
        result.clear();

        let cells_range = (range / self.cell_size).ceil() as i32 + 1;
        let center = self.cell_key(x, y);
        let r2 = range * range;

        for dx_cell in -cells_range..=cells_range {
            for dy_cell in -cells_range..=cells_range {
                let key = (center.0 + dx_cell, center.1 + dy_cell);
                if let Some(indices) = self.cells.get(&key) {
                    for &idx in indices {
                        let (px, py) = get_pos(idx);
                        let dx = px - x;
                        let dy = py - y;
                        if dx * dx + dy * dy <= r2 {
                            result.push(idx);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证粗筛会包含 box 角上的伪命中（这是已知行为，调用方需自知）
    #[test]
    fn query_into_includes_box_corners() {
        let mut g = SpatialGrid::new(15.0);
        g.insert(0, 0.0, 0.0); // 圆心
        g.insert(1, 12.0, 12.0); // box 内但圆外（dist≈16.97 > 10）
        g.insert(2, 5.0, 5.0); // 圆内（dist≈7.07 < 10）

        let mut buf = Vec::new();
        g.query_into(0.0, 0.0, 10.0, &mut buf);
        buf.sort();
        // 粗筛包含所有 cell 命中的实体，包括圆外的 (12,12)
        assert_eq!(buf, vec![0, 1, 2]);
    }

    /// 验证精筛只返回真正在圆内的实体
    #[test]
    fn query_circle_excludes_box_corners() {
        let mut g = SpatialGrid::new(15.0);
        g.insert(0, 0.0, 0.0);
        g.insert(1, 12.0, 12.0); // 圆外
        g.insert(2, 5.0, 5.0); // 圆内
        g.insert(3, 9.99, 0.0); // 圆边缘内
        g.insert(4, 10.01, 0.0); // 圆边缘外

        let positions = [
            (0.0, 0.0),
            (12.0, 12.0),
            (5.0, 5.0),
            (9.99, 0.0),
            (10.01, 0.0),
        ];
        let mut buf = Vec::new();
        g.query_circle_into(0.0, 0.0, 10.0, &mut buf, |i| positions[i]);
        buf.sort();
        assert_eq!(buf, vec![0, 2, 3]);
    }

    /// 验证精筛 range=0 时只返回圆心位置的实体
    #[test]
    fn query_circle_zero_range() {
        let mut g = SpatialGrid::new(15.0);
        g.insert(0, 5.0, 5.0);
        g.insert(1, 5.0001, 5.0); // 极近但非完全重合

        let positions = [(5.0, 5.0), (5.0001, 5.0)];
        let mut buf = Vec::new();
        g.query_circle_into(5.0, 5.0, 0.0, &mut buf, |i| positions[i]);
        assert_eq!(buf, vec![0]);
    }
}
