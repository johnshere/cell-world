//! 转换脚本：将旧 snapshot.json 转换为新目录格式
//!
//! 用法: cargo run --bin convert_snapshot [原存档路径] [目标目录名]
//! 默认: cargo run --bin convert_snapshot snapshot.json snapshots/legacy_20260411

use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Write;

const SNAPSHOT_PATH: &str = "snapshot.json";

/// 存档元信息（与 snapshot.rs 保持一致）
#[derive(Serialize, Deserialize)]
struct ArchiveMeta {
    version: u32,
    world_time: f64,
    creature_count: usize,
    energy_count: usize,
    trail_count: usize,
    config_energy_denominator: f64,
    config_heat_floor: f64,
    config_heat_dissipation: f64,
    original_file_size: u64,
    converted_at: String,
    saved_at: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source_path = args.get(1).map(|s| s.as_str()).unwrap_or(SNAPSHOT_PATH);
    let target_name = args.get(2).map(|s| s.as_str()).unwrap_or("legacy_archive");

    println!("转换存档: {} -> snapshots/{}", source_path, target_name);

    // 读取旧snapshot
    let source_bytes = fs::read(source_path).expect("无法读取源文件");
    let original_size = source_bytes.len();
    println!("源文件大小: {} bytes", original_size);

    let json: Value = serde_json::from_slice(&source_bytes).expect("JSON解析失败");

    // 提取元信息
    let world_time = json.get("world_time")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let creature_count = json.get("creatures")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let energy_count = json.get("energy_particles")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let trail_count = json.get("trail_points")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    // 提取配置
    let config = json.get("config");
    let config_energy_denominator = config.and_then(|c| c.get("energy_denominator"))
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let config_heat_floor = config.and_then(|c| c.get("heat_floor"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.05);
    let config_heat_dissipation = config.and_then(|c| c.get("heat_dissipation_coefficient"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.024);

    // 创建目标目录
    let target_dir = format!("snapshots/{}", target_name);
    fs::create_dir_all(&target_dir).expect("无法创建目标目录");

    // 生成meta.json
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let meta = ArchiveMeta {
        version: 2,
        world_time,
        creature_count,
        energy_count,
        trail_count,
        config_energy_denominator,
        config_heat_floor,
        config_heat_dissipation,
        original_file_size: original_size as u64,
        converted_at: now.clone(),
        saved_at: now,
    };

    let meta_json = serde_json::to_string_pretty(&meta).unwrap();
    fs::write(format!("{}/meta.json", target_dir), meta_json).expect("写入meta.json失败");
    println!("已生成: {}/meta.json", target_dir);

    // 直接压缩整个JSON作为data.json.gz
    let encoded = compress_gzip(&source_bytes);
    fs::write(format!("{}/data.json.gz", target_dir), &encoded).expect("写入data.json.gz失败");
    println!("已生成: {}/data.json.gz (压缩后 {} bytes)", target_dir, encoded.len());

    println!("\n转换完成!");
    println!("目标目录: {}", target_dir);
    println!("原始大小: {} bytes", original_size);
    println!("压缩后: {} bytes ({:.1}%)", encoded.len(), 100.0 * encoded.len() as f64 / original_size as f64);
    println!("\n旧存档 snapshot.json 未删除，请手动处理。");
}

/// Gzip压缩
fn compress_gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}
