// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLEANROOM Capsicum `cap_rights_*` bit operations.

use crate::consts::{
    CAP_ALL0, CAP_ALL1, CAP_FEXECVE, CAP_INDEX0, CAP_INDEX1, CAP_MMAP, CAP_MMAP_R, CAP_MMAP_W,
    CAP_MMAP_X, CAP_PREAD, CAP_PWRITE, CAP_READ, CAP_RIGHTS_VERSION, CAP_WRITE,
};

/// A `cap_rights_t` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapRights {
    /// Two 64-bit array elements.
    pub r: [u64; 2],
}

/// A rejected rights operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapError {
    /// The right does not carry the index bit for its slot.
    BadIndex,
    /// The rights value is structurally invalid.
    Invalid,
}

impl CapRights {
    /// An empty set with both index bits.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            r: [CAP_INDEX0, CAP_INDEX1],
        }
    }

    /// Build a set from one or more rights.
    #[must_use]
    pub fn init(rights: &[u64]) -> Self {
        let mut value = Self::empty();
        for right in rights {
            let _ = value.set(*right);
        }
        value
    }

    /// Add one right.
    fn set(&mut self, right: u64) -> Result<(), CapError> {
        let slot = slot_of(right)?;
        let target = self.r.get_mut(slot).ok_or(CapError::BadIndex)?;
        *target |= right;
        Ok(())
    }

    /// Remove one right.
    pub fn clear(&mut self, right: u64) -> Result<(), CapError> {
        let slot = slot_of(right)?;
        let target = self.r.get_mut(slot).ok_or(CapError::BadIndex)?;
        *target &= !right;
        self.restore_index(slot);
        Ok(())
    }

    /// Whether every right in `rights` is present.
    #[must_use]
    pub fn is_set(&self, rights: &[u64]) -> bool {
        rights.iter().all(|right| self.contains_one(*right))
    }

    /// Whether no right is present.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.r[0] == CAP_INDEX0 && self.r[1] == CAP_INDEX1
    }

    /// Whether the value is a version-0, two-element rights set.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        if self.r[0] & (3u64 << 62) != CAP_RIGHTS_VERSION {
            return false;
        }
        if self.r[0] & !CAP_ALL0 != 0 || self.r[1] & !CAP_ALL1 != 0 {
            return false;
        }
        self.r[0] & CAP_INDEX0 != 0 && self.r[1] & CAP_INDEX1 != 0
    }

    /// Union `other` into `self`.
    pub fn merge(&mut self, other: &Self) {
        self.r[0] |= other.r[0];
        self.r[1] |= other.r[1];
        self.restore_index(0);
        self.restore_index(1);
    }

    /// Remove every right present in `other`.
    pub fn remove(&mut self, other: &Self) {
        self.r[0] &= !other.r[0];
        self.r[1] &= !other.r[1];
        self.restore_index(0);
        self.restore_index(1);
    }

    /// Whether every right of `other` is present.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        self.r[0] & other.r[0] == other.r[0] && self.r[1] & other.r[1] == other.r[1]
    }

    /// Intersect with `other`.
    pub fn intersect(&mut self, other: &Self) {
        self.r[0] &= other.r[0];
        self.r[1] &= other.r[1];
        self.restore_index(0);
        self.restore_index(1);
    }

    fn contains_one(&self, right: u64) -> bool {
        let Ok(slot) = slot_of(right) else {
            return false;
        };
        self.r.get(slot).is_some_and(|value| value & right == right)
    }

    fn restore_index(&mut self, slot: usize) {
        if slot == 0 {
            self.r[0] |= CAP_INDEX0;
        } else {
            self.r[1] |= CAP_INDEX1;
        }
    }
}

impl Default for CapRights {
    fn default() -> Self {
        Self::empty()
    }
}

fn slot_of(right: u64) -> Result<usize, CapError> {
    let index0 = right & CAP_INDEX0 != 0;
    let index1 = right & CAP_INDEX1 != 0;
    match (index0, index1) {
        (true, false) => Ok(0),
        (false, true) => Ok(1),
        _ => Err(CapError::BadIndex),
    }
}

/// Map the handle-level rights onto the Raven rights mask.
#[must_use]
pub fn to_raven_rights(rights: &CapRights) -> u32 {
    let mut mask = 0u32;
    if rights.contains_one(CAP_READ) || rights.contains_one(CAP_PREAD) {
        mask |= 1;
    }
    if rights.contains_one(CAP_WRITE) || rights.contains_one(CAP_PWRITE) {
        mask |= 2;
    }
    if rights.contains_one(CAP_MMAP)
        || rights.contains_one(CAP_MMAP_R)
        || rights.contains_one(CAP_MMAP_W)
        || rights.contains_one(CAP_MMAP_X)
    {
        mask |= 4;
    }
    if rights.contains_one(CAP_FEXECVE) {
        mask |= 8;
    }
    mask
}

#[cfg(test)]
#[path = "cap_rights_ut.rs"]
mod tests;
