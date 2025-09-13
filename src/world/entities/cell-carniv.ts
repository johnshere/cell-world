import Cell from './cell';
import CellHerbiv from './cell-herbiv';
import CellPlant from './cell-plant';

/** 食肉细胞（以植食细胞为食） */
export default class CellCarniv extends Cell {
  private lastMoveTime = 0;
  private moveInterval = 0;
  private target: CellHerbiv | null = null; // 处于狩猎状态

  maxGeneration = 3; // 最大分裂次数
  maxNearingCells = 1; // 周围同类细胞数量超过此值时不分裂
  moveMinInterval = 300; // 移动间隔时间（毫秒）
  moveMaxInterval = 1000; // 移动间隔时间（毫秒）

  /** 觅食范围 */
  huntRange = 7;
  /** 低能量觅食范围 */
  huntRangeLowEnergy = 3;
  /** 捕猎失败被反杀的概率（比目标能量低时） */
  huntFailedRatio = 0.5;

  energy = 30;
  energyToSplit = 400; // 分裂所需的能量
  energyToMove = -3; // 移动所需的能量
  /** 能量对速度的加成 */
  energyToSpeed = 1;
  /** 低能量阈值（低于等于该值时进入待机：不移动不消耗能量） */
  lowEnergyThreshold = 0.2;
  /** 低能量状态下的能量消耗 */
  lowEnergyConsumption = 0.1;
  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() * (this.moveMaxInterval - this.moveMinInterval) +
      this.moveMinInterval;
    this.energy = this.energy * (Math.random() + 1);
    this.lowEnergyThreshold =
      (1.5 - Math.random()) * this.lowEnergyThreshold * this.energy;

    this.color = 'DeepPink';
  }
  grow() {
    if (this.generation >= this.maxGeneration) {
      this.die();
      return;
    }
    if (this.energy <= 0) {
      if (this.findSpecifyClassPositions(CellPlant).length > 6) {
        if (this.findSpecifyClassPositions(CellCarniv, 2).length === 0) {
          const newSelf = new CellHerbiv();
          newSelf.ocean = this.ocean;
          newSelf.setPosition(this.row, this.col);
          this.ocean.registerEntity(newSelf);
        }
      }
      this.die();
      return;
    }
    // 检查是否可以分裂
    if (this.energy >= this.energyToSplit) {
      const child = this.split() as CellCarniv;
      const energy = this.energy / 2;
      if (child) {
        child.energy = energy;
        this.energy = energy; // 分裂后重置能量
      }
    }
  }

  /** 移动到相邻位置 */
  move() {
    // 所有状态下都先累积移动计时，便于低能量追击同样遵循节奏
    this.lastMoveTime += this.deltaTime;

    // 低能量待机但可反击：不做随机游走；若目标在半径内则追击；否则仅在半径内尝试锁定目标。
    if (this.energy <= this.lowEnergyThreshold) {
      const huntRange = this.huntRangeLowEnergy;

      // 检查现有目标是否仍在半径内
      let canPursue = false;
      if (this.target) {
        const withinHalf =
          Math.abs(this.target.row - this.row) <= huntRange &&
          Math.abs(this.target.col - this.col) <= huntRange &&
          this.ocean.entities.includes(this.target);
        if (!withinHalf) this.target = null;
        else canPursue = true;
      }

      // 若没有可追击目标，则在半径内尝试寻找
      if (!canPursue) {
        const found = this.huntInRange(huntRange);
        canPursue = !!found;
      }

      // 可追击则按节奏移动靠近目标，否则静止等待，并尝试吞食同格猎物
      if (canPursue) {
        if (
          this.lastMoveTime <
          this.moveInterval - this.energy * this.energyToSpeed
        ) {
          // 未到移动时间，仍尝试原地进食
          this.attackAndEat();
          return;
        }
        this.lastMoveTime = 0;
        this.moveTowardsTarget();
        this.attackAndEat();
        this.energy += this.energyToMove; // 追击产生移动能耗
        return;
      } else {
        this.attackAndEat();
        this.energy -= this.lowEnergyConsumption;
        return;
      }
    }

    // 正常能量：保留原有逻辑
    if (
      this.lastMoveTime <
      this.moveInterval - this.energy * this.energyToSpeed
    ) {
      return;
    }
    this.lastMoveTime = 0;

    if (this.target) {
      // 处于狩猎状态则向目标移动
      this.moveTowardsTarget();
    } else {
      // 不处于狩猎状态，随机移动
      const nextPos = this.getNextMovePosition();
      if (nextPos) {
        this.setPosition(nextPos.row, nextPos.col);
      }
    }
    // 移动后检查当前位置是否有植食细胞并吃掉它们
    this.attackAndEat();

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

    // 检查目标位置是否被非猎物占用（用索引加速）
    const blockingSet = this.ocean.getCellSet(nextRow, nextCol);
    let blocking = false;
    if (blockingSet) {
      for (const entity of blockingSet) {
        if (entity !== this && !(entity instanceof CellHerbiv)) {
          blocking = true;
          break;
        }
      }
    }

    if (!blocking) {
      // 移动到新位置
      this.setPosition(nextRow, nextCol);
    }
    // 如果目标位置被非猎物占用，保持当前位置，下次再尝试
  }

  /** 在指定范围内寻找植食细胞目标（返回选择的目标或 null） */
  private huntInRange(range: number): CellHerbiv | null {
    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    const candidates = new Set<CellHerbiv>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const set = this.ocean.getCellSet(r, c);
        if (!set) continue;
        for (const e of set) {
          if (e instanceof CellHerbiv) {
            candidates.add(e);
          }
        }
      }
    }

    if (candidates.size === 0) return null;

    for (const e of this.ocean.entities) {
      if (candidates.has(e as CellHerbiv)) {
        this.target = e as CellHerbiv;
        return this.target;
      }
    }
    return null;
  }

  /** 吃掉当前位置的植食细胞 */
  private attackAndEat() {
    // 用索引快速取出当前格子的植食细胞
    const herbivSet = this.ocean.getCellSet(this.row, this.col);
    if (!herbivSet) return;
    const herbivsAtCurrentPosition: CellHerbiv[] = [];
    for (const e of herbivSet) {
      if (e instanceof CellHerbiv) herbivsAtCurrentPosition.push(e);
    }

    // 吃掉所有找到的植食细胞
    for (const prey of herbivsAtCurrentPosition) {
      if (this.energy < prey.energy) {
        if (Math.random() < this.huntFailedRatio) {
          this.die();
          return;
        }
      }
      // 从海洋中移除猎物
      prey.die();

      // 增加能量：用被吃猎物的代际数作为增量
      this.energy += prey.energy;

      // 如果吃掉的是当前目标，清除目标
      if (prey === this.target) {
        this.target = null;
      }

      this.breath();
    }
  }

  /** 觅食 - 在一定范围内寻找植食细胞作为目标（正常状态下使用全范围） */
  hunt() {
    if (this.target) return;

    const range = this.huntRange;

    // 搜索范围的正方形区域
    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    // 用索引收集候选猎物集合
    const candidates = new Set<CellHerbiv>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const set = this.ocean.getCellSet(r, c);
        if (!set) continue;
        for (const e of set) {
          if (e instanceof CellHerbiv) {
            candidates.add(e);
          }
        }
      }
    }

    if (candidates.size === 0) return;

    // 按原 entities 顺序选择首个候选，尽量不改变既有逻辑
    for (const e of this.ocean.entities) {
      if (candidates.has(e as CellHerbiv)) {
        this.target = e as CellHerbiv;
        break;
      }
    }
  }
}
