/// NEAT 创新号追踪器
pub struct Innovation {
    counter: usize,
}

impl Innovation {
    pub fn new() -> Self {
        Self { counter: 0 }
    }

    pub fn next(&mut self) -> usize {
        let id = self.counter;
        self.counter += 1;
        id
    }
}

impl Default for Innovation {
    fn default() -> Self {
        Self::new()
    }
}
