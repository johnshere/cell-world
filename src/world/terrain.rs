const CHUNK_SIZE: i32 = 100;

/// 根据区块坐标 (cx, cy) 获取该 100×100 区块的统一高度
fn chunk_terrain_height(chunk_x: i32, chunk_y: i32) -> i32 {
    // 火山世界中心点
    let center_x = 0;
    let center_y = 0;

    // 区块中心的世界坐标
    let px = chunk_x * CHUNK_SIZE + CHUNK_SIZE / 2;
    let py = chunk_y * CHUNK_SIZE + CHUNK_SIZE / 2;

    // 距离 & 角度
    let dx = px - center_x;
    let dy = py - center_y;
    let dist_sq = dx * dx + dy * dy;
    let dist = (dist_sq as f64).sqrt() as i32;
    let angle = dy.atan2(dx as f64);

    // ========== 1. 基础火山圆锥（整体圆形下降） ==========
    let base_height = 60 - (dist / 160);
    let base_height = base_height.clamp(8, 60);

    // ========== 2. 环形山脉（圆形结构） ==========
    let ring_period = 6; // 几圈山脉
    let ring_amp = 5; // 环形起伏强度
    let ring = ((dist / 110) % ring_period) * ring_amp;

    // ========== 3. 放射沟壑（向外辐射结构） ==========
    let radiate_count = 12; // 沟壑条数
    let radiate_amp = 7; // 沟壑深度
    let radiate = (angle * radiate_count as f64).sin() * radiate_amp as f64;

    // ========== 4. 微小扰动，让区块不呆板 ==========
    let noise = ((px * 17 + py * 23) % 5) as i32 - 2;

    // ========== 总高度（整数） ==========
    let mut height = base_height + ring + radiate.round() as i32 + noise;

    // 圆形世界边界
    if dist > 2200 {
        height = 8;
    }

    // 最低高度保护
    height.max(8)
}
