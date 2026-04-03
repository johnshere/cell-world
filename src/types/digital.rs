pub struct U5(pub u8);

impl U5 {
    /// 直接构造：超出0-31会panic，适合确定取值合法的内部场景
    pub fn new(v: u8) -> Self {
        assert!(v <= 31, "U5 value must be between 0 and 31");
        Self(v)
    }

    /// 安全构造：返回Result，适合外部传参/不确定取值的场景，避免panic
    pub fn try_new(v: u8) -> Result<Self, &'static str> {
        if v <= 31 {
            Ok(Self(v))
        } else {
            Err("U5 value must be between 0 and 31")
        }
    }

    /// 获取内部原始值，方便业务使用
    pub fn get(&self) -> u8 {
        self.0
    }
}
