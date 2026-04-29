#![recursion_limit = "256"]

mod app;
mod config;
mod mcp;
mod neural;
mod render;
mod snapshot;
mod store;
mod world;

use app::CellWorldApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let mcp_port = parse_mcp_args();

    if let Some(port) = mcp_port {
        // Headless MCP 模式（无 GUI）
        mcp::start_mcp_mode(port);
        Ok(())
    } else {
        // 正常 GUI 模式
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
}

/// 解析 --mcp 和 --mcp-port 参数
/// 返回 Some(port) 表示启用 MCP 模式，None 表示正常 GUI 模式
fn parse_mcp_args() -> Option<u16> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    let mut mcp = false;
    let mut port: u16 = 9877;

    while i < args.len() {
        match args[i].as_str() {
            "--mcp" => mcp = true,
            "--mcp-port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(9877);
                }
            }
            _ => {}
        }
        i += 1;
    }

    if mcp { Some(port) } else { None }
}
