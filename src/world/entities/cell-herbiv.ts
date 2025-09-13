import Cell from './cell';
import CellCarniv from './cell-carniv';
import CellPlant from './cell-plant';

/** 植食细胞 */
export default class CellHerbiv extends Cell {
  maxGeneration = 2; // 最大分裂次数
  maxNearingCells = 1; // 周围同类细胞数量超过此值时不分裂
  moveMinInterval = 600; // 移动间隔时间（毫秒）
  moveMaxInterval = 1000; // 移动间隔时间（毫秒）
  /** 觅食范围 */
  huntRange = 3;

  energy = 100;
  energyToSplit = 100; // 分裂所需的能量
  energyToMove = -2.5; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed = 0.5;
  /** 低于此能量百分比时必定向目标移动 */
  private energyThresholdPercent = 0.6;
  private lastMoveTime = 0;
  private moveInterval = 0;
  private target: CellPlant | null = null; // 处于狩猎状态

  // 鸟群算法相关属性
  /** 速度向量 */
  private velocity = { x: 0, y: 0 };
  /** 最大速度 */
  private maxSpeed = 1;
  /** 感知范围 */
  private perceptionRange = 4;
  /** 分离权重 */
  private separationWeight = 1.0;
  /** 对齐权重 */
  private alignmentWeight = 5;
  /** 聚集权重 */
  private cohesionWeight = 2;
  /** 速度衰减系数 */
  private velocityDecay = 0.9;

  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() * (this.moveMaxInterval - this.moveMinInterval) +
      this.moveMinInterval;

    this.color = 'sandybrown';
  }

  /** 对齐 - 与邻近同类细胞保持相同的移动方向 */
  align() {
    const steer = { x: 0, y: 0 };
    let count = 0;

    // 获取感知范围内的位置
    const nearPositions = this.getNearPositions(this.perceptionRange);

    for (const pos of nearPositions) {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) continue;

      for (const entity of set) {
        if (entity instanceof CellHerbiv && entity !== this) {
          // 计算距离
          const dx = this.col - entity.col;
          const dy = this.row - entity.row;
          const distance = Math.sqrt(dx * dx + dy * dy);

          if (distance > 0 && distance < this.perceptionRange) {
            // 累加邻居的速度向量
            steer.x += entity.velocity.x;
            steer.y += entity.velocity.y;
            count++;
          }
        }
      }
    }

    if (count > 0) {
      // 计算平均速度
      steer.x /= count;
      steer.y /= count;

      // 归一化并应用权重
      const magnitude = Math.sqrt(steer.x * steer.x + steer.y * steer.y);
      if (magnitude > 0) {
        steer.x = (steer.x / magnitude) * this.alignmentWeight;
        steer.y = (steer.y / magnitude) * this.alignmentWeight;
      }
    }

    return steer;
  }

  /** 分离 - 避免与邻近同类细胞过于拥挤 */
  separate() {
    const steer = { x: 0, y: 0 };
    let count = 0;

    // 获取感知范围内的位置
    const nearPositions = this.getNearPositions(this.perceptionRange);

    for (const pos of nearPositions) {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) continue;

      for (const entity of set) {
        if (entity instanceof CellHerbiv && entity !== this) {
          // 计算距离
          const dx = this.col - entity.col;
          const dy = this.row - entity.row;
          const distance = Math.sqrt(dx * dx + dy * dy);

          if (distance > 0 && distance < this.perceptionRange) {
            // 计算分离向量（远离邻居）
            const separateX = dx / distance;
            const separateY = dy / distance;

            // 距离越近，分离力越强
            const force = 1 / distance;
            steer.x += separateX * force;
            steer.y += separateY * force;
            count++;
          }
        }
      }
    }

    if (count > 0) {
      // 平均化分离向量
      steer.x /= count;
      steer.y /= count;

      // 归一化并应用权重
      const magnitude = Math.sqrt(steer.x * steer.x + steer.y * steer.y);
      if (magnitude > 0) {
        steer.x = (steer.x / magnitude) * this.separationWeight;
        steer.y = (steer.y / magnitude) * this.separationWeight;
      }
    }

    return steer;
  }

  /** 聚集 - 向邻近同类细胞的中心位置移动 */
  cohesion() {
    const center = { x: 0, y: 0 };
    let count = 0;

    // 获取感知范围内的位置
    const nearPositions = this.getNearPositions(this.perceptionRange);

    for (const pos of nearPositions) {
      const set = this.ocean.getCellSet(pos.row, pos.col);
      if (!set) continue;

      for (const entity of set) {
        if (entity instanceof CellHerbiv && entity !== this) {
          // 计算距离
          const dx = this.col - entity.col;
          const dy = this.row - entity.row;
          const distance = Math.sqrt(dx * dx + dy * dy);

          if (distance > 0 && distance < this.perceptionRange) {
            // 累加邻居位置
            center.x += entity.col;
            center.y += entity.row;
            count++;
          }
        }
      }
    }

    const steer = { x: 0, y: 0 };

    if (count > 0) {
      // 计算邻居的中心位置
      center.x /= count;
      center.y /= count;

      // 计算向中心的方向向量
      const dx = center.x - this.col;
      const dy = center.y - this.row;

      // 归一化并应用权重
      const magnitude = Math.sqrt(dx * dx + dy * dy);
      if (magnitude > 0) {
        steer.x = (dx / magnitude) * this.cohesionWeight;
        steer.y = (dy / magnitude) * this.cohesionWeight;
      }
    }

    return steer;
  }

  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      const nearSameCells = this.findSpecifyClassPositions(CellHerbiv, 2);
      if (nearSameCells.length > 0) {
        const nearPlantCells = this.findSpecifyClassPositions(CellPlant, 2);
        if (nearPlantCells.length === 0) {
          // 转换为肉食细胞
          const newSelf = new CellCarniv();
          newSelf.energy = this.energy;
          newSelf.ocean = this.ocean;
          newSelf.setPosition(this.row, this.col);
          this.ocean.registerEntity(newSelf);
          this.die();
          return;
        }
      }
      this.die();
      return;
    }
    // 检查是否可以分裂
    if (this.energy >= this.energyToSplit) {
      const child = this.split() as CellHerbiv;
      const energy = this.energy / 2;
      if (child) {
        child.energy = energy;
        this.energy = energy; // 分裂后重置能量
      }
    }
  }

  /** 移动到相邻位置 */
  move() {
    this.lastMoveTime += this.deltaTime;
    // 检查是否可以移动
    if (
      this.lastMoveTime <
      this.moveInterval - this.energy * this.energyToSpeed
    ) {
      return;
    }
    this.lastMoveTime = 0;

    // 计算当前能量百分比
    const energyPercent = this.energy / this.energyToSplit;

    // 如果能量低于阈值且有目标，必定向目标移动
    if (energyPercent < this.energyThresholdPercent && this.target) {
      this.moveTowardsTarget();
    } else {
      // 使用鸟群算法计算移动方向
      const separation = this.separate();
      const alignment = this.align();
      const cohesion = this.cohesion();

      // 应用速度衰减
      this.velocity.x *= this.velocityDecay;
      this.velocity.y *= this.velocityDecay;

      // 整合所有行为
      const acceleration = {
        x: separation.x + alignment.x + cohesion.x,
        y: separation.y + alignment.y + cohesion.y,
      };

      // 更新速度
      this.velocity.x += acceleration.x;
      this.velocity.y += acceleration.y;

      // 限制最大速度
      const speed = Math.sqrt(
        this.velocity.x * this.velocity.x + this.velocity.y * this.velocity.y
      );
      if (speed > this.maxSpeed) {
        this.velocity.x = (this.velocity.x / speed) * this.maxSpeed;
        this.velocity.y = (this.velocity.y / speed) * this.maxSpeed;
      }

      // 根据速度移动
      if (Math.abs(this.velocity.x) > 0.1 || Math.abs(this.velocity.y) > 0.1) {
        const newCol = Math.round(this.col + this.velocity.x);
        const newRow = Math.round(this.row + this.velocity.y);

        this.setPosition(newRow, newCol);
      }
    }

    // 移动后检查当前位置是否有植物细胞并吃掉它们
    this.eatPlantsAtCurrentPosition();

    this.energy += this.energyToMove;
  }

  /** 向目标移动并尝试进食 */
  private moveTowardsTarget() {
    if (!this.target) {
      this.target = null;
      return;
    }

    // 检查目标是否还存在于海洋中
    if (!this.ocean.entities.includes(this.target)) {
      this.target = null;
      return;
    }

    // 计算向目标移动的方向
    const deltaRow = this.target.row - this.row;
    const deltaCol = this.target.col - this.col;

    // 检查是否已经到达目标位置
    if (deltaRow === 0 && deltaCol === 0) {
      // 已经在目标位置，清除目标
      this.target = null;
      return;
    }

    // 确定下一步移动位置
    let nextRow = this.row;
    let nextCol = this.col;

    if (Math.abs(deltaRow) > Math.abs(deltaCol)) {
      // 优先在行方向移动
      nextRow = this.row + (deltaRow > 0 ? 1 : -1);
    } else {
      // 优先在列方向移动
      nextCol = this.col + (deltaCol > 0 ? 1 : -1);
    }

    // 移动到新位置
    this.setPosition(nextRow, nextCol);
  }

  /** 吃掉当前位置的植物细胞 */
  private eatPlantsAtCurrentPosition() {
    // 用索引快速取出当前格子的植物（避免数组分配，先收集后处理）
    const set = this.ocean.getCellSet(this.row, this.col);
    if (!set) return;
    const plantsAtCurrentPosition: CellPlant[] = [];
    for (const e of set) {
      if (e instanceof CellPlant) plantsAtCurrentPosition.push(e);
    }

    // 吃掉所有找到的植物细胞
    plantsAtCurrentPosition.forEach(plant => {
      // 从海洋中移除植物
      plant.die();

      // 增加能量
      this.energy += plant.energy;

      // 如果吃掉的是当前目标，清除目标
      if (plant === this.target) {
        this.target = null;
      }
    });

    if (plantsAtCurrentPosition.length) {
      this.breath();
    }
  }

  /** 觅食 - 在一定范围内寻找植物细胞作为目标 */
  hunt() {
    if (this.target) return;

    const range = this.huntRange;

    // 根据方向确定搜索的正方形区域
    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    // 先用索引收集候选植物集合
    const candidates = new Set<CellPlant>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const set = this.ocean.getCellSet(r, c);
        if (!set) continue;
        for (const e of set) {
          if (e instanceof CellPlant) {
            candidates.add(e);
          }
        }
      }
    }

    if (candidates.size === 0) return;

    // 按原 entities 顺序选择首个候选，尽量不改变既有逻辑
    for (const e of this.ocean.entities) {
      if (candidates.has(e as CellPlant)) {
        this.target = e as CellPlant;
        break;
      }
    }
  }
}
