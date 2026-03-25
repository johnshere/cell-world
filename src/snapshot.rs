#[cfg(feature = "persistence")]
use serde::{Deserialize, Serialize};

use rustc_hash::FxHashMap;

use crate::config::Config;
use crate::neural::{Genome, SpikingNetwork};
use crate::world::{Creature, DeathAgeStats, DominantCandidate, EnergyParticle, SpatialGrid, TrailPoint, World};

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
}

impl WorldSnapshot {
    /// 从 World 提取快照
    pub fn capture(world: &World, config: &Config) -> Self {
        Self {
            version: 1,
            world_time: world.time,
            creatures: world.creatures.iter().filter(|c| c.alive).cloned().collect(),
            energy_particles: world.energy_particles.iter().filter(|e| e.alive).cloned().collect(),
            trail_points: world.trail_points.iter().filter(|t| t.alive).cloned().collect(),
            next_creature_id: world.next_creature_id(),
            next_energy_id: world.next_energy_id(),
            volcano_timer: world.volcano_timer(),
            meteorite_timer: world.meteorite_timer(),
            action_counts: world.action_counts,
            death_age_stats: world.death_age_stats.clone(),
            death_ages: world.death_ages().to_vec(),
            death_age_sum: world.death_age_sum(),
            trail_disabled: world.trail_disabled,
            clan_genomes: world.clan_genomes().clone(),
            dominant_species: world.dominant_species.clone(),
            config: config.clone(),
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

    /// 返回存档中的配置
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 重建完整 World
    pub fn into_world(mut self) -> (World, Config) {
        let config = self.config.clone();
        // 从 genome 重建每个生物的 brain
        for creature in &mut self.creatures {
            creature.genome.ensure_sorted_cache();
            creature.brain = SpikingNetwork::from_genome(&creature.genome);
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
            self.meteorite_timer,
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
