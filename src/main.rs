mod app;
mod config;
mod neural;
mod render;
mod store;
mod world;

use app::CellWorldApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1425.0, 900.0])
            .with_title("Cell World - Neural Emergence Simulator"),
        ..Default::default()
    };

    eframe::run_native(
        "Cell World",
        options,
        Box::new(|cc| Ok(Box::new(CellWorldApp::new(cc)))),
    )
}
