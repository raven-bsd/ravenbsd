// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2015 Nuxi, https://nuxi.nl/
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdlib/hsearch_r.c, hcreate_r.c, hdestroy_r.c and
// hsearch.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb (all BSD-2; the Nuxi files carry no
// per-file revision tag).
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // RavenBSD: donor probing arithmetic

//! `hsearch_r`, `hcreate_r`, `hdestroy_r` over opaque key words.
//!
//! Open addressing with quadratic probing, a power-of-two table and the donor's growth rule
//! `entries_used * 2 >= index_mask`; the per-table FNV-1a offset basis comes from [`Entropy`]
//! (the donor calls `arc4random_buf`, the shell supplies a real source, tests a counter). Keys
//! are not copied and `hdestroy` frees neither keys nor data (T-7); returned slot ids are
//! invalidated by growth, exactly as in the donor.

use alloc::vec::Vec;

/// One table entry; `key == 0` (the empty word) marks a free slot.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Entry {
    /// The caller's opaque key word.
    pub key: usize,
    /// The caller's opaque data word.
    pub data: usize,
}

/// The key text seam: hashing and comparison read the caller's strings.
pub trait KeyText {
    /// The NUL-free text of a key word.
    fn text(&self, key: usize) -> &[u8];
}

/// The entropy seam (PG-52); the donor fills the offset basis with `arc4random_buf`.
pub trait Entropy {
    /// Fill `buf`.
    fn fill(&mut self, buf: &mut [u8]);
}

/// The action of `hsearch`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Action {
    /// Look up only.
    Find,
    /// Insert when absent.
    Enter,
}

/// A hash-table failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HSearchError {
    /// `FIND` missed (`ESRCH`).
    NotFound,
    /// The table could not grow (`ENOMEM`).
    NoMem,
}

/// A table slot; growth invalidates it.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct SlotId(pub usize);

/// The hash table.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct HTable {
    entries: Vec<Option<Entry>>,
    offset_basis: u64,
    index_mask: usize,
    entries_used: usize,
}

impl HTable {
    /// The number of used entries.
    #[must_use]
    pub fn entries_used(&self) -> usize {
        self.entries_used
    }

    /// The current slot count.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.index_mask.wrapping_add(1)
    }

    /// `hsearch(item, action)`.
    ///
    /// # Errors
    ///
    /// [`HSearchError::NotFound`] for a `FIND` miss, [`HSearchError::NoMem`] when growth fails.
    pub fn hsearch<K: KeyText>(
        &mut self,
        key: usize,
        data: usize,
        action: Action,
        keys: &K,
    ) -> Result<SlotId, HSearchError> {
        let hash = self.hash(keys.text(key));
        let mut index = hash as usize;
        let mut step = 0usize;
        loop {
            let slot = index & self.index_mask;
            match self.entries.get(slot).copied().flatten() {
                None => {
                    if action == Action::Find {
                        return Err(HSearchError::NotFound);
                    }
                    if self.entries_used.wrapping_mul(2) >= self.index_mask {
                        self.grow(keys)?;
                        return self.insert_after_growth(key, data, hash, keys);
                    }
                    return self.store(slot, key, data);
                }
                Some(entry) => {
                    if keys.text(entry.key) == keys.text(key) {
                        return Ok(SlotId(slot));
                    }
                }
            }
            step = step.wrapping_add(1);
            index = index.wrapping_add(step);
        }
    }

    fn insert_after_growth<K: KeyText>(
        &mut self,
        key: usize,
        data: usize,
        hash: u64,
        keys: &K,
    ) -> Result<SlotId, HSearchError> {
        let mut index = hash as usize;
        let mut step = 0usize;
        loop {
            let slot = index & self.index_mask;
            match self.entries.get(slot).copied().flatten() {
                None => return self.store(slot, key, data),
                Some(entry) => {
                    if keys.text(entry.key) == keys.text(key) {
                        return Ok(SlotId(slot));
                    }
                }
            }
            step = step.wrapping_add(1);
            index = index.wrapping_add(step);
        }
    }

    fn store(&mut self, slot: usize, key: usize, data: usize) -> Result<SlotId, HSearchError> {
        let Some(entry) = self.entries.get_mut(slot) else {
            return Err(HSearchError::NoMem);
        };
        *entry = Some(Entry { key, data });
        self.entries_used = self.entries_used.wrapping_add(1);
        Ok(SlotId(slot))
    }

    fn grow<K: KeyText>(&mut self, keys: &K) -> Result<(), HSearchError> {
        let new_count = self.capacity().checked_mul(2).ok_or(HSearchError::NoMem)?;
        let mut new_entries: Vec<Option<Entry>> = Vec::new();
        new_entries
            .try_reserve_exact(new_count)
            .map_err(|_| HSearchError::NoMem)?;
        new_entries.resize(new_count, None);
        let old = core::mem::replace(&mut self.entries, new_entries);
        self.index_mask = new_count.wrapping_sub(1);
        for entry in old.into_iter().flatten() {
            let hash = self.hash(keys.text(entry.key));
            let mut index = hash as usize;
            let mut step = 0usize;
            loop {
                let slot = index & self.index_mask;
                if self.entries.get(slot).is_none_or(|slot| slot.is_none()) {
                    if let Some(target) = self.entries.get_mut(slot) {
                        *target = Some(entry);
                    }
                    break;
                }
                step = step.wrapping_add(1);
                index = index.wrapping_add(step);
            }
        }
        Ok(())
    }

    fn hash(&self, text: &[u8]) -> u64 {
        let mut hash = self.offset_basis;
        for byte in text {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
        hash
    }
}

/// `hcreate_r(nel)`: the hint is ignored and the table starts at 16 slots.
///
/// # Errors
///
/// [`HSearchError::NoMem`] when the initial table cannot be allocated.
pub fn hcreate<E: Entropy>(entropy: &mut E) -> Result<HTable, HSearchError> {
    let mut basis = [0u8; 8];
    entropy.fill(&mut basis);
    let offset_basis = u64::from_le_bytes(basis);
    let mut entries: Vec<Option<Entry>> = Vec::new();
    entries
        .try_reserve_exact(16)
        .map_err(|_| HSearchError::NoMem)?;
    entries.resize(16, None);
    Ok(HTable {
        entries,
        offset_basis,
        index_mask: 15,
        entries_used: 0,
    })
}

/// `hdestroy_r`: drops the table; keys and data are the caller's (T-7).
pub fn hdestroy(table: HTable) {
    drop(table);
}

#[cfg(test)]
#[path = "hsearch_ut.rs"]
mod tests;
