use rustc_hash::FxHashMap;

/// LIFO 空闲列表 Slot 分配器
/// 管理 creature_id ↔ GPU slot 映射
pub struct SlotAllocator {
    max_slots: usize,
    /// creature_id → slot_index
    id_to_slot: FxHashMap<u64, usize>,
    /// slot_index → creature_id
    slot_to_id: FxHashMap<usize, u64>,
    /// 空闲 slot 栈（LIFO）
    free_list: Vec<usize>,
}

impl SlotAllocator {
    pub fn new(max_slots: usize) -> Self {
        let free_list: Vec<usize> = (0..max_slots).rev().collect();
        Self {
            max_slots,
            id_to_slot: FxHashMap::default(),
            slot_to_id: FxHashMap::default(),
            free_list,
        }
    }

    /// 分配一个 slot 给 creature_id，返回 slot_index
    pub fn allocate(&mut self, creature_id: u64) -> Option<usize> {
        // 已分配则返回已有的
        if let Some(&slot) = self.id_to_slot.get(&creature_id) {
            return Some(slot);
        }

        let slot = self.free_list.pop()?;
        self.id_to_slot.insert(creature_id, slot);
        self.slot_to_id.insert(slot, creature_id);
        Some(slot)
    }

    /// 释放 creature_id 的 slot
    pub fn free(&mut self, creature_id: u64) {
        if let Some(slot) = self.id_to_slot.remove(&creature_id) {
            self.slot_to_id.remove(&slot);
            self.free_list.push(slot);
        }
    }

    /// 获取 creature_id 对应的 slot
    pub fn get_slot(&self, creature_id: u64) -> Option<usize> {
        self.id_to_slot.get(&creature_id).copied()
    }

    /// 获取所有活跃的 (creature_id, slot_index) 对
    pub fn active_entries(&self) -> impl Iterator<Item = (&u64, &usize)> {
        self.id_to_slot.iter()
    }

    /// 当前使用数
    pub fn used_count(&self) -> usize {
        self.id_to_slot.len()
    }

    /// 最大 slot 数
    pub fn max_slots(&self) -> usize {
        self.max_slots
    }
}
