export const RootEl = document.getElementById('app')!;

export const WorldConfig = {
  // 帧率控制
  FrameRate: 10,
  AccelerateMax: 70,
  /** 开启加速 */
  IsAutoAccelerate: true,
  /** fps超过时，加速 */
  FpsToAccelerate: 40,
  /** fps小于时，减速 */
  FpsToDecelerate: 30,
  /** 单次调整速度 */
  AccelerateStep: 0.4,
};

// 海洋配置
export const OceanConfig = {
  initEntities: 7000, // 最大细胞数量
  /** 自然诞生植物细胞的比率（世界大小/rate）（单位时间（1s）内投放比例） */
  SpawnNaturalPlantRate: 15000, // xxx个细胞诞生一个
  /** 初始细胞生成权重（用以控制概率，权重为0表示不生成该类型） */
  SpawnWeights: {
    plant: 15, // 植物细胞权重（原模板为3份）
    herbiv: 5, // 植食细胞权重（原模板为2份）
    carniv: 0, // 肉食细胞权重（原模板为1份）
  },
};

declare global {
  interface Window {
    CellWorldConfig: {
      WorldConfig: typeof WorldConfig;
      OceanConfig: typeof OceanConfig;
    };
  }
}
window.CellWorldConfig = {
  WorldConfig,
  OceanConfig,
};
console.log('CellWorldConfig', window.CellWorldConfig);
