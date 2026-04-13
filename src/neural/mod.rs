pub mod block;
pub mod bridge;
mod genome;
pub mod gpu;
pub mod slot_alloc;
mod spiking;
pub mod thread;

pub use genome::{PhysioGene, Genome};
pub use spiking::SpikingNetwork;
