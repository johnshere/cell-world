mod app;
mod config;
mod neural;
mod render;
mod snapshot;
mod store;
mod world;

use app::CellWorldApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_title("Cell World - Neural Emergence Simulator"),
        ..Default::default()
    };

    eframe::run_native(
        "Cell World",
        options,
        Box::new(|cc| Ok(Box::new(CellWorldApp::new(cc)))),
    )
}
