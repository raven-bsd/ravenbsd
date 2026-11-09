// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process handle table with rights and generation checks.
//!
//! A [`Handle`] packs a 24-bit table index and an 8-bit generation. Closing a slot increments its
//! generation, so a stale handle fails with `EBADF` instead of touching a reused object. Rights are
//! attached to the handle; [`HandleTable::dup`] can only intersect them. The table is generic so
//! the kernel can store its object enum while host tests use a small test object.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

/// Maximum number of handles in one process.
pub const MAX_HANDLES: usize = 65_536;

struct Slot<T> {
    value: Option<T>,
    rights: Rights,
    generation: u8,
}

impl<T> Slot<T> {
    const fn empty() -> Self {
        Self {
            value: None,
            rights: Rights::NONE,
            generation: 0,
        }
    }
}

/// A per-process handle table.
pub struct HandleTable<T> {
    slots: Vec<Slot<T>>,
}

impl<T> HandleTable<T> {
    /// Create an empty table.
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Number of occupied slots.
    pub fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.value.is_some())
            .count()
    }

    /// True when no slot is occupied.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: Clone> HandleTable<T> {
    /// Insert an object with `rights`, returning its handle.
    pub fn insert(&mut self, value: T, rights: Rights) -> Result<Handle, Errno> {
        let mut index = 0usize;
        while index < self.slots.len() {
            if let Some(slot) = self.slots.get(index)
                && slot.value.is_none()
            {
                return Ok(self.fill(index, value, rights));
            }
            index = index.saturating_add(1);
        }
        if self.slots.len() >= MAX_HANDLES {
            return Err(Errno::EMFILE);
        }
        let index = self.slots.len();
        self.slots.push(Slot::empty());
        Ok(self.fill(index, value, rights))
    }

    /// Look up `handle`, requiring every right in `needed`.
    pub fn get(&self, handle: Handle, needed: Rights) -> Result<&T, Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() {
            return Err(Errno::EBADF);
        }
        let value = slot.value.as_ref().ok_or(Errno::EBADF)?;
        if !slot.rights.contains(needed) {
            return Err(Errno::EACCES);
        }
        Ok(value)
    }

    /// Close `handle`, invalidating its generation.
    pub fn close(&mut self, handle: Handle) -> Result<(), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get_mut(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() || slot.value.is_none() {
            return Err(Errno::EBADF);
        }
        slot.value = None;
        slot.rights = Rights::NONE;
        slot.generation = slot.generation.wrapping_add(1);
        Ok(())
    }

    /// Duplicate `handle` with `rights` intersected with the old rights.
    pub fn dup(&mut self, handle: Handle, rights: Rights) -> Result<Handle, Errno> {
        let (value, old_rights) = {
            let slot_index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
            let slot = self.slots.get(slot_index).ok_or(Errno::EBADF)?;
            if slot.generation != handle.generation() {
                return Err(Errno::EBADF);
            }
            let value = slot.value.as_ref().ok_or(Errno::EBADF)?;
            if !slot.rights.contains(Rights::DUP) {
                return Err(Errno::EACCES);
            }
            (value.clone(), slot.rights)
        };
        self.insert(value, old_rights.intersect(rights))
    }

    fn fill(&mut self, index: usize, value: T, rights: Rights) -> Handle {
        let slot = match self.slots.get_mut(index) {
            Some(slot) => slot,
            None => return Handle::NONE,
        };
        slot.value = Some(value);
        slot.rights = rights;
        Handle::new(index as u32, slot.generation)
    }
}

impl<T> Default for HandleTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestObject(u32);

    fn table() -> HandleTable<TestObject> {
        HandleTable::new()
    }

    #[test]
    fn insert_and_get() {
        let mut table = table();
        let handle = table.insert(TestObject(7), Rights::READ).expect("insert");
        assert_eq!(handle.index(), 0);
        assert_eq!(handle.generation(), 0);
        assert_eq!(table.get(handle, Rights::READ), Ok(&TestObject(7)));
    }

    #[test]
    fn rights_are_checked() {
        let mut table = table();
        let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
        assert_eq!(table.get(handle, Rights::WRITE), Err(Errno::EACCES));
    }

    #[test]
    fn bad_index_is_ebadf() {
        let table = table();
        assert_eq!(
            table.get(Handle::new(3, 0), Rights::NONE),
            Err(Errno::EBADF)
        );
    }

    #[test]
    fn stale_generation_is_ebadf() {
        let mut table = table();
        let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
        table.close(handle).expect("close");
        assert_eq!(table.get(handle, Rights::NONE), Err(Errno::EBADF));
    }

    #[test]
    fn slot_reuse_increments_generation() {
        let mut table = table();
        let first = table.insert(TestObject(1), Rights::READ).expect("insert");
        table.close(first).expect("close");
        let second = table.insert(TestObject(2), Rights::READ).expect("insert");
        assert_eq!(second.index(), first.index());
        assert_eq!(second.generation(), first.generation().wrapping_add(1));
        assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
        assert_eq!(table.get(second, Rights::READ), Ok(&TestObject(2)));
    }

    #[test]
    fn dup_intersects_rights() {
        let mut table = table();
        let handle = table
            .insert(
                TestObject(3),
                Rights::READ.union(Rights::WRITE).union(Rights::DUP),
            )
            .expect("insert");
        let dup = table.dup(handle, Rights::READ).expect("dup");
        assert_eq!(table.get(dup, Rights::READ), Ok(&TestObject(3)));
        assert_eq!(table.get(dup, Rights::WRITE), Err(Errno::EACCES));
    }

    #[test]
    fn dup_requires_dup_right() {
        let mut table = table();
        let handle = table.insert(TestObject(3), Rights::READ).expect("insert");
        assert_eq!(table.dup(handle, Rights::READ), Err(Errno::EACCES));
    }

    #[test]
    fn close_twice_is_ebadf() {
        let mut table = table();
        let handle = table.insert(TestObject(3), Rights::ALL).expect("insert");
        assert_eq!(table.close(handle), Ok(()));
        assert_eq!(table.close(handle), Err(Errno::EBADF));
    }

    #[test]
    fn capacity_is_enforced() {
        let mut table = table();
        for index in 0..MAX_HANDLES {
            assert!(
                table.insert(TestObject(index as u32), Rights::READ).is_ok(),
                "insert {index}"
            );
        }
        assert_eq!(
            table.insert(TestObject(0), Rights::READ),
            Err(Errno::EMFILE)
        );
    }

    #[test]
    fn close_reuses_the_first_free_slot() {
        let mut table = table();
        let first = table.insert(TestObject(1), Rights::READ).expect("insert");
        let second = table.insert(TestObject(2), Rights::READ).expect("insert");
        table.close(first).expect("close");
        let third = table.insert(TestObject(3), Rights::READ).expect("insert");
        assert_eq!(third.index(), first.index());
        assert_eq!(second.index(), 1);
    }

    #[test]
    fn get_with_no_rights_still_needs_a_live_slot() {
        let table = table();
        assert_eq!(
            table.get(Handle::new(0, 0), Rights::NONE),
            Err(Errno::EBADF)
        );
    }

    #[test]
    fn close_with_wrong_generation_is_ebadf() {
        let mut table = table();
        let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
        assert_eq!(
            table.close(Handle::new(
                handle.index(),
                handle.generation().wrapping_add(1)
            )),
            Err(Errno::EBADF)
        );
    }

    #[test]
    fn is_empty_and_len_track_slots() {
        let mut table = table();
        assert!(table.is_empty());
        let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
        assert_eq!(table.len(), 1);
        table.close(handle).expect("close");
        assert!(table.is_empty());
    }

    #[test]
    fn duplicate_handle_can_be_closed_independently() {
        let mut table = table();
        let original = table
            .insert(TestObject(1), Rights::READ.union(Rights::DUP))
            .expect("insert");
        let duplicate = table.dup(original, Rights::READ).expect("dup");
        table.close(original).expect("close");
        assert_eq!(table.get(duplicate, Rights::READ), Ok(&TestObject(1)));
    }

    #[test]
    fn max_index_handle_is_out_of_range() {
        let table = table();
        assert_eq!(
            table.get(Handle::new(0x00FF_FFFF, 0), Rights::NONE),
            Err(Errno::EBADF)
        );
    }
}
