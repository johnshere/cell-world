export const RootEl = document.getElementById('app')!;

// 帧率控制
export const FrameRate = 10;

// 海洋配置
export const OceanConfig = {
  maxEntities: 500, // 最大细胞数量
  baseSpawnInterval: 100, // 基础生成间隔（毫秒）
  maxSpawnInterval: 1000, // 最大生成间隔（毫秒）
};

export const CellConfig = {
  maxSplitCount: 3, // 最大分裂次数
  maxNearingCells: 3, // 周围细胞数量超过此值时不分裂
  breathInterval: 1000, // 呼吸间隔时间（毫秒）
  breathDuration: 5000, // 呼吸颜色持续时间（毫秒）
  breathColor: 'white', // 呼吸颜色
};

// 植物细胞配置
export const PlantCellConfig = {
  ...CellConfig,
  splitMinInterval: 6000, // 分裂间隔时间（毫秒）
  splitMaxInterval: 20000, // 分裂间隔时间（毫秒）
};

// 植食细胞配置
export const HerbivCellConfig = {
  ...CellConfig,
  maxSplitCount: 5, // 最大分裂次数
  moveMinInterval: 400, // 移动间隔时间（毫秒）
  moveMaxInterval: 4000, // 移动间隔时间（毫秒）
  /** 觅食范围 */
  huntRange: 5,
  basedEnergy: 4, // 初始能量
  energyToSplit: 6, // 分裂所需的能量
  energyToMove: -0.1, // 移动所需的能量
};
