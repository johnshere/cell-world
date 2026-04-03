pub mod block;
pub mod bridge;
mod genome;
pub mod gpu;
mod network;
pub mod slot_alloc;
mod spiking;
pub mod thread;

pub use genome::{Genome, LayerType, NodeType};
pub use spiking::SpikingNetwork;
