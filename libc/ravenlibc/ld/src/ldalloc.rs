// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/{rtld_malloc.c,rtld_malloc.h} @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The loader's own fixed pool allocator (the 16 KiB bootstrap region and buckets).

/// The bootstrap pool size.
pub const BOOTSTRAP_BYTES: usize = 16 * 1024;
/// The number of free-list entries.
pub const MAX_BLOCKS: usize = 64;
/// The smallest bucket.
pub const MIN_BUCKET: usize = 16;

/// One free block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Block {
    offset: usize,
    len: usize,
}

/// A first-fit allocator over a fixed byte pool.
pub struct LdAlloc {
    pool: [u8; BOOTSTRAP_BYTES],
    free: [Block; MAX_BLOCKS],
    free_count: usize,
    bump: usize,
    live: usize,
    peak: usize,
    failed: bool,
}

impl LdAlloc {
    /// An empty allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool: [0; BOOTSTRAP_BYTES],
            free: [Block { offset: 0, len: 0 }; MAX_BLOCKS],
            free_count: 0,
            bump: 0,
            live: 0,
            peak: 0,
            failed: false,
        }
    }

    /// Allocate `size` bytes aligned to `align`.
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<usize> {
        if size == 0 {
            return None;
        }
        let align = align.max(MIN_BUCKET).next_power_of_two();
        for index in 0..self.free_count {
            let block = *self.free.get(index)?;
            let start = round_up(block.offset, align)?;
            let end = start.checked_add(size)?;
            let block_end = block.offset.checked_add(block.len)?;
            if end <= block_end {
                let removed = self.free.get(index).copied()?;
                self.free.get_mut(index)?.clone_from(&removed);
                self.free_count = self.free_count.checked_sub(1)?;
                if start > block.offset {
                    self.push_free(Block {
                        offset: block.offset,
                        len: start.checked_sub(block.offset)?,
                    });
                }
                if end < block_end {
                    self.push_free(Block {
                        offset: end,
                        len: block_end.checked_sub(end)?,
                    });
                }
                self.live = self.live.checked_add(size)?;
                self.peak = self.peak.max(self.live);
                return Some(start);
            }
        }
        let start = round_up(self.bump, align)?;
        let end = start.checked_add(size)?;
        if end > self.pool.len() {
            self.failed = true;
            return None;
        }
        self.bump = end;
        self.live = self.live.checked_add(size)?;
        self.peak = self.peak.max(self.live);
        Some(start)
    }

    /// Free an allocation returned by [`Self::alloc`].
    pub fn free(&mut self, offset: usize, size: usize) -> bool {
        if size == 0 || offset.checked_add(size).is_none() {
            return false;
        }
        for index in 0..self.free_count {
            if let Some(block) = self.free.get(index)
                && block.offset == offset
            {
                return false;
            }
        }
        self.push_free(Block { offset, len: size });
        self.live = self.live.saturating_sub(size);
        true
    }

    /// The number of live bytes.
    #[must_use]
    pub fn live(&self) -> usize {
        self.live
    }

    /// The peak live bytes.
    #[must_use]
    pub fn peak(&self) -> usize {
        self.peak
    }

    /// Whether an allocation failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The bytes still available in the bootstrap pool.
    #[must_use]
    pub fn available(&self) -> usize {
        self.pool.len().saturating_sub(self.bump)
    }

    fn push_free(&mut self, block: Block) {
        if self.free_count < MAX_BLOCKS
            && let Some(slot) = self.free.get_mut(self.free_count)
        {
            *slot = block;
            self.free_count = self.free_count.saturating_add(1);
        }
    }
}

impl Default for LdAlloc {
    fn default() -> Self {
        Self::new()
    }
}

fn round_up(value: usize, align: usize) -> Option<usize> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

#[cfg(test)]
#[path = "ldalloc_ut.rs"]
mod tests;
