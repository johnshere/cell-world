use rand::Rng;

/// 分区编号 i8（-31~31），编码：层级（绝对值）、左右（正负）、功能区（范围）
///
/// Input（固定交互点）  -7 ~ 7
///   0        自身状态（体感）
///   -1 / +1  左眼 / 右眼
///   -2~-7 / +2~+7  预留
///
/// Block（处理节点）  -31 ~ 31
///   与 Input 同号（|b| ≤ 7）: 感官处理区 SENSORY
///   8 ≤ |b| ≤ 24:            联合区 ASSOCIATION（进化产生）
///   25 ≤ |b| ≤ 31:           运动执行区 MOTOR
///
/// Output（固定交互点） ±25 ~ ±31
///   与 Block 运动区同号，只从对应 Block 接收
///
/// 前馈方向: |target| > |source|
/// 同侧偏好: 同号优先
/// 跨半球:   小概率异号（类似胼胝体）

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockZone {
    Sensory,
    Association,
    Motor,
}

/// 分区所属功能区
pub fn zone(block: i8) -> BlockZone {
    let abs = block.unsigned_abs();
    match abs {
        0..=7 => BlockZone::Sensory,
        8..=24 => BlockZone::Association,
        25..=31 => BlockZone::Motor,
        _ => BlockZone::Association,
    }
}

pub fn is_sensory(block: i8) -> bool {
    block.unsigned_abs() <= 7
}

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

/// 层级深度
pub fn abs_level(block: i8) -> u8 {
    block.unsigned_abs()
}

/// Input 节点 id → 所属感官区 block 编号
pub fn sensory_block_for_input(input_id: usize) -> i8 {
    match input_id {
        0..=7 => -1,   // 左眼
        8..=15 => 1,   // 右眼
        16 => 0,       // 自身状态
        _ => 0,
    }
}

/// Output 节点索引(0~6) → 所属运动区 block 编号
pub fn motor_block_for_output(_output_idx: usize) -> i8 {
    25 // 当前所有输出都是运动控制，未来可按 idx 分配 ±25~±31
}

/// 根据源 block 和 rate，随机选择目标 block
/// rate 控制小概率事件：跨区、跨半球
pub fn random_target_block(source: i8, rate: f64, rng: &mut impl Rng) -> i8 {
    let src_zone = zone(source);
    let src_side: i8 = if source >= 0 { 1 } else { -1 };

    // 决定目标侧：大概率同侧，rate 概率对侧
    let target_side = if source == 0 || rng.gen::<f64>() < rate {
        if rng.gen_bool(0.5) { 1 } else { -1 }
    } else {
        src_side
    };

    // 决定目标区域
    let r = rng.gen::<f64>();
    let target_zone = match src_zone {
        BlockZone::Sensory => {
            if r < rate {
                BlockZone::Motor
            } else if r < rate * 3.0 {
                BlockZone::Association
            } else {
                BlockZone::Sensory
            }
        }
        BlockZone::Association => {
            if r < rate {
                BlockZone::Sensory
            } else if r < rate * 2.0 {
                BlockZone::Motor
            } else {
                BlockZone::Association
            }
        }
        BlockZone::Motor => {
            if r < rate {
                BlockZone::Sensory
            } else if r < rate * 3.0 {
                BlockZone::Association
            } else {
                BlockZone::Motor
            }
        }
    };

    // 在目标区域内随机选 block
    let abs_block = match target_zone {
        BlockZone::Sensory => rng.gen_range(0..=7u8),
        BlockZone::Association => rng.gen_range(8..=24u8),
        BlockZone::Motor => rng.gen_range(25..=31u8),
    };

    abs_block as i8 * target_side
}

/// 随机生成联合区 block（进化新区用）
pub fn random_association_block(side: i8, rng: &mut impl Rng) -> i8 {
    let abs = rng.gen_range(8..=24u8) as i8;
    if side >= 0 { abs } else { -abs }
}
