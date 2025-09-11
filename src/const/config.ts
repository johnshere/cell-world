export const RootEl = document.getElementById('app')!;

export const WorldConfig = {
  // 帧率控制
  FrameRate: 10,
  AccelerateMax: 10,
  /** 开启加速 */
  IsAutoAccelerate: true,
  /** fps超过时，加速 */
  FpsToAccelerate: 20,
  /** fps小于时，减速 */
  FpsToDecelerate: 10,
  /** 单次调整速度 */
  AccelerateStep: 0.5,
};

// 海洋配置
export const OceanConfig = {
  initEntities: 7000, // 最大细胞数量
  /** 自然诞生植物细胞的时间间隔（毫秒） */
  SpawnNaturalPlantInterval: 1000,
  /** 初始细胞生成权重（用以控制概率，权重为0表示不生成该类型） */
  SpawnWeights: {
    plant: 15, // 植物细胞权重（原模板为3份）
    herbiv: 11, // 植食细胞权重（原模板为2份）
    carniv: 1, // 肉食细胞权重（原模板为1份）
  },
};

export const CellConfig = {
  basedEnergy: 1, // 初始能量
  energyToSplit: 20, // 分裂所需的能量
  maxGeneration: 3, // 最大分裂次数
  maxNearingCells: 2, // 周围同类细胞数量超过此值时不分裂
  breathInterval: 500, // 呼吸间隔时间（毫秒）
  breathDuration: 3000, // 呼吸颜色持续时间（毫秒）
  breathColor: 'white', // 呼吸颜色
};

// 植物细胞配置
export const PlantCellConfig = {
  ...CellConfig,
};

// 植食细胞配置
export const HerbivCellConfig = {
  ...CellConfig,
  maxGeneration: 2, // 最大分裂次数
  maxNearingCells: 1, // 周围同类细胞数量超过此值时不分裂
  moveMinInterval: 1500, // 移动间隔时间（毫秒）
  moveMaxInterval: 4000, // 移动间隔时间（毫秒）
  /** 觅食范围 */
  huntRange: 5,
  basedEnergy: 4, // 初始能量
  energyToSplit: 60, // 分裂所需的能量
  energyToMove: -0.6, // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed: 20,
};

// 肉食细胞配置
export const CarnivCellConfig = {
  ...CellConfig,
  maxGeneration: 3, // 最大分裂次数
  maxNearingCells: 1, // 周围同类细胞数量超过此值时不分裂
  moveMinInterval: 700, // 移动间隔时间（毫秒）
  moveMaxInterval: 2000, // 移动间隔时间（毫秒）
  /** 觅食范围 */
  huntRange: 15,
  /** 低能量觅食范围 */
  huntRangeLowEnergy: 2,
  basedEnergy: 6, // 初始能量
  energyToSplit: 40, // 分裂所需的能量
  energyToMove: -1, // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed: 10,
  /** 低能量阈值（低于等于该值时进入待机：不移动不消耗能量） */
  lowEnergyThreshold: 10,
  /** 低能量状态下的能量消耗 */
  lowEnergyConsumption: 0.035,
};

declare global {
  interface Window {
    CellWorldConfig: {
      WorldConfig: typeof WorldConfig;
      OceanConfig: typeof OceanConfig;
      CellConfig: typeof CellConfig;
      PlantCellConfig: typeof PlantCellConfig;
      HerbivCellConfig: typeof HerbivCellConfig;
      CarnivCellConfig: typeof CarnivCellConfig;
    };
  }
}
window.CellWorldConfig = {
  WorldConfig,
  OceanConfig,
  CellConfig,
  PlantCellConfig,
  HerbivCellConfig,
  CarnivCellConfig,
};
console.log('CellWorldConfig', window.CellWorldConfig);
