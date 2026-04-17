#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

use rustc_hash::FxHashMap;

use crate::config::Config;
use crate::neural::{Genome, SpikingNetwork};
use crate::world::{
    Creature, DeathAgeStats, DominantCandidate, EnergyParticle, HotSpring, SpatialGrid, TrailPoint,
    World,
};

const SNAPSHOT_PATH: &str = "snapshot.json";

/// 世界快照（可序列化）
#[cfg_attr(feature = "persistence", derive(Serialize, Deserialize))]
pub struct WorldSnapshot {
    pub version: u32,
    pub world_time: f64,
    pub creatures: Vec<Creature>,
    pub energy_particles: Vec<EnergyParticle>,
    pub trail_points: Vec<TrailPoint>,
    pub next_creature_id: u64,
    pub next_energy_id: u64,
    pub volcano_timer: f64,
    /// 已废弃，保留兼容旧存档
    #[serde(default)]
    pub meteorite_timer: f64,
    pub action_counts: [usize; 4],
    pub death_age_stats: DeathAgeStats,
    pub death_ages: Vec<f64>,
    pub death_age_sum: f64,
    pub trail_disabled: bool,
    pub clan_genomes: FxHashMap<u64, Genome>,
    pub dominant_species: Vec<DominantCandidate>,
    /// 存档时的完整配置（恢复时使用，保证环境一致）
    pub config: Config,
    // === 温泉 ===
    #[serde(default)]
    pub hot_springs: Vec<HotSpring>,
    #[serde(default)]
    pub spring_spawn_timer: f64,
    #[serde(default)]
    pub next_spring_id: u64,
}

impl WorldSnapshot {
    /// 从 World 提取快照
    pub fn capture(world: &World, config: &Config) -> Self {
        Self {
            version: 1,
            world_time: world.time,
            creatures: world
                .creatures
                .iter()
                .filter(|c| c.alive)
                .cloned()
                .collect(),
            energy_particles: world
                .energy_particles
                .iter()
                .filter(|e| e.alive)
                .cloned()
                .collect(),
            trail_points: world
                .trail_points
                .iter()
                .filter(|t| t.alive)
                .cloned()
                .collect(),
            next_creature_id: world.next_creature_id(),
            next_energy_id: world.next_energy_id(),
            volcano_timer: world.volcano_timer(),
            meteorite_timer: 0.0,
            action_counts: world.action_counts,
            death_age_stats: world.death_age_stats.clone(),
            death_ages: world.death_ages().to_vec(),
            death_age_sum: world.death_age_sum(),
            trail_disabled: world.trail_disabled,
            clan_genomes: world.clan_genomes().clone(),
            dominant_species: world.dominant_species.clone(),
            config: config.clone(),
            hot_springs: world.hot_springs.clone(),
            spring_spawn_timer: world.spring_spawn_timer(),
            next_spring_id: world.next_spring_id(),
        }
    }

    /// 保存到文件
    #[cfg(feature = "persistence")]
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string(self).map_err(|e| format!("序列化失败: {}", e))?;
        std::fs::write(SNAPSHOT_PATH, json).map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }

    /// 从文件加载
    #[cfg(feature = "persistence")]
    pub fn load() -> Option<Self> {
        let content = std::fs::read_to_string(SNAPSHOT_PATH).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// 检测是否存在存档文件
    pub fn exists() -> bool {
        std::path::Path::new(SNAPSHOT_PATH).exists()
    }

    /// 重建完整 World
    pub fn into_world(mut self) -> (World, Config) {
        let config = self.config.clone();
        // 从 genome 重建每个生物的 brain
        for creature in &mut self.creatures {
            creature.genome.ensure_sorted_cache();
            creature.brain = SpikingNetwork::from_genome(&creature.genome);
        }
        // 重建 clan_genomes 中的排序缓存（serde skip 导致反序列化后为空）
        for genome in self.clan_genomes.values_mut() {
            genome.ensure_sorted_cache();
        }

        // 构建空间索引
        let cell_size = config.vision_range * 1.5;
        let mut creature_grid = SpatialGrid::new(cell_size);
        let mut energy_grid = SpatialGrid::new(cell_size);
        let mut trail_grid = SpatialGrid::new(cell_size);

        for (i, c) in self.creatures.iter().enumerate() {
            creature_grid.insert(i, c.x, c.y);
        }
        for (i, e) in self.energy_particles.iter().enumerate() {
            energy_grid.insert(i, e.x, e.y);
        }
        for (i, t) in self.trail_points.iter().enumerate() {
            trail_grid.insert(i, t.x, t.y);
        }

        let world = World::from_snapshot(
            self.creatures,
            self.energy_particles,
            self.trail_points,
            creature_grid,
            energy_grid,
            trail_grid,
            self.world_time,
            self.volcano_timer,
            self.hot_springs,
            self.spring_spawn_timer,
            self.next_spring_id,
            self.next_creature_id,
            self.next_energy_id,
            self.action_counts,
            self.death_age_stats,
            self.death_ages,
            self.death_age_sum,
            self.trail_disabled,
            self.clan_genomes,
            self.dominant_species,
            &config,
        );
        (world, config)
    }
}

// =====================================================================
// 目录格式存档（多存档支持）
// =====================================================================

const SNAPSHOTS_DIR: &str = "snapshots";

/// 存档元信息
#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveMeta {
    pub version: u32,
    pub world_time: f64,
    pub creature_count: usize,
    pub energy_count: usize,
    pub trail_count: usize,
    pub config_energy_denominator: f64,
    pub config_heat_floor: f64,
    pub config_heat_dissipation: f64,
    pub original_file_size: u64,
    pub converted_at: String,
    pub saved_at: String,
}

/// 存档摘要（用于列表显示）
#[derive(Clone)]
pub struct ArchiveSummary {
    pub name: String,
    pub meta: ArchiveMeta,
}

/// 基因组库条目（保留用于未来优化）
#[derive(Clone, Serialize, Deserialize)]
struct GenomeEntry {
    hash: String,
    genome: Genome,
}

/// 基因组库（保留用于未来优化）
#[derive(Clone, Serialize, Deserialize)]
struct GenomeLibrary {
    genomes: Vec<GenomeEntry>,
}

impl WorldSnapshot {
    /// 保存到目录格式（多存档）
    /// 格式：meta.json + data.json.gz（压缩的完整快照）
    #[cfg(feature = "persistence")]
    pub fn save_to_dir(&self, name: &str) -> Result<(), String> {
        let dir = format!("{}/{}", SNAPSHOTS_DIR, name);

        // 确保目录存在
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

        // 生成元信息
        let meta = ArchiveMeta {
            version: 2,
            world_time: self.world_time,
            creature_count: self.creatures.len(),
            energy_count: self.energy_particles.len(),
            trail_count: self.trail_points.len(),
            config_energy_denominator: self.config.energy_denominator,
            config_heat_floor: self.config.heat_floor,
            config_heat_dissipation: self.config.heat_dissipation_coefficient,
            original_file_size: 0,
            converted_at: String::new(),
            saved_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        // 写入 meta.json
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| format!("序列化meta失败: {}", e))?;
        std::fs::write(format!("{}/meta.json", dir), meta_json)
            .map_err(|e| format!("写入meta.json失败: {}", e))?;

        // 序列化并压缩完整快照
        let json = serde_json::to_string(self).map_err(|e| format!("序列化失败: {}", e))?;
        let encoded = compress_gzip(json.as_bytes());
        std::fs::write(format!("{}/data.json.gz", dir), &encoded)
            .map_err(|e| format!("写入data.json.gz失败: {}", e))?;

        Ok(())
    }

    /// 从目录加载
    #[cfg(feature = "persistence")]
    pub fn load_from_dir(name: &str) -> Option<Self> {
        let dir = format!("{}/{}", SNAPSHOTS_DIR, name);

        // 读取并解压数据
        let data = std::fs::read(format!("{}/data.json.gz", dir)).ok()?;
        let json = decompress_gzip(&data);

        // 反序列化
        serde_json::from_slice(&json).ok()
    }
}

/// 列出所有存档
#[cfg(feature = "persistence")]
pub fn list_archives() -> Vec<ArchiveSummary> {
    let mut archives = Vec::new();
    let snapshots_dir = std::path::Path::new(SNAPSHOTS_DIR);

    if !snapshots_dir.exists() {
        return archives;
    }

    if let Ok(entries) = std::fs::read_dir(snapshots_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("meta.json").exists() {
                if let Ok(meta_json) = std::fs::read_to_string(path.join("meta.json")) {
                    if let Ok(meta) = serde_json::from_str::<ArchiveMeta>(&meta_json) {
                        let name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        archives.push(ArchiveSummary { name, meta });
                    }
                }
            }
        }
    }

    // 按时间排序（最新的在前）
    archives.sort_by(|a, b| b.meta.saved_at.cmp(&a.meta.saved_at));
    archives
}

/// 删除存档
#[cfg(feature = "persistence")]
pub fn delete_archive(name: &str) -> Result<(), String> {
    let dir = format!("{}/{}", SNAPSHOTS_DIR, name);
    let path = std::path::Path::new(&dir);

    if !path.exists() {
        return Err("存档不存在".to_string());
    }

    std::fs::remove_dir_all(path).map_err(|e| format!("删除失败: {}", e))
}

/// 检测是否存在旧格式存档
pub fn has_legacy_snapshot() -> bool {
    std::path::Path::new(SNAPSHOT_PATH).exists()
}

// =====================================================================
// 工具函数
// =====================================================================

/// 计算基因组的简单哈希
fn genome_hash(genome: &Genome) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // 使用关键字段做哈希
    genome.nodes.len().hash(&mut hasher);
    genome.connections.len().hash(&mut hasher);
    if let Some(first_conn) = genome.connections.first() {
        first_conn.weight.to_bits().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Gzip压缩
fn compress_gzip(data: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, data).unwrap();
    encoder.finish().unwrap()
}

/// Gzip解压
fn decompress_gzip(data: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).unwrap();
    out
}
