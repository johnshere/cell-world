use rand::Rng;

/// 分区编号 i8（-31~31），编码：层级（绝对值）、左右（正负）、功能区（范围）
///
/// Input（固定交互点）  -7 ~ 7
///   -2       发光感知（360°扫描）
///   -1       左眼（140° FOV 扫描）
///    0       自身状态（体感：能量 + 地形）
///   +1       右眼（140° FOV 扫描）
///   +2~+7    预留
///
/// Block（处理节点）  -31 ~ 31
///   与 Input 同号（|b| ≤ 7）: 感官处理区 SENSORY
///   8 ≤ |b| ≤ 24:            联合区 ASSOCIATION（进化产生）
///   25 ≤ |b| ≤ 31:           运动执行区 MOTOR
///
/// Output（固定交互点） ±25 ~ ±31
///   -25      繁殖控制（意愿/阈值/子代比例）
///   +25      运动控制（转向/速度/嘴/痕迹）
///   +26      发光控制
///
/// 前馈方向: |target| > |source|
/// 同侧偏好: 同号优先
/// 跨半球:   小概率异号（类似胼胝体）

pub fn is_association(block: i8) -> bool {
    let abs = block.unsigned_abs();
    (8..=24).contains(&abs)
}

pub fn is_motor(block: i8) -> bool {
    block.unsigned_abs() >= 25
}

/// 是否同侧（同号或有一方为0）
pub fn is_same_side(a: i8, b: i8) -> bool {
    a == 0 || b == 0 || (a > 0) == (b > 0)
}

/// 前馈方向：|target| > |source|
pub fn is_forward(from: i8, to: i8) -> bool {
    to.unsigned_abs() > from.unsigned_abs()
}

/// Input 节点 id → 所属感官区 block 编号
pub fn sensory_block_for_input(input_id: usize) -> i8 {
    match input_id {
        0..=7 => -1,    // 左眼
        8..=15 => 1,    // 右眼
        16 | 17 => 0,   // 自身状态 + 地形感知
        18 | 19 => -2,  // 发光感知
        _ => 0,
    }
}

/// Output 节点索引(0~7) → 所属运动区 block 编号
pub fn motor_block_for_output(output_idx: usize) -> i8 {
    match output_idx {
        0 | 1 | 2 | 6 => 25,  // 转向/速度/嘴/痕迹 → 运动
        3 | 4 | 5 => -25,     // 繁殖意愿/阈值/子代比例 → 繁殖
        7 => 26,               // 发光 → 发光控制
        _ => 25,
    }
}

/// 随机生成联合区 block（进化新区用）
pub fn random_association_block(side: i8, rng: &mut impl Rng) -> i8 {
    let abs = rng.gen_range(8..=24u8) as i8;
    if side >= 0 { abs } else { -abs }
}
