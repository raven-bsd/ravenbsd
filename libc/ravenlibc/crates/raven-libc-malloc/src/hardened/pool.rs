// SPDX-License-Identifier: ISC
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// Copyright (c) 2008, 2010, 2011, 2016, 2023 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2012 Matthew Dempsky <matthew@openbsd.org>
// Copyright (c) 2008 Damien Miller <djm@openbsd.org>
// Copyright (c) 2000 Poul-Henning Kamp <phk@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: malloc.c,v 1.302 2026/11/04 12:37:25 otto Exp $)
// The full ISC permission text and the Beerware remark are in hardened/mod.rs.

//! Pool, region and chunk metadata layouts for the hardened backend.
//!
//! The metadata itself lives in provider pages; these `#[repr(C)]` structs define the offsets and
//! the `*Ref` wrappers give safe accessors over a `usize` address.

use crate::hardened::consts::{BUCKETS, CHUNK_INFO_SIZE, DELAYED, MAX_CACHE_ENTRIES, MAX_CHUNKS};
use crate::raw;

/// One cached page run.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CacheEntry {
    /// Page address.
    pub page: usize,
    /// Length in pages.
    pub pages: u32,
}

/// One pool's mutable state.
#[repr(C)]
pub struct Pool {
    /// Canary derived from the process secret and this address.
    pub canary1: u32,
    /// Complement of `canary1`.
    pub canary2: u32,
    /// Recursion guard.
    pub active: u32,
    /// Lock index.
    pub mutex: u32,
    /// Junk level.
    pub malloc_junk: i32,
    /// Reserved mapping flags.
    pub mmap_flag: u32,
    /// Region hash table address.
    pub regions: usize,
    /// Region slots.
    pub regions_total: usize,
    /// Free region slots.
    pub regions_free: usize,
    /// Chunk-metadata batch address.
    pub chunk_pages: usize,
    /// Used pages in the batch.
    pub chunk_pages_used: usize,
    /// Free chunk-info list head.
    pub chunk_info_free: usize,
    /// Chunk pages with free slots, one head per bucket.
    pub chunk_dir: [usize; BUCKETS],
    /// Delayed-free slots.
    pub delayed: [usize; DELAYED],
    /// Random byte stream.
    pub rbytes: [u8; 32],
    /// Bytes consumed from `rbytes`.
    pub rbytesused: usize,
    /// Junk location variation.
    pub junk_loc: u32,
    /// Cached page runs.
    pub cache_count: usize,
    /// Cached pages.
    pub cache_used: usize,
    /// Page cache.
    pub cache: [CacheEntry; MAX_CACHE_ENTRIES],
    /// Bytes obtained from the provider and still held.
    pub mapped: usize,
    /// Bytes in live allocations.
    pub in_use: usize,
    /// Bytes in live page allocations.
    pub page_in_use: usize,
    /// Number of live page allocations.
    pub page_count: usize,
}

/// One region-table entry.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RegionInfo {
    /// Page address, with `bucket + 1` in the low bits for chunk pages.
    pub p: usize,
    /// Region size, or the `ChunkInfo` address for chunk pages.
    pub size: usize,
}

/// One chunk page's bitmap and per-chunk requested sizes.
#[repr(C)]
pub struct ChunkInfo {
    /// Next free/partial list link.
    pub next: usize,
    /// Page address.
    pub page: usize,
    /// Bucket.
    pub bucket: u32,
    /// Free chunks.
    pub free: u32,
    /// Total chunks.
    pub total: u32,
    /// Bitmap words in use.
    pub offset: u32,
    /// Pool canary.
    pub canary: u32,
    /// Requested size per chunk when canaries are on.
    pub sizes: [u16; MAX_CHUNKS],
    /// Free bitmap.
    pub bits: [u64; 4],
}

/// Byte offset of a `Pool` field.
macro_rules! pool_offset {
    ($field:ident) => {
        core::mem::offset_of!(Pool, $field)
    };
}

/// A safe accessor over one `Pool` in provider memory.
#[derive(Clone, Copy)]
pub struct PoolRef {
    address: usize,
}

impl PoolRef {
    /// Wrap `address`.
    #[must_use]
    pub const fn new(address: usize) -> Self {
        Self { address }
    }

    /// The raw address.
    #[must_use]
    pub const fn address(self) -> usize {
        self.address
    }

    fn get_u32(self, offset: usize) -> u32 {
        // SAFETY: a Pool lives in a mapped provider page and the offset is a field.
        raw::read_u32(self.address.wrapping_add(offset))
    }

    fn set_u32(self, offset: usize, value: u32) {
        // SAFETY: a Pool lives in a mapped provider page and the offset is a field.
        raw::write_u32(self.address.wrapping_add(offset), value);
    }

    fn get_i32(self, offset: usize) -> i32 {
        self.get_u32(offset) as i32
    }

    fn set_i32(self, offset: usize, value: i32) {
        self.set_u32(offset, value as u32);
    }

    fn get_usize(self, offset: usize) -> usize {
        // SAFETY: a Pool lives in a mapped provider page and the offset is a field.
        raw::read_usize(self.address.wrapping_add(offset))
    }

    fn set_usize(self, offset: usize, value: usize) {
        // SAFETY: a Pool lives in a mapped provider page and the offset is a field.
        raw::write_usize(self.address.wrapping_add(offset), value);
    }

    /// Canary 1.
    #[must_use]
    pub fn canary1(self) -> u32 {
        self.get_u32(pool_offset!(canary1))
    }

    /// Set canary 1.
    pub fn set_canary1(self, value: u32) {
        self.set_u32(pool_offset!(canary1), value);
    }

    /// Canary 2.
    #[must_use]
    pub fn canary2(self) -> u32 {
        self.get_u32(pool_offset!(canary2))
    }

    /// Set canary 2.
    pub fn set_canary2(self, value: u32) {
        self.set_u32(pool_offset!(canary2), value);
    }

    /// Recursion guard.
    #[must_use]
    pub fn active(self) -> u32 {
        self.get_u32(pool_offset!(active))
    }

    /// Set the recursion guard.
    pub fn set_active(self, value: u32) {
        self.set_u32(pool_offset!(active), value);
    }

    /// Lock index.
    #[must_use]
    pub fn mutex(self) -> u32 {
        self.get_u32(pool_offset!(mutex))
    }

    /// Set the lock index.
    pub fn set_mutex(self, value: u32) {
        self.set_u32(pool_offset!(mutex), value);
    }

    /// Junk level.
    #[must_use]
    pub fn malloc_junk(self) -> i32 {
        self.get_i32(pool_offset!(malloc_junk))
    }

    /// Set the junk level.
    pub fn set_malloc_junk(self, value: i32) {
        self.set_i32(pool_offset!(malloc_junk), value);
    }

    /// Region table.
    #[must_use]
    pub fn regions(self) -> usize {
        self.get_usize(pool_offset!(regions))
    }

    /// Set the region table.
    pub fn set_regions(self, value: usize) {
        self.set_usize(pool_offset!(regions), value);
    }

    /// Region slots.
    #[must_use]
    pub fn regions_total(self) -> usize {
        self.get_usize(pool_offset!(regions_total))
    }

    /// Set the region slot count.
    pub fn set_regions_total(self, value: usize) {
        self.set_usize(pool_offset!(regions_total), value);
    }

    /// Free region slots.
    #[must_use]
    pub fn regions_free(self) -> usize {
        self.get_usize(pool_offset!(regions_free))
    }

    /// Set the free region slot count.
    pub fn set_regions_free(self, value: usize) {
        self.set_usize(pool_offset!(regions_free), value);
    }

    /// Chunk metadata batch.
    #[must_use]
    pub fn chunk_pages(self) -> usize {
        self.get_usize(pool_offset!(chunk_pages))
    }

    /// Set the chunk metadata batch.
    pub fn set_chunk_pages(self, value: usize) {
        self.set_usize(pool_offset!(chunk_pages), value);
    }

    /// Used pages in the metadata batch.
    #[must_use]
    pub fn chunk_pages_used(self) -> usize {
        self.get_usize(pool_offset!(chunk_pages_used))
    }

    /// Set the used metadata pages.
    pub fn set_chunk_pages_used(self, value: usize) {
        self.set_usize(pool_offset!(chunk_pages_used), value);
    }

    /// Free chunk-info list head.
    #[must_use]
    pub fn chunk_info_free(self) -> usize {
        self.get_usize(pool_offset!(chunk_info_free))
    }

    /// Set the free chunk-info list head.
    pub fn set_chunk_info_free(self, value: usize) {
        self.set_usize(pool_offset!(chunk_info_free), value);
    }

    /// Head of a bucket's partial-page list.
    #[must_use]
    pub fn chunk_dir(self, bucket: usize) -> usize {
        let offset = pool_offset!(chunk_dir).wrapping_add(bucket.wrapping_mul(8));
        self.get_usize(offset)
    }

    /// Set a bucket's head.
    pub fn set_chunk_dir(self, bucket: usize, value: usize) {
        let offset = pool_offset!(chunk_dir).wrapping_add(bucket.wrapping_mul(8));
        self.set_usize(offset, value);
    }

    /// One delayed-free slot.
    #[must_use]
    pub fn delayed(self, slot: usize) -> usize {
        let offset = pool_offset!(delayed).wrapping_add(slot.wrapping_mul(8));
        self.get_usize(offset)
    }

    /// Set one delayed-free slot.
    pub fn set_delayed(self, slot: usize, value: usize) {
        let offset = pool_offset!(delayed).wrapping_add(slot.wrapping_mul(8));
        self.set_usize(offset, value);
    }

    /// One random byte.
    #[must_use]
    pub fn rbyte(self, index: usize) -> u8 {
        let offset = pool_offset!(rbytes).wrapping_add(index);
        // SAFETY: the index is bounded by the caller to 32.
        raw::read_u8(self.address.wrapping_add(offset))
    }

    /// Write one random byte.
    pub fn set_rbyte(self, index: usize, value: u8) {
        let offset = pool_offset!(rbytes).wrapping_add(index);
        // SAFETY: the index is bounded by the caller to 32.
        raw::write_u8(self.address.wrapping_add(offset), value);
    }

    /// Consumed random bytes.
    #[must_use]
    pub fn rbytesused(self) -> usize {
        self.get_usize(pool_offset!(rbytesused))
    }

    /// Set the consumed random-byte count.
    pub fn set_rbytesused(self, value: usize) {
        self.set_usize(pool_offset!(rbytesused), value);
    }

    /// Junk location.
    #[must_use]
    pub fn junk_loc(self) -> u32 {
        self.get_u32(pool_offset!(junk_loc))
    }

    /// Set the junk location.
    pub fn set_junk_loc(self, value: u32) {
        self.set_u32(pool_offset!(junk_loc), value);
    }

    /// Cached page runs.
    #[must_use]
    pub fn cache_count(self) -> usize {
        self.get_usize(pool_offset!(cache_count))
    }

    /// Set the cached page-run count.
    pub fn set_cache_count(self, value: usize) {
        self.set_usize(pool_offset!(cache_count), value);
    }

    /// Cached pages.
    #[must_use]
    pub fn cache_used(self) -> usize {
        self.get_usize(pool_offset!(cache_used))
    }

    /// Set the cached page count.
    pub fn set_cache_used(self, value: usize) {
        self.set_usize(pool_offset!(cache_used), value);
    }

    /// One cache entry.
    #[must_use]
    pub fn cache(self, index: usize) -> CacheEntry {
        let base = pool_offset!(cache).wrapping_add(index.wrapping_mul(16));
        CacheEntry {
            page: self.get_usize(base),
            pages: self.get_u32(base.wrapping_add(8)),
        }
    }

    /// Set one cache entry.
    pub fn set_cache(self, index: usize, value: CacheEntry) {
        let base = pool_offset!(cache).wrapping_add(index.wrapping_mul(16));
        self.set_usize(base, value.page);
        self.set_u32(base.wrapping_add(8), value.pages);
    }

    /// Bytes held from the provider.
    #[must_use]
    pub fn mapped(self) -> usize {
        self.get_usize(pool_offset!(mapped))
    }

    /// Set bytes held from the provider.
    pub fn set_mapped(self, value: usize) {
        self.set_usize(pool_offset!(mapped), value);
    }

    /// Bytes in live allocations.
    #[must_use]
    pub fn in_use(self) -> usize {
        self.get_usize(pool_offset!(in_use))
    }

    /// Set bytes in live allocations.
    pub fn set_in_use(self, value: usize) {
        self.set_usize(pool_offset!(in_use), value);
    }

    /// Bytes in live page allocations.
    #[must_use]
    pub fn page_in_use(self) -> usize {
        self.get_usize(pool_offset!(page_in_use))
    }

    /// Set bytes in live page allocations.
    pub fn set_page_in_use(self, value: usize) {
        self.set_usize(pool_offset!(page_in_use), value);
    }

    /// Live page allocations.
    #[must_use]
    pub fn page_count(self) -> usize {
        self.get_usize(pool_offset!(page_count))
    }

    /// Set live page allocations.
    pub fn set_page_count(self, value: usize) {
        self.set_usize(pool_offset!(page_count), value);
    }
}

/// A safe accessor over one `RegionInfo`.
#[derive(Clone, Copy)]
pub struct RegionRef {
    address: usize,
}

impl RegionRef {
    /// Wrap `address`.
    #[must_use]
    pub const fn new(address: usize) -> Self {
        Self { address }
    }

    /// The raw address of this entry.
    #[must_use]
    pub const fn address(self) -> usize {
        self.address
    }

    /// The page address with low bits.
    #[must_use]
    pub fn p(self) -> usize {
        // SAFETY: the region table is mapped provider memory.
        raw::read_usize(self.address)
    }

    /// Set the page address.
    pub fn set_p(self, value: usize) {
        // SAFETY: the region table is mapped provider memory.
        raw::write_usize(self.address, value);
    }

    /// The size or chunk-info address.
    #[must_use]
    pub fn size(self) -> usize {
        // SAFETY: the region table is mapped provider memory.
        raw::read_usize(self.address.wrapping_add(8))
    }

    /// Set the size or chunk-info address.
    pub fn set_size(self, value: usize) {
        // SAFETY: the region table is mapped provider memory.
        raw::write_usize(self.address.wrapping_add(8), value);
    }
}

/// A safe accessor over one `ChunkInfo`.
#[derive(Clone, Copy)]
pub struct ChunkInfoRef {
    address: usize,
}

impl ChunkInfoRef {
    /// Wrap `address`.
    #[must_use]
    pub const fn new(address: usize) -> Self {
        Self { address }
    }

    /// The raw address.
    #[must_use]
    pub const fn address(self) -> usize {
        self.address
    }

    fn get_u32(self, offset: usize) -> u32 {
        // SAFETY: ChunkInfo records are carved from mapped metadata pages.
        raw::read_u32(self.address.wrapping_add(offset))
    }

    fn set_u32(self, offset: usize, value: u32) {
        // SAFETY: ChunkInfo records are carved from mapped metadata pages.
        raw::write_u32(self.address.wrapping_add(offset), value);
    }

    fn get_usize(self, offset: usize) -> usize {
        // SAFETY: ChunkInfo records are carved from mapped metadata pages.
        raw::read_usize(self.address.wrapping_add(offset))
    }

    fn set_usize(self, offset: usize, value: usize) {
        // SAFETY: ChunkInfo records are carved from mapped metadata pages.
        raw::write_usize(self.address.wrapping_add(offset), value);
    }

    /// Next list link.
    #[must_use]
    pub fn next(self) -> usize {
        self.get_usize(core::mem::offset_of!(ChunkInfo, next))
    }

    /// Set the next list link.
    pub fn set_next(self, value: usize) {
        self.set_usize(core::mem::offset_of!(ChunkInfo, next), value);
    }

    /// Page address.
    #[must_use]
    pub fn page(self) -> usize {
        self.get_usize(core::mem::offset_of!(ChunkInfo, page))
    }

    /// Set the page address.
    pub fn set_page(self, value: usize) {
        self.set_usize(core::mem::offset_of!(ChunkInfo, page), value);
    }

    /// Bucket.
    #[must_use]
    pub fn bucket(self) -> u32 {
        self.get_u32(core::mem::offset_of!(ChunkInfo, bucket))
    }

    /// Set the bucket.
    pub fn set_bucket(self, value: u32) {
        self.set_u32(core::mem::offset_of!(ChunkInfo, bucket), value);
    }

    /// Free chunks.
    #[must_use]
    pub fn free(self) -> u32 {
        self.get_u32(core::mem::offset_of!(ChunkInfo, free))
    }

    /// Set free chunks.
    pub fn set_free(self, value: u32) {
        self.set_u32(core::mem::offset_of!(ChunkInfo, free), value);
    }

    /// Total chunks.
    #[must_use]
    pub fn total(self) -> u32 {
        self.get_u32(core::mem::offset_of!(ChunkInfo, total))
    }

    /// Set total chunks.
    pub fn set_total(self, value: u32) {
        self.set_u32(core::mem::offset_of!(ChunkInfo, total), value);
    }

    /// Bitmap words in use.
    #[must_use]
    pub fn offset(self) -> u32 {
        self.get_u32(core::mem::offset_of!(ChunkInfo, offset))
    }

    /// Set bitmap words in use.
    pub fn set_offset(self, value: u32) {
        self.set_u32(core::mem::offset_of!(ChunkInfo, offset), value);
    }

    /// Pool canary.
    #[must_use]
    pub fn canary(self) -> u32 {
        self.get_u32(core::mem::offset_of!(ChunkInfo, canary))
    }

    /// Set the pool canary.
    pub fn set_canary(self, value: u32) {
        self.set_u32(core::mem::offset_of!(ChunkInfo, canary), value);
    }

    /// The requested size of `chunknum` when canaries are on.
    #[must_use]
    pub fn size_at(self, chunknum: usize) -> u16 {
        let offset = core::mem::offset_of!(ChunkInfo, sizes).wrapping_add(chunknum.wrapping_mul(2));
        // SAFETY: `chunknum` is below `MAX_CHUNKS`.
        raw::read_u16(self.address.wrapping_add(offset))
    }

    /// Store the requested size of `chunknum`.
    pub fn set_size_at(self, chunknum: usize, value: u16) {
        let offset = core::mem::offset_of!(ChunkInfo, sizes).wrapping_add(chunknum.wrapping_mul(2));
        // SAFETY: `chunknum` is below `MAX_CHUNKS`.
        raw::write_u16(self.address.wrapping_add(offset), value);
    }

    fn word(self, index: usize) -> u64 {
        let offset = core::mem::offset_of!(ChunkInfo, bits).wrapping_add(index.wrapping_mul(8));
        // SAFETY: there are four bitmap words.
        raw::read_u64(self.address.wrapping_add(offset))
    }

    fn set_word(self, index: usize, value: u64) {
        let offset = core::mem::offset_of!(ChunkInfo, bits).wrapping_add(index.wrapping_mul(8));
        // SAFETY: there are four bitmap words.
        raw::write_u64(self.address.wrapping_add(offset), value);
    }

    /// One bitmap word by index (0..4).
    #[must_use]
    pub fn word_at(self, index: usize) -> u64 {
        match index {
            0 => self.word(0),
            1 => self.word(1),
            2 => self.word(2),
            _ => self.word(3),
        }
    }

    /// Whether chunk `index` is free.
    #[must_use]
    pub fn is_free(self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;
        self.word(word) & (1u64 << bit) != 0
    }

    /// Mark chunk `index` free.
    pub fn free_chunk(self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        let value = self.word(word) | (1u64 << bit);
        self.set_word(word, value);
    }

    /// Mark chunk `index` busy.
    pub fn busy_chunk(self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        let value = self.word(word) & !(1u64 << bit);
        self.set_word(word, value);
    }

    /// Initialise the record for a page of `bucket` chunks.
    pub fn initialize(self, page: usize, bucket: usize, canary: u32) {
        let chunk_bytes = crate::hardened::consts::bucket_bytes(bucket);
        let total = crate::consts::PAGE_SIZE / chunk_bytes;
        self.set_page(page);
        self.set_bucket(bucket as u32);
        self.set_total(total as u32);
        self.set_free(total as u32);
        let offset = total.div_ceil(64);
        self.set_offset(offset as u32);
        self.set_canary(canary);
        let mut word = 0usize;
        while word < 4 {
            self.set_word(word, 0);
            word = word.wrapping_add(1);
        }
        let mut index = 0usize;
        while index < total {
            self.free_chunk(index);
            index = index.wrapping_add(1);
        }
        let mut index = 0usize;
        while index < MAX_CHUNKS {
            self.set_size_at(index, 0);
            index = index.wrapping_add(1);
        }
    }
}

const _: () = assert!(core::mem::size_of::<Pool>() <= crate::consts::PAGE_SIZE);
const _: () = assert!(core::mem::size_of::<ChunkInfo>() <= CHUNK_INFO_SIZE);

#[cfg(test)]
#[path = "pool_ut.rs"]
mod tests;
