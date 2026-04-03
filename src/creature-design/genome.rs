use super::bug::Bug;
use super::neuron::Neuron;

pub enum GeneType {
    Creature,
    Neuron,
}

pub struct Gene {
    pub id: usize,
    /// 基因类型 生命基因、神经元基因
    pub gene_type: GeneType,
}

pub struct Genome {
    /// 神经元表达权重数据
    pub neuron_expression_weights: Vec<f64>,
    /// 生物表达权重数据
    pub creature_expression_weights: Vec<f64>,
    /// 神经元基因，控制神经元行为
    pub genes: Vec<Gene>,
}

impl Gene {}

/// 基因组，容纳基因、控制基因变异和表达
impl Genome {
    /// 神经元基因表达
    pub fn express_neuron(neuron: &Neuron) {
        // 根据神经元当前所有属性，根据算法触发基因行为
        // 1 获取神经元所有属性数组
        let properties = vec![
            neuron.generation as f64,
            neuron.part_type.0 as f64,
            match neuron.layer_type {
                NeuronLayerType::Input => 0.0,
                NeuronLayerType::Connection => 1.0,
                NeuronLayerType::Processing => 2.0,
                NeuronLayerType::Output => 3.0,
                NeuronLayerType::Feedback => 4.0,
            },
            neuron.is_dividing as u8 as f64,
        ];
        // 2 根据权重计算每个属性的激活值（归一化）
        let activation_value = properties
            .iter()
            .zip(neuron.genome.neuron_expression_weights.iter())
            .map(|(p, w)| p * w)
            .sum::<f64>()
            / neuron.genome.neuron_expression_weights.len() as f64;
        // 3 根据激活值触发基因
        let neuron_genes = neuron
            .genome
            .genes
            .iter()
            .filter(|g| matches!(g.gene_type, GeneType::Neuron));
        let neuron_gene_count = neuron_genes.clone().count() as f64;
        // 表达一个神经元基因 规则固定的，相同的参数一定表达同一个基因，保证稳定性
        let activation_gene = neuron_genes
            .nth((activation_value * neuron_gene_count) as usize % neuron_gene_count as usize);
        // 4 根据基因记录的权重值计算调整的属性；规则固定，参数相同，同一个基因，调整的效果相同，保证稳定性
        // 选择属性
        let mutation_property_index = activation_gene
            .map(|g| g.id % properties.len())
            .unwrap_or(0);
        // 选择调整强度，根据参数类型
        if properties[mutation_property_index] == 0.0 || properties[mutation_property_index] == 1.0
        {
            // 二值属性，直接翻转
            // 例如：是否分裂属性，翻转后神经元将进入分裂状态，生成一个新神经元
        } else {
            // 连续属性，根据权重调整一定比例
            // 例如：神经元层类型属性，调整后可能改变神经元的层类型，从而改变其在网络中的作用
        }
    }
    // 生物基因表达
    pub fn express_creature(bug: &Bug) {}
}
