pub struct Bug {
    pub id: usize,
    /// 代数
    pub generation: u32,
    /// 寿命
    pub lifespan: u32,
    /// 能量
    pub energy: u32,

    pub genome: Genome,
}
