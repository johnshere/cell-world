//! 地形系统：生成后冻结的高度图
//!
//! 网格基本单元 50×50（与渲染网格共用 GRID_WORLD_SIZE 常量）。
//! 每个区块整数高度，描述海底火山周围地形：
//!   - 中心高（火山口暖橙）→ 外围低（深蓝海底）
//!   - 叠加环形山脉、放射沟壑、微噪声
//!
//! 设计原则：地形生成后**永久冻结**。后续 config.volcano_radius 调整不影响已生成区块；
//! 若需重新生成需通过 TerrainMap::generate 显式调用。

#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

use rustc_hash::FxHashMap;

/// 渲染网格 / 地形 chunk 的世界坐标边长（像素）。
/// 渲染层、地形层及任何按格对齐的逻辑都应引用此常量。
pub const GRID_WORLD_SIZE: f64 = 50.0;

/// 地形生成参数（用户可在生成弹框中调整）
#[derive(Clone, Copy, Debug)]
pub struct TerrainParams {
    /// 火山口最高基底高度
    pub base_height: i32,
    /// 火山圆锥衰减距离（每该值距离下降 1）
    pub base_falloff: i32,
    /// 环形山脉波长（峰到峰世界坐标距离）
    pub ring_wavelength: f64,
    /// 环形山脉幅度
    pub ring_amp: f64,
    /// 放射沟壑条数
    pub radiate_count: i32,
    /// 放射沟壑深度
    pub radiate_amp: f64,
    /// 微噪声幅度（±此值）
    pub noise_amp: i32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            base_height: 60,
            base_falloff: 160,
            // 环数减少 1/3：原 220 → 330
            ring_wavelength: 330.0,
            ring_amp: 6.0,
            // 沟壑宽度减少 1/3 = 数量增加 1.5 倍：原 12 → 18
            radiate_count: 18,
            radiate_amp: 7.0,
            noise_amp: 2,
        }
    }
}

/// 根据区块坐标 (cx, cy) 与生成参数计算该 chunk 的整数高度。
/// 纯函数，与火山半径无关 —— 半径仅影响 `TerrainMap::generate` 的覆盖范围。
pub fn chunk_terrain_height(chunk_x: i32, chunk_y: i32, params: &TerrainParams) -> i32 {
    // 区块中心的世界坐标
    let chunk_size = GRID_WORLD_SIZE as i32;
    let px = chunk_x * chunk_size + chunk_size / 2;
    let py = chunk_y * chunk_size + chunk_size / 2;

    // 距离 & 角度
    let dx = px;
    let dy = py;
    let dist_sq = dx * dx + dy * dy;
    let dist = (dist_sq as f64).sqrt();
    let dist_i = dist as i32;
    let angle = (dy as f64).atan2(dx as f64);

    // ========== 1. 基础火山圆锥（整体圆形下降） ==========
    let base = params.base_height - (dist_i / params.base_falloff.max(1));
    let base = base.clamp(8, params.base_height);

    // ========== 2. 环形山脉（连续 sin 波，无突降） ==========
    let ring = ((dist / params.ring_wavelength.max(1.0)) * std::f64::consts::TAU).sin()
        * params.ring_amp;

    // ========== 3. 放射沟壑（向外辐射结构） ==========
    let radiate = (angle * params.radiate_count as f64).sin() * params.radiate_amp;

    // ========== 4. 微小扰动 ==========
    let noise_range = (params.noise_amp.max(0) * 2 + 1).max(1);
    let noise = (px * 17 + py * 23).rem_euclid(noise_range) - params.noise_amp.max(0);

    // ========== 总高度（整数） ==========
    let height = base + ring.round() as i32 + radiate.round() as i32 + noise;

    // 最低高度保护
    height.max(8)
}

/// 已生成的地形数据
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize, Default, Clone))]
#[cfg_attr(not(feature = "persistence"), derive(Default, Clone))]
pub struct TerrainMap {
    /// 已生成区块：(chunk_x, chunk_y) -> 高度
    pub chunks: FxHashMap<(i32, i32), i32>,
    /// 是否已生成
    pub generated: bool,
    /// 生成时锁定的火山半径（用于重建）
    pub generated_radius: f64,
    /// 高度归一化范围
    pub min_h: i32,
    pub max_h: i32,
    /// 生成时锁定的参数（用于重建）
    #[cfg_attr(feature = "persistence", serde(default))]
    pub generated_params: TerrainParamsPersist,
}

/// 持久化用：为 TerrainParams 提供 Default + Serde（避免 Copy 字段隐式问题）
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug)]
pub struct TerrainParamsPersist {
    pub base_height: i32,
    pub base_falloff: i32,
    pub ring_wavelength: f64,
    pub ring_amp: f64,
    pub radiate_count: i32,
    pub radiate_amp: f64,
    pub noise_amp: i32,
}

impl Default for TerrainParamsPersist {
    fn default() -> Self {
        TerrainParams::default().into()
    }
}

impl From<TerrainParams> for TerrainParamsPersist {
    fn from(p: TerrainParams) -> Self {
        Self {
            base_height: p.base_height,
            base_falloff: p.base_falloff,
            ring_wavelength: p.ring_wavelength,
            ring_amp: p.ring_amp,
            radiate_count: p.radiate_count,
            radiate_amp: p.radiate_amp,
            noise_amp: p.noise_amp,
        }
    }
}

impl From<TerrainParamsPersist> for TerrainParams {
    fn from(p: TerrainParamsPersist) -> Self {
        Self {
            base_height: p.base_height,
            base_falloff: p.base_falloff,
            ring_wavelength: p.ring_wavelength,
            ring_amp: p.ring_amp,
            radiate_count: p.radiate_count,
            radiate_amp: p.radiate_amp,
            noise_amp: p.noise_amp,
        }
    }
}

impl TerrainMap {
    /// 一次性生成全部 chunk。会清空已有数据。
    /// 覆盖范围：以原点为中心、`radius + 一格 chunk` 的圆。
    pub fn generate(&mut self, radius: f64, params: &TerrainParams) {
        self.chunks.clear();
        self.generated_radius = radius;
        self.generated_params = (*params).into();

        let extended = radius + GRID_WORLD_SIZE;
        let chunk_radius = (extended / GRID_WORLD_SIZE).ceil() as i32;
        let extended_sq = extended * extended;
        let mut min_h = i32::MAX;
        let mut max_h = i32::MIN;

        for cy in -chunk_radius..=chunk_radius {
            for cx in -chunk_radius..=chunk_radius {
                let px = cx as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
                let py = cy as f64 * GRID_WORLD_SIZE + GRID_WORLD_SIZE / 2.0;
                if px * px + py * py > extended_sq {
                    continue;
                }
                let h = chunk_terrain_height(cx, cy, params);
                if h < min_h {
                    min_h = h;
                }
                if h > max_h {
                    max_h = h;
                }
                self.chunks.insert((cx, cy), h);
            }
        }

        if self.chunks.is_empty() {
            self.min_h = 8;
            self.max_h = 8;
        } else {
            self.min_h = min_h;
            self.max_h = max_h;
        }
        self.generated = true;
    }

    /// 是否已生成
    pub fn is_generated(&self) -> bool {
        self.generated
    }

    /// 查询世界坐标 (x, y) 所在 chunk 的高度。未生成区域返回 None。
    pub fn height_at(&self, x: f64, y: f64) -> Option<i32> {
        if !self.generated {
            return None;
        }
        let cx = (x / GRID_WORLD_SIZE).floor() as i32;
        let cy = (y / GRID_WORLD_SIZE).floor() as i32;
        self.chunks.get(&(cx, cy)).copied()
    }
}
