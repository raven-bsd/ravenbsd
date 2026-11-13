// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fd entry array: 128 inline entries, an overflow array sized by the hard `RLIMIT_NOFILE`,
//! and a 64-bit-word bitmap with a `first_free` hint.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::fd::{DEFAULT_HARD_NOFILE, DEFAULT_SOFT_NOFILE};

/// `FD_CLOEXEC` in the fd flag byte.
pub const FD_CLOEXEC: u8 = 1;

/// Number of inline entries; the overflow array starts here.
pub const INLINE_ENTRIES: usize = 128;

/// One fd table slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FdEntry {
    /// OFD slot index; zero means free.
    pub ofd: u32,
    /// Bit 0 is `FD_CLOEXEC`; the rest is reserved and must stay zero.
    pub fd_flags: u8,
    /// fdsan owner tag (zero means unowned).
    pub tag: u64,
}

impl FdEntry {
    /// A free entry.
    pub const FREE: Self = Self {
        ofd: 0,
        fd_flags: 0,
        tag: 0,
    };

    /// Whether the entry is free.
    pub const fn is_free(&self) -> bool {
        self.ofd == 0
    }

    /// Whether `FD_CLOEXEC` is set.
    pub const fn cloexec(&self) -> bool {
        self.fd_flags & FD_CLOEXEC != 0
    }
}

/// The fd table.
pub struct FdTable {
    inline: [FdEntry; INLINE_ENTRIES],
    overflow: Option<Vec<FdEntry>>,
    bitmap: Vec<u64>,
    first_free: u32,
    soft_limit: u32,
    hard_limit: u32,
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new(DEFAULT_SOFT_NOFILE, DEFAULT_HARD_NOFILE)
    }
}

impl FdTable {
    /// A table with the given soft and hard `RLIMIT_NOFILE` values.
    ///
    /// The hard limit sizes the lazily allocated overflow array; the soft limit bounds
    /// allocation, exactly as the rlimit cell does in the shell.
    pub fn new(soft_limit: u32, hard_limit: u32) -> Self {
        let hard = hard_limit.max(INLINE_ENTRIES as u32);
        let words = (hard as usize).div_ceil(64).max(2);
        Self {
            inline: [FdEntry::FREE; INLINE_ENTRIES],
            overflow: None,
            bitmap: vec![0u64; words],
            first_free: 0,
            soft_limit: soft_limit.min(hard),
            hard_limit: hard,
        }
    }

    /// The current soft limit.
    pub const fn soft_limit(&self) -> u32 {
        self.soft_limit
    }

    /// The current hard limit.
    pub const fn hard_limit(&self) -> u32 {
        self.hard_limit
    }

    /// Change the soft limit (the hard limit never grows past the construction value).
    pub fn set_soft_limit(&mut self, soft: u32) {
        self.soft_limit = soft.min(self.hard_limit);
    }

    /// Change both limits; the caller is responsible for closing descriptors above a lowered
    /// hard limit. Growing allocates the overflow array when it is first needed.
    pub fn set_hard_limit(&mut self, hard: u32) {
        let hard = hard.max(INLINE_ENTRIES as u32);
        self.hard_limit = hard;
        self.soft_limit = self.soft_limit.min(hard);
        let words = (hard as usize).div_ceil(64).max(2);
        if self.bitmap.len() < words {
            self.bitmap.resize(words, 0);
        }
    }

    fn ensure_capacity(&mut self, fd: u32) -> Result<(), Errno> {
        if fd >= self.hard_limit {
            return Err(Errno::EBADF);
        }
        if fd < INLINE_ENTRIES as u32 {
            return Ok(());
        }
        if self.overflow.is_none() {
            let entries = self.hard_limit.saturating_sub(INLINE_ENTRIES as u32) as usize;
            self.overflow = Some(vec![FdEntry::FREE; entries]);
        }
        Ok(())
    }

    /// The entry for `fd`, or `None` when the fd is outside the table.
    pub fn entry(&self, fd: i32) -> Option<&FdEntry> {
        let index = usize::try_from(fd).ok()?;
        if index >= self.hard_limit as usize {
            return None;
        }
        if index < INLINE_ENTRIES {
            self.inline.get(index)
        } else {
            self.overflow
                .as_ref()?
                .get(index.saturating_sub(INLINE_ENTRIES))
        }
    }

    /// The mutable entry for `fd`.
    pub fn entry_mut(&mut self, fd: i32) -> Option<&mut FdEntry> {
        let index = usize::try_from(fd).ok()?;
        if index >= self.hard_limit as usize {
            return None;
        }
        if index < INLINE_ENTRIES {
            self.inline.get_mut(index)
        } else {
            self.overflow
                .as_mut()?
                .get_mut(index.saturating_sub(INLINE_ENTRIES))
        }
    }

    /// Whether `fd` names an open descriptor.
    pub fn is_open(&self, fd: i32) -> bool {
        self.entry(fd).is_some_and(|entry| !entry.is_free())
    }

    fn set_used(&mut self, fd: u32) {
        let word = (fd as usize) / 64;
        let bit = (fd as usize) % 64;
        if let Some(slot) = self.bitmap.get_mut(word) {
            *slot |= 1u64 << bit;
        }
    }

    fn set_free(&mut self, fd: u32) {
        let word = (fd as usize) / 64;
        let bit = (fd as usize) % 64;
        if let Some(slot) = self.bitmap.get_mut(word) {
            *slot &= !(1u64 << bit);
        }
        if fd < self.first_free {
            self.first_free = fd;
        }
    }

    fn is_used(&self, fd: u32) -> bool {
        let word = (fd as usize) / 64;
        let bit = (fd as usize) % 64;
        self.bitmap
            .get(word)
            .is_some_and(|slot| slot & (1u64 << bit) != 0)
    }

    /// Find the lowest free fd at or above `min_fd`.
    pub fn find_free(&self, min_fd: u32) -> Option<u32> {
        let mut candidate = min_fd.max(self.first_free);
        while candidate < self.soft_limit {
            if !self.is_used(candidate) {
                return Some(candidate);
            }
            candidate = candidate.saturating_add(1);
        }
        None
    }

    /// Allocate the lowest free fd at or above `min_fd`, mark it used, and return it.
    ///
    /// Returns `EMFILE` when the soft limit is reached and `EINVAL` when `min_fd` is above the
    /// soft limit (`F_DUPFD` with too large an argument).
    pub fn alloc(&mut self, min_fd: u32) -> Result<u32, Errno> {
        if min_fd >= self.soft_limit {
            return Err(Errno::EINVAL);
        }
        let fd = self.find_free(min_fd).ok_or(Errno::EMFILE)?;
        self.ensure_capacity(fd)?;
        self.set_used(fd);
        if let Some(entry) = self.entry_mut(fd as i32) {
            *entry = FdEntry::FREE;
        }
        Ok(fd)
    }

    /// Install an OFD at exactly `fd`.
    pub fn install_at(&mut self, fd: u32, ofd: u32, cloexec: bool, tag: u64) -> Result<(), Errno> {
        if fd >= self.hard_limit {
            return Err(Errno::EBADF);
        }
        self.ensure_capacity(fd)?;
        self.set_used(fd);
        let entry = self.entry_mut(fd as i32).ok_or(Errno::EBADF)?;
        *entry = FdEntry {
            ofd,
            fd_flags: if cloexec { FD_CLOEXEC } else { 0 },
            tag,
        };
        Ok(())
    }

    /// Remove `fd` and return its entry.
    pub fn remove(&mut self, fd: i32) -> Option<FdEntry> {
        let entry = *self.entry(fd)?;
        if entry.is_free() {
            return None;
        }
        if let Some(slot) = self.entry_mut(fd) {
            *slot = FdEntry::FREE;
        }
        if let Ok(index) = u32::try_from(fd) {
            self.set_free(index);
        }
        Some(entry)
    }

    /// Set or clear `FD_CLOEXEC`.
    pub fn set_cloexec(&mut self, fd: i32, cloexec: bool) -> Result<(), Errno> {
        let entry = self.entry_mut(fd).ok_or(Errno::EBADF)?;
        if entry.is_free() {
            return Err(Errno::EBADF);
        }
        if cloexec {
            entry.fd_flags |= FD_CLOEXEC;
        } else {
            entry.fd_flags &= !FD_CLOEXEC;
        }
        Ok(())
    }

    /// Set or clear the owner tag.
    pub fn set_tag(&mut self, fd: i32, tag: u64) -> Result<u64, Errno> {
        let entry = self.entry_mut(fd).ok_or(Errno::EBADF)?;
        if entry.is_free() {
            return Err(Errno::EBADF);
        }
        let previous = entry.tag;
        entry.tag = tag;
        Ok(previous)
    }

    /// The owner tag, or zero for an unknown fd.
    pub fn tag(&self, fd: i32) -> u64 {
        self.entry(fd).map_or(0, |entry| entry.tag)
    }

    /// Every open `(fd, entry)`, in ascending order.
    pub fn open_entries(&self) -> Vec<(i32, FdEntry)> {
        let mut out = Vec::new();
        let mut fd = 0u32;
        while fd < self.hard_limit {
            if self.is_used(fd)
                && let Ok(index) = i32::try_from(fd)
                && let Some(entry) = self.entry(index)
                && !entry.is_free()
            {
                out.push((index, *entry));
            }
            fd = fd.saturating_add(1);
        }
        out
    }

    /// Close every descriptor `>= first`, returning the removed entries.
    pub fn close_from(&mut self, first: u32) -> Vec<(i32, FdEntry)> {
        let mut removed = Vec::new();
        let mut fd = first;
        while fd < self.hard_limit {
            if self.is_used(fd)
                && let Ok(index) = i32::try_from(fd)
                && let Some(entry) = self.remove(index)
            {
                removed.push((index, entry));
            }
            fd = fd.saturating_add(1);
        }
        removed
    }

    /// Close a half-open range `[first, last)`.
    pub fn close_range(&mut self, first: u32, last: u32) -> Vec<(i32, FdEntry)> {
        let mut removed = Vec::new();
        let mut fd = first;
        while fd < last && fd < self.hard_limit {
            if self.is_used(fd)
                && let Ok(index) = i32::try_from(fd)
                && let Some(entry) = self.remove(index)
            {
                removed.push((index, entry));
            }
            fd = fd.saturating_add(1);
        }
        removed
    }

    /// Number of open descriptors.
    pub fn open_count(&self) -> usize {
        self.bitmap
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }
}
