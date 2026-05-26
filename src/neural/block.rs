use rand::Rng;

/// 分区编号 i8（-31~31），编码：层级（绝对值）、左右（正负）、功能区（范围）
///
/// Input（固定交互点）每种感官占一个绝对值的镜像对，单侧投射，靠同源跨连让对侧使用：
///   -1       左眼（8 通道 FOV 扫描）
///   +1       右眼（8 通道 FOV 扫描）
///   -2       光语言（"光耳"，2 通道：方位 + 强度）
///   +2       预留作"对侧听觉皮层"（无 input 投射）
///   -3       自身能量（内省）
///   +3       地形感知（外感觉）
///
/// Block（处理节点）  -31 ~ 31
///   |b| ∈ {1, 2, 3}        感官处理区 SENSORY（既接收又处理，类比 V1/A1）
///   4 ≤ |b| ≤ 24            联合区 ASSOCIATION（进化产生）
///   25 ≤ |b| ≤ 31           运动执行区 MOTOR
///
/// Output（固定交互点）：
///   +25      运动控制（转向/速度/嘴/痕迹）
///   -25      繁殖控制（意愿/阈值/子代比例）
///   -26      光嘴/语言生成（跟光耳同侧成完整闭环）
///
/// block 0：永久空置——无 input/output 投射，random_association_block 不生成
///
/// 前馈方向: |target| > |source|
/// 同侧偏好: 同号优先
/// 跨半球:   严格同源 |from| == |to|（仿胼胝体镜像拓扑），跨级跨半球被禁止

pub fn is_association(block: i8) -> bool {
    let abs = block.unsigned_abs();
    (4..=24).contains(&abs)
}

pub fn is_motor(block: i8) -> bool {
    block.unsigned_abs() >= 25
}

/// 是否同侧（严格按符号判断；block 0 不再有中轴特权，按"非正"对待但已弃用）
pub fn is_same_side(a: i8, b: i8) -> bool {
    (a > 0) == (b > 0)
}

/// 是否同源镜像点（绝对值相同 + 异侧）
pub fn is_homotopic(a: i8, b: i8) -> bool {
    a != 0 && b != 0 && a.unsigned_abs() == b.unsigned_abs() && (a > 0) != (b > 0)
}

/// 前馈方向：|target| > |source|
pub fn is_forward(from: i8, to: i8) -> bool {
    to.unsigned_abs() > from.unsigned_abs()
}

/// Input 节点 id → 所属感官区 block 编号
pub fn sensory_block_for_input(input_id: usize) -> i8 {
    match input_id {
        0..=7 => -1,   // 左眼
        8..=15 => 1,   // 右眼
        16 => -3,      // 自身能量（内省）
        17 => 3,       // 地形感知（外感觉）
        18 | 19 => -2, // 光语言两通道（不拆分，靠同源跨连传到 +2）
        _ => -1,       // fallback（不再回退到 0）
    }
}

/// Output 节点索引(0~7) → 所属运动区 block 编号
pub fn motor_block_for_output(output_idx: usize) -> i8 {
    match output_idx {
        0 | 1 | 2 | 6 => 25, // 转向/速度/嘴/痕迹 → 运动
        3 | 4 | 5 => -25,    // 繁殖意愿/阈值/子代比例 → 繁殖
        7 => -26,            // 光嘴 → 跟光耳 -2 同侧，闭合语言通路
        _ => 25,
    }
}

/// 随机生成联合区 block（进化新区用）
/// 范围：|b| ∈ [1, 24]，含感官 abs（让 V1/A1 既接收 input 又能内部处理），不含 0
pub fn random_association_block(side: i8, rng: &mut impl Rng) -> i8 {
    let abs = rng.gen_range(1..=24u8) as i8;
    if side >= 0 {
        abs
    } else {
        -abs
    }
}
