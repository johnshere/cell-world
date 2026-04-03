use crate::creature_design::genome::Genome;

pub enum NeuronLayerType {
    Input,
    Connection,
    Processing,
    Output,
    Feedback,
}

pub struct Neuron {
    pub id: usize,
    /// 代数
    pub generation: u32,
    pub genome: Genome,

    /// 神经元分区 类型（0-31）
    pub part_type: U5,
    /// 神经元层类型
    pub layer_type: NeuronLayerType,

    /// 是否分裂
    pub is_dividing: bool,
}

impl Neuron {
    pub fn new(
        id: usize,
        generation: u32,
        genome: Genome,
        part_type: U5,
        layer_type: NeuronLayerType,
    ) -> Self {
        Self {
            id,
            generation,
            genome,
            part_type,
            layer_type,
        }
    }
    /// 神经元行为
    pub fn activate(&self) {
        self.genome.express_neuron(self);
    }
}
