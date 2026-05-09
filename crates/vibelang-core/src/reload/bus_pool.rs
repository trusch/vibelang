//! Bus allocator with entity affinity for live reloading.
//!
//! This module provides a bus allocator that maintains affinity between
//! entities and their allocated buses. When an entity is updated (not
//! deleted), it keeps its existing bus. When an entity is deleted, its
//! bus goes into a pool for reuse.

use crate::types::{BusId, GroupId};
use std::collections::HashMap;

/// Bus allocator with entity affinity.
///
/// Manages audio bus allocation with the following goals:
/// - Entities keep their bus across updates (no audio glitches)
/// - Deleted entities release their bus to a pool
/// - New entities get buses from the pool (if available) or allocate new ones
///
/// # Example
///
/// ```ignore
/// let mut allocator = BusAllocator::new(16); // Start at bus 16
///
/// // First allocation
/// let bus1 = allocator.get_or_alloc(GroupId::new(1)); // Returns BusId(16)
/// let bus2 = allocator.get_or_alloc(GroupId::new(2)); // Returns BusId(17)
///
/// // Same group gets same bus
/// let bus1_again = allocator.get_or_alloc(GroupId::new(1)); // Returns BusId(16)
///
/// // Release a bus
/// allocator.release(GroupId::new(1));
///
/// // New group gets the freed bus
/// let bus3 = allocator.get_or_alloc(GroupId::new(3)); // Returns BusId(16)
/// ```
#[derive(Clone, Debug)]
pub struct BusAllocator {
    /// Map of group ID to its assigned bus.
    group_buses: HashMap<GroupId, BusId>,

    /// Pool of freed buses available for reuse.
    free_buses: Vec<BusId>,

    /// Next bus ID if pool is empty.
    next_bus_id: u32,
}

impl BusAllocator {
    /// Create a new bus allocator.
    ///
    /// # Arguments
    /// * `start_bus_id` - First bus ID to allocate (typically 16 to reserve 0-15 for I/O)
    pub fn new(start_bus_id: u32) -> Self {
        Self {
            group_buses: HashMap::new(),
            free_buses: Vec::new(),
            next_bus_id: start_bus_id,
        }
    }

    /// Get the bus for a group, allocating if needed.
    ///
    /// If the group already has a bus, returns that bus.
    /// Otherwise, takes a bus from the pool or allocates a new one.
    pub fn get_or_alloc(&mut self, group_id: GroupId) -> BusId {
        if let Some(&bus) = self.group_buses.get(&group_id) {
            return bus;
        }

        // Try to reuse a freed bus, otherwise allocate new stereo pair
        let bus = self.free_buses.pop().unwrap_or_else(|| {
            let id = BusId::new(self.next_bus_id);
            self.next_bus_id += 2; // Allocate stereo pair (2 consecutive buses)
            id
        });

        self.group_buses.insert(group_id, bus);
        bus
    }

    /// Get the bus for a group without allocating.
    ///
    /// Returns None if the group doesn't have a bus.
    pub fn get(&self, group_id: GroupId) -> Option<BusId> {
        self.group_buses.get(&group_id).copied()
    }

    /// Release a group's bus back to the pool.
    ///
    /// The bus will be available for reuse by new groups.
    pub fn release(&mut self, group_id: GroupId) {
        if let Some(bus) = self.group_buses.remove(&group_id) {
            self.free_buses.push(bus);
        }
    }

    /// Release multiple groups' buses.
    pub fn release_many(&mut self, group_ids: impl IntoIterator<Item = GroupId>) {
        for id in group_ids {
            self.release(id);
        }
    }

    /// Check if a group has an allocated bus.
    pub fn has_bus(&self, group_id: GroupId) -> bool {
        self.group_buses.contains_key(&group_id)
    }

    /// Get the number of allocated buses.
    pub fn allocated_count(&self) -> usize {
        self.group_buses.len()
    }

    /// Get the number of freed buses in the pool.
    pub fn pool_size(&self) -> usize {
        self.free_buses.len()
    }

    /// Get the next bus ID that would be allocated.
    pub fn next_id(&self) -> u32 {
        self.next_bus_id
    }

    /// Clear all allocations and reset to initial state.
    pub fn clear(&mut self, start_bus_id: u32) {
        self.group_buses.clear();
        self.free_buses.clear();
        self.next_bus_id = start_bus_id;
    }

    /// Get all currently allocated group IDs.
    pub fn allocated_groups(&self) -> impl Iterator<Item = GroupId> + '_ {
        self.group_buses.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_allocator() {
        let allocator = BusAllocator::new(16);
        assert_eq!(allocator.next_id(), 16);
        assert_eq!(allocator.allocated_count(), 0);
        assert_eq!(allocator.pool_size(), 0);
    }

    #[test]
    fn test_first_allocation() {
        let mut allocator = BusAllocator::new(16);
        let bus = allocator.get_or_alloc(GroupId::new(1));

        assert_eq!(bus.0, 16);
        assert_eq!(allocator.next_id(), 18); // Stereo: increments by 2
        assert_eq!(allocator.allocated_count(), 1);
    }

    #[test]
    fn test_same_group_same_bus() {
        let mut allocator = BusAllocator::new(16);

        let bus1 = allocator.get_or_alloc(GroupId::new(1));
        let bus2 = allocator.get_or_alloc(GroupId::new(1));

        assert_eq!(bus1, bus2);
        assert_eq!(allocator.allocated_count(), 1);
    }

    #[test]
    fn test_different_groups_different_buses() {
        let mut allocator = BusAllocator::new(16);

        let bus1 = allocator.get_or_alloc(GroupId::new(1));
        let bus2 = allocator.get_or_alloc(GroupId::new(2));

        assert_ne!(bus1, bus2);
        assert_eq!(allocator.allocated_count(), 2);
    }

    #[test]
    fn test_release_and_reuse() {
        let mut allocator = BusAllocator::new(16);

        let bus1 = allocator.get_or_alloc(GroupId::new(1));
        assert_eq!(bus1.0, 16);

        allocator.release(GroupId::new(1));
        assert_eq!(allocator.allocated_count(), 0);
        assert_eq!(allocator.pool_size(), 1);

        // New group should get the freed bus
        let bus3 = allocator.get_or_alloc(GroupId::new(3));
        assert_eq!(bus3.0, 16); // Reused!
        assert_eq!(allocator.pool_size(), 0);
    }

    #[test]
    fn test_release_many() {
        let mut allocator = BusAllocator::new(16);

        allocator.get_or_alloc(GroupId::new(1));
        allocator.get_or_alloc(GroupId::new(2));
        allocator.get_or_alloc(GroupId::new(3));

        assert_eq!(allocator.allocated_count(), 3);

        allocator.release_many([GroupId::new(1), GroupId::new(2)]);

        assert_eq!(allocator.allocated_count(), 1);
        assert_eq!(allocator.pool_size(), 2);
    }

    #[test]
    fn test_get_without_alloc() {
        let mut allocator = BusAllocator::new(16);

        assert!(allocator.get(GroupId::new(1)).is_none());

        allocator.get_or_alloc(GroupId::new(1));

        assert!(allocator.get(GroupId::new(1)).is_some());
    }

    #[test]
    fn test_has_bus() {
        let mut allocator = BusAllocator::new(16);

        assert!(!allocator.has_bus(GroupId::new(1)));

        allocator.get_or_alloc(GroupId::new(1));

        assert!(allocator.has_bus(GroupId::new(1)));
    }

    #[test]
    fn test_clear() {
        let mut allocator = BusAllocator::new(16);

        allocator.get_or_alloc(GroupId::new(1));
        allocator.get_or_alloc(GroupId::new(2));
        allocator.release(GroupId::new(1));

        allocator.clear(20);

        assert_eq!(allocator.allocated_count(), 0);
        assert_eq!(allocator.pool_size(), 0);
        assert_eq!(allocator.next_id(), 20);
    }

    #[test]
    fn test_allocated_groups() {
        let mut allocator = BusAllocator::new(16);

        allocator.get_or_alloc(GroupId::new(1));
        allocator.get_or_alloc(GroupId::new(2));

        let groups: Vec<_> = allocator.allocated_groups().collect();
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&GroupId::new(1)));
        assert!(groups.contains(&GroupId::new(2)));
    }
}
