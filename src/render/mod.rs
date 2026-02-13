mod canvas;
mod panel;

pub use canvas::{WorldCanvas, VisibleWorldBounds};
pub use panel::{StatsPanel, CachedStats};

/// 选中状态
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    None,
    Creature(u64),
    Energy(u64),
}
