//! 地形系统：生成后冻结的 fBm 噪声高度图
//!
//! 网格基本单元 50×50（与渲染网格共用 GRID_WORLD_SIZE 常量）。
//! 高度由两部分构成：
//!   1. 火山圆锥基底（中心高 → 向外平滑下降）
//!   2. 多倍频 value noise (fBm) 提供自然起伏细节
//! 地形通过种子参数化，相同 seed + 相同参数 → 相同地形。
//!
//! 设计原则：地形生成后**永久冻结**。后续 config.volcano_radius 调整不影响已生成区块；
//! 若需重新生成需通过 TerrainMap::generate 显式调用。

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// 渲染网格 / 地形 chunk 的世界坐标边长（像素）。
/// 渲染层、地形层及任何按格对齐的逻辑都应引用此常量。
pub const GRID_WORLD_SIZE: f64 = 50.0;

// =====================================================================
// 噪声工具函数（手写，无外部依赖）
// =====================================================================

/// 32 位整数哈希 → [-1, 1]
#[inline]
fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(seed);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// 二次平滑曲线（Hermite smoothstep）
#[inline]
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 双线性插值的 value noise，输入是浮点格点坐标
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let u = smoothstep(xf);
    let v = smoothstep(yf);
    let v00 = hash2(xi, yi, seed);
    let v10 = hash2(xi + 1, yi, seed);
    let v01 = hash2(xi, yi + 1, seed);
    let v11 = hash2(xi + 1, yi + 1, seed);
    let a = v00 + (v10 - v00) * u;
    let b = v01 + (v11 - v01) * u;
    a + (b - a) * v
}

/// fBm 多倍频叠加（lacunarity=2, persistence=0.5），归一化到 ≈ [-1, 1]
fn fbm(x: f32, y: f32, octaves: i32, seed: u32) -> f32 {
    let mut amp = 1.0_f32;
    let mut freq = 1.0_f32;
    let mut sum = 0.0_f32;
    let mut norm = 0.0_f32;
    for i in 0..octaves.max(1) {
        let s = seed.wrapping_add((i as u32).wrapping_mul(0x9E3779B1));
        sum += value_noise(x * freq, y * freq, s) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm.max(1e-6)
}

// =====================================================================
// 参数与地形函数
// =====================================================================

/// 地形生成参数（用户可在生成弹框中调整）
#[derive(Clone, Copy, Debug)]
pub struct TerrainParams {
    /// 火山口最高基底高度
    pub base_height: i32,
    /// 火山圆锥衰减距离（每该值距离下降 1）
    pub base_falloff: i32,
    /// fBm 噪声特征尺寸（像素）—— 越大山脉块越大
    pub fbm_scale: f64,
    /// fBm 噪声起伏幅度
    pub fbm_amp: f64,
    /// fBm 倍频层数（越多细节越丰富）
    pub fbm_octaves: i32,
    /// 随机种子
    pub seed: u32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            base_height: 60,
            base_falloff: 160,
            fbm_scale: 280.0,
            fbm_amp: 28.0,
            fbm_octaves: 4,
            seed: 1337,
        }
    }
}

/// 根据区块坐标 (cx, cy) 与生成参数计算该 chunk 的整数高度。
/// 纯函数 —— 给定相同输入产生确定结果。
pub fn chunk_terrain_height(chunk_x: i32, chunk_y: i32, params: &TerrainParams) -> i32 {
    let chunk_size = GRID_WORLD_SIZE as i32;
    let px = (chunk_x * chunk_size + chunk_size / 2) as f32;
    let py = (chunk_y * chunk_size + chunk_size / 2) as f32;
    let dist = (px * px + py * py).sqrt();

    // 1. 火山圆锥基底（用 f32 避免整数除法的阶梯）
    let base = (params.base_height as f32 - dist / params.base_falloff.max(1) as f32)
        .clamp(8.0, params.base_height as f32);

    // 2. fBm 噪声细节
    let nx = px / params.fbm_scale.max(1.0) as f32;
    let ny = py / params.fbm_scale.max(1.0) as f32;
    let n = fbm(nx, ny, params.fbm_octaves, params.seed);

    let height = base + n * params.fbm_amp as f32;
    height.round().max(8.0) as i32
}

// =====================================================================
// 地形数据结构
// =====================================================================

/// 已生成的地形数据
#[cfg_attr(not(feature = "persistence"), derive(Default, Clone))]
#[cfg_attr(feature = "persistence", derive(Clone))]
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
    pub generated_params: TerrainParamsPersist,
}

#[cfg(feature = "persistence")]
impl Serialize for TerrainMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("generated", &self.generated)?;
        map.serialize_entry("generated_radius", &self.generated_radius)?;
        map.serialize_entry("min_h", &self.min_h)?;
        map.serialize_entry("max_h", &self.max_h)?;
        map.serialize_entry("generated_params", &self.generated_params)?;
        // chunks: (i32,i32) key → "x,y" string
        let chunks: std::collections::HashMap<String, i32> = self
            .chunks
            .iter()
            .map(|(&(cx, cy), &h)| (format!("{},{}", cx, cy), h))
            .collect();
        map.serialize_entry("chunks", &chunks)?;
        map.end()
    }
}

#[cfg(feature = "persistence")]
impl<'de> Deserialize<'de> for TerrainMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            generated: bool,
            generated_radius: f64,
            min_h: i32,
            max_h: i32,
            #[serde(default)]
            generated_params: TerrainParamsPersist,
            chunks: std::collections::HashMap<String, i32>,
        }
        let h = Helper::deserialize(deserializer)?;
        let chunks: FxHashMap<(i32, i32), i32> = h
            .chunks
            .into_iter()
            .filter_map(|(k, v)| {
                let parts: Vec<&str> = k.split(',').collect();
                if parts.len() == 2 {
                    let cx = parts[0].parse().ok()?;
                    let cy = parts[1].parse().ok()?;
                    Some(((cx, cy), v))
                } else {
                    None
                }
            })
            .collect();
        Ok(Self {
            chunks,
            generated: h.generated,
            generated_radius: h.generated_radius,
            min_h: h.min_h,
            max_h: h.max_h,
            generated_params: h.generated_params,
        })
    }
}

impl Default for TerrainMap {
    fn default() -> Self {
        Self {
            chunks: FxHashMap::default(),
            generated: false,
            generated_radius: 0.0,
            min_h: 8,
            max_h: 8,
            generated_params: TerrainParamsPersist::default(),
        }
    }
}

/// 持久化用：为 TerrainParams 提供 Default + Serde
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug)]
pub struct TerrainParamsPersist {
    #[cfg_attr(feature = "persistence", serde(default = "default_base_height"))]
    pub base_height: i32,
    #[cfg_attr(feature = "persistence", serde(default = "default_base_falloff"))]
    pub base_falloff: i32,
    #[cfg_attr(feature = "persistence", serde(default = "default_fbm_scale"))]
    pub fbm_scale: f64,
    #[cfg_attr(feature = "persistence", serde(default = "default_fbm_amp"))]
    pub fbm_amp: f64,
    #[cfg_attr(feature = "persistence", serde(default = "default_fbm_octaves"))]
    pub fbm_octaves: i32,
    #[cfg_attr(feature = "persistence", serde(default = "default_seed"))]
    pub seed: u32,
}

fn default_base_height() -> i32 {
    60
}
fn default_base_falloff() -> i32 {
    160
}
fn default_fbm_scale() -> f64 {
    280.0
}
fn default_fbm_amp() -> f64 {
    28.0
}
fn default_fbm_octaves() -> i32 {
    4
}
fn default_seed() -> u32 {
    1337
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
            fbm_scale: p.fbm_scale,
            fbm_amp: p.fbm_amp,
            fbm_octaves: p.fbm_octaves,
            seed: p.seed,
        }
    }
}

impl From<TerrainParamsPersist> for TerrainParams {
    fn from(p: TerrainParamsPersist) -> Self {
        Self {
            base_height: p.base_height,
            base_falloff: p.base_falloff,
            fbm_scale: p.fbm_scale,
            fbm_amp: p.fbm_amp,
            fbm_octaves: p.fbm_octaves,
            seed: p.seed,
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

    /// 保存到独立文件 `terrain.json`，与 snapshot 解耦。
    /// ⛰ 生成地形时立即调用，确保下次启动可自动复用。
    #[cfg(feature = "persistence")]
    pub fn save_to_disk(&self) -> Result<(), String> {
        let json = serde_json::to_string(self).map_err(|e| format!("地形序列化失败: {}", e))?;
        std::fs::write(TERRAIN_PATH, json).map_err(|e| format!("地形写入失败: {}", e))?;
        Ok(())
    }

    /// 从 `terrain.json` 加载。文件不存在或解析失败返回 `None`。
    #[cfg(feature = "persistence")]
    pub fn load_from_disk() -> Option<Self> {
        let content = std::fs::read_to_string(TERRAIN_PATH).ok()?;
        serde_json::from_str(&content).ok()
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

/// 独立地形文件路径（与 snapshot.json 解耦，启动时无条件加载）
#[cfg(feature = "persistence")]
const TERRAIN_PATH: &str = "terrain.json";
