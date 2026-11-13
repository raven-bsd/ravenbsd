// SPDX-License-Identifier: ISC
// Copyright (c) 2008, 2010, 2011, 2016, 2023 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2012 Matthew Dempsky <matthew@openbsd.org>
// Copyright (c) 2008 Damien Miller <djm@openbsd.org>
// Copyright (c) 2000 Poul-Henning Kamp <phk@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: malloc.c,v 1.302 2026/11/04 12:37:25 otto Exp $)
// The full ISC permission text and the Beerware remark are in hardened/mod.rs.

//! Small-chunk allocation, delayed free, junk and canaries.

use crate::consts::PAGE_SIZE;
use crate::hardened::consts::{
    CHECK_LENGTH, DELAYED, DELAYED_MASK, JUNK_ALLOC, JUNK_FREE, JUNK_FREE_WORD, bucket_bytes,
    find_bucket,
};
use crate::hardened::pool::{ChunkInfoRef, PoolRef, RegionRef};
use crate::raw;

impl<P, L, E, X> super::Hardened<P, L, E, X>
where
    P: crate::provider::PageProvider,
    L: crate::provider::Lock,
    E: crate::provider::Entropy,
    X: crate::provider::Platform,
{
    /// One random byte, refilled in 32-byte batches.
    pub(crate) fn getrbyte(&self, pool: PoolRef) -> u8 {
        if pool.rbytesused() >= 32 {
            let mut bytes = [0u8; 32];
            if !self.entropy.fill(&mut bytes) {
                self.error(pool, b"malloc", "malloc: no entropy");
            }
            let mut index = 0usize;
            while let Some(&byte) = bytes.get(index) {
                pool.set_rbyte(index, byte);
                index = index.wrapping_add(1);
            }
            pool.set_rbytesused(1usize.wrapping_add(usize::from(bytes[0]) % 16));
        }
        let index = pool.rbytesused();
        let value = pool.rbyte(index);
        pool.set_rbytesused(index.wrapping_add(1));
        value
    }

    /// Fill freed memory with the free junk pattern.
    pub(crate) fn junk_free(&self, pool: PoolRef, pointer: usize, size: usize) {
        if pool.malloc_junk() == 0 || size == 0 {
            return;
        }
        // SAFETY: the range is a live or just-freed allocation of `size` bytes.
        raw::fill(pointer, JUNK_FREE, size);
    }

    /// Validate the sampled free junk words.
    pub(crate) fn validate_junk(&self, pool: PoolRef, pointer: usize, size: usize) {
        if pool.malloc_junk() == 0 || size < 8 {
            return;
        }
        let words = size / 8;
        let step = if pool.malloc_junk() == 1 {
            (words / 4).max(1)
        } else {
            1
        };
        let start = (pool.junk_loc() as usize) % step;
        let mut index = start;
        while index < words {
            // SAFETY: the sampled word is inside the allocation.
            let value = raw::read_u64(pointer.wrapping_add(index.wrapping_mul(8)));
            if value != JUNK_FREE_WORD {
                self.error_offset(pool, b"free", "write to free mem", pointer, index);
            }
            index = index.wrapping_add(step);
        }
    }

    /// Write the trailing canary.
    pub(crate) fn fill_canary(&self, pointer: usize, requested: usize, allocated: usize) {
        let Some(remaining) = allocated.checked_sub(requested) else {
            return;
        };
        let check = remaining.min(CHECK_LENGTH);
        if check != 0 {
            // SAFETY: the canary lies inside the allocated chunk.
            raw::fill(pointer.wrapping_add(requested), self.chunk_canary(), check);
        }
    }

    /// Validate the trailing canary.
    pub(crate) fn validate_canary(
        &self,
        pool: PoolRef,
        pointer: usize,
        requested: usize,
        allocated: usize,
    ) {
        let Some(remaining) = allocated.checked_sub(requested) else {
            return;
        };
        let check = remaining.min(CHECK_LENGTH);
        let mut offset = 0usize;
        while offset < check {
            // SAFETY: the canary lies inside the allocation.
            let value = raw::read_u8(pointer.wrapping_add(requested).wrapping_add(offset));
            if value != self.chunk_canary() {
                self.error_canary(pool, pointer, requested, allocated, offset, value);
            }
            offset = offset.wrapping_add(1);
        }
    }

    /// Allocate one small chunk.
    pub(crate) fn malloc_bytes(&self, pool: PoolRef, request: usize) -> usize {
        let bucket = find_bucket(request);
        let mut info = ChunkInfoRef::new(pool.chunk_dir(bucket));
        if info.address() == 0 {
            let Some(fresh) = self.make_chunks(pool, bucket) else {
                return 0;
            };
            info = fresh;
        }
        let chunk_bytes = bucket_bytes(bucket);
        let total = info.total() as usize;
        let mut chunknum = total;
        let mut word_index = 0usize;
        while word_index < 4 {
            let word = info.word_at(word_index);
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                let candidate = word_index.wrapping_mul(64).wrapping_add(bit);
                if candidate < total {
                    chunknum = candidate;
                    break;
                }
            }
            word_index = word_index.wrapping_add(1);
        }
        if chunknum >= total {
            return 0;
        }
        info.busy_chunk(chunknum);
        let free = info.free().saturating_sub(1);
        info.set_free(free);
        if free == 0 {
            pool.set_chunk_dir(bucket, info.next());
            info.set_next(0);
        }
        if request > 0 {
            info.set_size_at(chunknum, request as u16);
        }
        let pointer = info.page().wrapping_add(chunknum.wrapping_mul(chunk_bytes));
        if bucket > 0 {
            self.validate_junk(pool, pointer, chunk_bytes);
            if pool.malloc_junk() != 0 {
                // SAFETY: the chunk is allocated and writable for `chunk_bytes`.
                raw::fill(pointer, JUNK_ALLOC, chunk_bytes);
            }
        }
        if pool_canaries(self.options()) && request > 0 {
            self.fill_canary(pointer, request, chunk_bytes);
        }
        pointer
    }

    /// Free one small chunk back to its page.
    pub(crate) fn free_bytes(&self, pool: PoolRef, info: ChunkInfoRef, pointer: usize) {
        let bucket = info.bucket() as usize;
        let chunk_bytes = bucket_bytes(bucket);
        let chunknum = (pointer & (PAGE_SIZE - 1)) / chunk_bytes;
        if pointer & 15 != 0 {
            self.error_ptr(pool, b"free", "modified chunk-pointer", pointer);
        }
        if chunknum >= info.total() as usize || info.is_free(chunknum) {
            self.error_ptr(pool, b"free", "double free", pointer);
        }
        info.free_chunk(chunknum);
        let free = info.free().saturating_add(1);
        info.set_free(free);
        if free == 1 {
            info.set_next(pool.chunk_dir(bucket));
            pool.set_chunk_dir(bucket, info.address());
        }
        if free == info.total() {
            self.remove_chunk_list(pool, bucket, info);
            self.unmap_pages(pool, info.page(), PAGE_SIZE, 0);
            let region = self.find_region(pool, info.page() | bucket.wrapping_add(1));
            if let Some(region) = region {
                self.delete_region(pool, region);
            }
            self.free_chunk_info(pool, info);
        }
    }

    fn remove_chunk_list(&self, pool: PoolRef, bucket: usize, target: ChunkInfoRef) {
        let mut previous = 0usize;
        let mut current = pool.chunk_dir(bucket);
        while current != 0 {
            let info = ChunkInfoRef::new(current);
            if current == target.address() {
                if previous == 0 {
                    pool.set_chunk_dir(bucket, info.next());
                } else {
                    ChunkInfoRef::new(previous).set_next(info.next());
                }
                return;
            }
            previous = current;
            current = info.next();
        }
    }

    /// The real size of a region, honouring the low bucket tag.
    #[must_use]
    pub(crate) fn region_size(region: RegionRef) -> usize {
        let tag = region.p() & (PAGE_SIZE - 1);
        if tag == 0 {
            region.size()
        } else {
            bucket_bytes(tag.wrapping_sub(1))
        }
    }

    /// Whether a region is a page allocation.
    #[must_use]
    pub(crate) fn is_page(region: RegionRef) -> bool {
        region.p() & (PAGE_SIZE - 1) == 0
    }

    /// Free a pointer in `pool`; false when it is not from this pool.
    pub(crate) fn free_pointer(
        &self,
        pool: PoolRef,
        pointer: usize,
        clear: bool,
        check: bool,
        argsz: usize,
    ) -> bool {
        let Some(region) = self.find_region(pool, pointer) else {
            return false;
        };
        let real = Self::region_size(region);
        if Self::is_page(region) {
            let requested = real.saturating_sub(self.options().malloc_guard);
            if check && requested < argsz {
                self.error_sizes(pool, b"free", "recorded size", requested, argsz);
            }
            if pool_canaries(self.options()) && requested > 0 {
                let rounded = crate::consts::align_up(requested, PAGE_SIZE).unwrap_or(requested);
                self.validate_canary(pool, pointer, requested, rounded);
            }
            if clear {
                // SAFETY: the live allocation is at least `argsz` bytes for `clear`.
                raw::fill_volatile(pointer, 0, argsz);
            }
            let page_size = crate::consts::align_up(real, PAGE_SIZE).unwrap_or(real);
            self.unmap_pages(pool, pointer, page_size, 0);
            self.delete_region(pool, region);
            return true;
        }
        let info = ChunkInfoRef::new(region.size());
        let chunk_bytes = bucket_bytes(info.bucket() as usize);
        let chunknum = (pointer & (PAGE_SIZE - 1)) / chunk_bytes;
        if pool_canaries(self.options()) && info.size_at(chunknum) > 0 {
            self.validate_canary(
                pool,
                pointer,
                usize::from(info.size_at(chunknum)),
                chunk_bytes,
            );
        }
        if check {
            if pool_canaries(self.options()) && info.size_at(chunknum) > 0 {
                if usize::from(info.size_at(chunknum)) < argsz {
                    self.error_sizes(
                        pool,
                        b"free",
                        "recorded size",
                        usize::from(info.size_at(chunknum)),
                        argsz,
                    );
                }
            } else if real < argsz {
                self.error_sizes(pool, b"free", "chunk size", real, argsz);
            }
        }
        if clear {
            // SAFETY: the caller asserted `argsz` live bytes.
            raw::fill_volatile(pointer, 0, argsz);
        }
        if self.options().malloc_freecheck {
            let mut slot = 0usize;
            while slot < DELAYED {
                let delayed = pool.delayed(slot);
                if delayed == pointer {
                    self.error_ptr(pool, b"free", "double free", pointer);
                }
                slot = slot.wrapping_add(1);
            }
        }
        self.junk_free(pool, pointer, real);
        let slot = usize::from(self.getrbyte(pool) & (DELAYED_MASK as u8));
        let previous = pool.delayed(slot);
        if previous == pointer {
            self.error_ptr(pool, b"free", "double free", pointer);
        }
        pool.set_delayed(slot, pointer);
        if previous != 0 {
            let Some(previous_region) = self.find_region(pool, previous) else {
                self.error_ptr(pool, b"free", "bogus pointer (double free?)", previous);
            };
            if !self.options().malloc_freecheck {
                let previous_size = Self::region_size(previous_region);
                self.validate_junk(pool, previous, previous_size);
            }
            let previous_info = ChunkInfoRef::new(previous_region.size());
            self.free_bytes(pool, previous_info, previous);
        }
        true
    }
}

fn pool_canaries(options: crate::compat::options::Options) -> bool {
    options.chunk_canaries
}

#[cfg(test)]
#[path = "chunks_ut.rs"]
mod tests;
