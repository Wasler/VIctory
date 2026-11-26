use std::collections::{BTreeMap, BTreeSet};

/// Free intervals in the part of a view heap used by temporary tables.
/// Take the smallest interval that fits, so small tables leave room for
/// larger ones. Released intervals coalesce before the next allocation.
pub(super) struct DynamicSlots {
    capacity: u32,
    ends: BTreeMap<u32, u32>,
    by_size: BTreeSet<(u32, u32)>,
}

impl DynamicSlots {
    pub(super) fn new(capacity: u32) -> Self {
        let mut slots = Self {
            capacity,
            ends: BTreeMap::new(),
            by_size: BTreeSet::new(),
        };
        if capacity > 0 {
            slots.insert(0, capacity);
        }
        slots
    }

    fn insert(&mut self, first: u32, end: u32) {
        self.ends.insert(first, end);
        self.by_size.insert((end - first, first));
    }

    fn remove(&mut self, first: u32, end: u32) {
        self.ends.remove(&first);
        self.by_size.remove(&(end - first, first));
    }

    pub(super) fn take(&mut self, count: u32) -> Option<u32> {
        if count == 0 {
            return None;
        }
        let &(size, first) = self.by_size.range((count, 0)..).next()?;
        let end = first + size;
        self.remove(first, end);
        if size > count {
            self.insert(first + count, end);
        }
        Some(first)
    }

    pub(super) fn release(&mut self, mut first: u32, count: u32) {
        let mut end = first.saturating_add(count).min(self.capacity);
        if first >= end {
            return;
        }
        if let Some((&left, &left_end)) = self.ends.range(..first).next_back()
            && left_end >= first
        {
            self.remove(left, left_end);
            first = left;
            end = end.max(left_end);
        }
        while let Some((&right, &right_end)) = self.ends.range(first..=end).next() {
            self.remove(right, right_end);
            end = end.max(right_end);
        }
        self.insert(first, end);
    }

    #[allow(dead_code)] // Diagnostic heap inspection in VIctory-NVIDIA.
    pub(super) fn taken(&self, index: u32) -> Option<bool> {
        (index < self.capacity)
            .then(|| self.ends.range(..=index).next_back().is_none_or(|(_, &end)| index >= end))
    }
}

#[cfg(test)]
mod tests {
    use super::DynamicSlots;

    #[test]
    fn small_allocations_preserve_larger_ranges() {
        let mut slots = DynamicSlots::new(16);
        assert_eq!(slots.take(16), Some(0));
        slots.release(0, 6);
        slots.release(8, 3);
        assert_eq!(slots.take(3), Some(8));
        assert_eq!(slots.take(6), Some(0));
    }

    #[test]
    fn releases_merge_touching_and_already_free_ranges() {
        let mut slots = DynamicSlots::new(16);
        assert_eq!(slots.take(16), Some(0));
        slots.release(8, 4);
        slots.release(0, 4);
        assert_eq!(slots.take(5), None);
        slots.release(3, 6);
        assert_eq!(slots.take(12), Some(0));
        slots.release(14, u32::MAX);
        assert_eq!(slots.take(3), None);
        assert_eq!(slots.take(2), Some(14));
    }
}
