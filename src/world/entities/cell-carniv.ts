import { CarnivCellConfig } from '../../const/config';

import Cell from './cell';
import CellHerbiv from './cell-herbiv';

/** 食肉细胞（以植食细胞为食） */
export default class CellCarniv extends Cell {
  private lastMoveTime = 0;
  private moveInterval = 0;
  private target: CellHerbiv | null = null; // 处于狩猎状态
  energy = CarnivCellConfig.basedEnergy;

  constructor() {
    super();

    // 随机设置移动间隔
    this.moveInterval =
      Math.random() *
        (CarnivCellConfig.moveMaxInterval - CarnivCellConfig.moveMinInterval) +
      CarnivCellConfig.moveMinInterval;

    this.color = 'DeepPink';
  }

  grow() {
    if (this.generation >= CarnivCellConfig.maxGeneration || this.energy <= 0) {
      this.die();
    }
    // 检查是否可以分裂
    if (this.energy >= CarnivCellConfig.energyToSplit) {
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
    if (!this.ocean) return;

    // 所有状态下都先累积移动计时，便于低能量追击同样遵循节奏
    this.lastMoveTime += this.deltaTime;

    // 低能量待机但可反击：不做随机游走；若目标在半径内则追击；否则仅在半径内尝试锁定目标。
    if (this.energy <= CarnivCellConfig.lowEnergyThreshold) {
      const halfRange = Math.max(1, Math.floor(CarnivCellConfig.huntRange / 2));

      // 检查现有目标是否仍在半径内
      let canPursue = false;
      if (this.target) {
        const withinHalf =
          Math.abs(this.target.row - this.row) <= halfRange &&
          Math.abs(this.target.col - this.col) <= halfRange &&
          this.ocean.entities.includes(this.target);
        if (!withinHalf) this.target = null;
        else canPursue = true;
      }

      // 若没有可追击目标，则在半径内尝试寻找
      if (!canPursue) {
        const found = this.huntInRange(halfRange);
        canPursue = !!found;
      }

      // 可追击则按节奏移动靠近目标，否则静止等待，并尝试吞食同格猎物
      if (canPursue) {
        if (
          this.lastMoveTime <
          this.moveInterval - this.energy * CarnivCellConfig.energyToSpeed
        ) {
          // 未到移动时间，仍尝试原地进食
          this.eatHerbivoresAtCurrentPosition();
          return;
        }
        this.lastMoveTime = 0;
        this.moveTowardsTarget();
        this.eatHerbivoresAtCurrentPosition();
        this.energy += CarnivCellConfig.energyToMove; // 追击产生移动能耗
        return;
      } else {
        // 静止不动，但可吞食同格猎物，不扣能量
        this.eatHerbivoresAtCurrentPosition();
        this.energy -= CarnivCellConfig.lowEnergyConsumption;
        return;
      }
    }

    // 正常能量：保留原有逻辑
    if (
      this.lastMoveTime <
      this.moveInterval - this.energy * CarnivCellConfig.energyToSpeed
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
        this.row = nextPos.row;
        this.col = nextPos.col;
      }
    }
    // 移动后检查当前位置是否有植食细胞并吃掉它们
    this.eatHerbivoresAtCurrentPosition();

    this.energy += CarnivCellConfig.energyToMove;
  }

  /** 向目标移动并尝试进食 */
  private moveTowardsTarget() {
    if (!this.target || !this.ocean) {
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
    const blocking = this.ocean
      .getCellEntities(nextRow, nextCol)
      .some(entity => entity !== this && !(entity instanceof CellHerbiv));

    if (!blocking) {
      // 移动到新位置
      this.row = nextRow;
      this.col = nextCol;
    }
    // 如果目标位置被非猎物占用，保持当前位置，下次再尝试
  }

  /** 在指定范围内寻找植食细胞目标（返回选择的目标或 null） */
  private huntInRange(range: number): CellHerbiv | null {
    if (!this.ocean) return null;

    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    const candidates = new Set<CellHerbiv>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const ents = this.ocean.getCellEntities(r, c);
        for (const e of ents) {
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
  private eatHerbivoresAtCurrentPosition() {
    if (!this.ocean) return;

    // 用索引快速取出当前格子的植食细胞
    const herbivsAtCurrentPosition = this.ocean
      .getCellEntities(this.row, this.col)
      .filter((e): e is CellHerbiv => e instanceof CellHerbiv);

    // 吃掉所有找到的植食细胞
    herbivsAtCurrentPosition.forEach(prey => {
      // 从海洋中移除猎物
      prey.die();

      // 增加能量：用被吃猎物的代际数作为增量
      this.energy += prey.energy;

      // 如果吃掉的是当前目标，清除目标
      if (prey === this.target) {
        this.target = null;
      }
    });

    if (herbivsAtCurrentPosition.length) {
      this.breath();
    }
  }

  /** 觅食 - 在一定范围内寻找植食细胞作为目标（正常状态下使用全范围） */
  hunt() {
    if (!this.ocean || this.target) return;

    const range = CarnivCellConfig.huntRange;

    // 搜索范围的正方形区域
    const startRow = this.row - range;
    const startCol = this.col - range;
    const endRow = this.row + range;
    const endCol = this.col + range;

    // 用索引收集候选猎物集合
    const candidates = new Set<CellHerbiv>();
    for (let r = startRow; r <= endRow; r++) {
      for (let c = startCol; c <= endCol; c++) {
        const ents = this.ocean.getCellEntities(r, c);
        for (const e of ents) {
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
