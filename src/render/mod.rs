mod canvas;
mod panel;

pub use canvas::{RenderContext, VisibleWorldBounds, WorldCanvas};
pub use panel::{format_dhms, PanelAction, StatsPanel};

/// 选中状态
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    None,
    Creature(u64),
    Energy(u64),
}
