mod creature;
mod energy;
pub mod sim_thread;
mod spatial;
pub mod terrain;
mod trail;
mod world;

pub use creature::Creature;
pub use energy::{EnergyParticle, ParticleSource};
pub use sim_thread::SimSnapshot;
pub use spatial::SpatialGrid;
pub use terrain::{TerrainMap, TerrainParams, GRID_WORLD_SIZE};
pub use trail::TrailPoint;
pub use world::{AliveAgeStats, DeathAgeStats, DominantCandidate, World};
