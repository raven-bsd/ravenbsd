// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process handle table with rights and generation checks.
//!
//! A [`Handle`] packs a 24-bit table index and an 8-bit generation. Closing a slot increments its
//! generation, so a stale handle fails with `EBADF` instead of touching a reused object. The
//! generation is exhausted after 255 closes: that slot is retired and never reused, so an early
//! stale handle can never become valid again. Rights are attached to the handle;
//! [`HandleTable::dup`] can only intersect them and [`HandleTable::get_with_rights`] returns the
//! stored mask without reconstructing it from a list of known bits. The table is generic so the
//! kernel can store its object enum while host tests use a small test object.

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
    /// A slot whose generation reached 255 after a close. Its stale handles must stay invalid
    /// forever, so `insert` never reuses it.
    retired: bool,
}

impl<T> Slot<T> {
    const fn empty() -> Self {
        Self {
            value: None,
            rights: Rights::NONE,
            generation: 0,
            retired: false,
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
    ///
    /// Free slots are reused in table order; a retired slot is skipped permanently.
    pub fn insert(&mut self, value: T, rights: Rights) -> Result<Handle, Errno> {
        let mut index = 0usize;
        while index < self.slots.len() {
            if let Some(slot) = self.slots.get(index)
                && slot.value.is_none()
                && !slot.retired
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

    /// Look up `handle`, requiring every right in `needed`, and return the `needed` entry with
    /// the exact rights stored in the slot.
    ///
    /// The rights mask is read from the slot, not reconstructed by probing a list of known bits,
    /// so every defined (and future) right round-trips without a mapping table.
    pub fn get_with_rights(&self, handle: Handle, needed: Rights) -> Result<(&T, Rights), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() {
            return Err(Errno::EBADF);
        }
        let value = slot.value.as_ref().ok_or(Errno::EBADF)?;
        if !slot.rights.contains(needed) {
            return Err(Errno::EACCES);
        }
        Ok((value, slot.rights))
    }

    /// Look up `handle`, requiring every right in `needed`.
    pub fn get(&self, handle: Handle, needed: Rights) -> Result<&T, Errno> {
        self.get_with_rights(handle, needed)
            .map(|(value, _rights)| value)
    }

    /// Rights stored for `handle`.
    pub fn rights(&self, handle: Handle) -> Result<Rights, Errno> {
        self.get_with_rights(handle, Rights::NONE)
            .map(|(_value, rights)| rights)
    }

    /// Close `handle`, invalidating its generation.
    ///
    /// Closing a slot at generation 255 retires it instead of wrapping to generation 0. The slot
    /// stays empty and unusable, so every handle issued for it before the retirement remains
    /// stale forever.
    pub fn close(&mut self, handle: Handle) -> Result<(), Errno> {
        let index = usize::try_from(handle.index()).map_err(|_| Errno::EBADF)?;
        let slot = self.slots.get_mut(index).ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() || slot.value.is_none() {
            return Err(Errno::EBADF);
        }
        slot.value = None;
        slot.rights = Rights::NONE;
        if slot.generation == u8::MAX {
            slot.retired = true;
        } else {
            slot.generation += 1;
        }
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
    fn generation_retires_the_slot_instead_of_wrapping() {
        let mut table = table();
        let first = table.insert(TestObject(1), Rights::READ).expect("insert");
        let mut current = first;
        for _ in 0..255 {
            table.close(current).expect("close");
            current = table.insert(TestObject(1), Rights::READ).expect("insert");
            assert_eq!(current.index(), first.index());
        }
        assert_eq!(current.generation(), u8::MAX);
        table.close(current).expect("retire");
        assert_eq!(table.get(current, Rights::NONE), Err(Errno::EBADF));
        assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
        let next = table.insert(TestObject(2), Rights::READ).expect("insert");
        assert_ne!(next.index(), first.index());
        assert_eq!(next.generation(), 0);
        assert_eq!(next.index(), first.index() + 1);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn retired_slot_stays_unused_across_later_inserts() {
        let mut table = table();
        let first = table.insert(TestObject(1), Rights::READ).expect("insert");
        let mut current = first;
        while current.generation() != u8::MAX {
            table.close(current).expect("close");
            current = table.insert(TestObject(1), Rights::READ).expect("insert");
        }
        table.close(current).expect("retire");
        for value in 0..4u32 {
            let handle = table
                .insert(TestObject(value), Rights::READ)
                .expect("insert");
            assert_eq!(handle.index(), first.index() + 1);
            table.close(handle).expect("close");
        }
        assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
    }

    #[test]
    fn rights_round_trip_for_every_defined_right() {
        const DEFINED: [Rights; 8] = [
            Rights::READ,
            Rights::WRITE,
            Rights::MAP,
            Rights::EXEC,
            Rights::DUP,
            Rights::TRANSFER,
            Rights::SIGNAL,
            Rights::ADMIN,
        ];
        let mut union = Rights::NONE;
        for right in DEFINED {
            union = union.union(right);
        }
        // Adding a right to `Rights::ALL` without adding it here fails this assertion.
        assert_eq!(union, Rights::ALL);
        for right in DEFINED {
            let mut table = table();
            let handle = table.insert(TestObject(9), right).expect("insert");
            let (value, stored) = table
                .get_with_rights(handle, right)
                .expect("get with the stored right");
            assert_eq!(value, &TestObject(9));
            assert_eq!(stored, right);
            assert_eq!(table.rights(handle), Ok(right));
        }
    }

    #[test]
    fn rights_are_read_without_requesting_extra_rights() {
        let mut table = table();
        let handle = table
            .insert(TestObject(4), Rights::READ.union(Rights::SIGNAL))
            .expect("insert");
        assert_eq!(
            table
                .get_with_rights(handle, Rights::NONE)
                .map(|item| item.1),
            Ok(Rights::READ.union(Rights::SIGNAL))
        );
        assert_eq!(table.get(handle, Rights::ADMIN), Err(Errno::EACCES));
    }

    #[test]
    fn closed_slot_rights_are_cleared() {
        let mut table = table();
        let handle = table.insert(TestObject(1), Rights::ALL).expect("insert");
        table.close(handle).expect("close");
        assert_eq!(table.rights(handle), Err(Errno::EBADF));
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
