mod canvas;
mod panel;

pub use canvas::{WorldCanvas, VisibleWorldBounds, RenderContext};
pub use panel::{StatsPanel, PanelAction};

/// 选中状态
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    None,
    Creature(u64),
    Energy(u64),
}
