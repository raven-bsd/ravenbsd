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

//! Region table, page cache and chunk-metadata batches.

use crate::consts::{PAGE_SIZE, align_up};
use crate::hardened::consts::{
    CHUNK_INFO_SIZE, CHUNK_PAGES, INITIAL_REGIONS, JUNK_ALLOC, JUNK_FREE, MAX_REGIONS,
};
use crate::hardened::pool::{CacheEntry, ChunkInfoRef, PoolRef, RegionRef};
use crate::raw;

impl<P, L, E, X> super::Hardened<P, L, E, X>
where
    P: crate::provider::PageProvider,
    L: crate::provider::Lock,
    E: crate::provider::Entropy,
    X: crate::provider::Platform,
{
    /// Donor hash of a page address.
    #[must_use]
    pub(crate) fn hash(pointer: usize) -> usize {
        let value = pointer >> crate::consts::PAGE_SHIFT;
        let mut sum = value;
        sum = sum
            .wrapping_shl(7)
            .wrapping_sub(sum)
            .wrapping_add(value >> 16);
        sum = sum
            .wrapping_shl(7)
            .wrapping_sub(sum)
            .wrapping_add(value >> 32);
        sum.wrapping_shl(7)
            .wrapping_sub(sum)
            .wrapping_add(value >> 48)
    }

    pub(crate) fn region_at(&self, pool: PoolRef, index: usize) -> RegionRef {
        RegionRef::new(pool.regions().wrapping_add(index.wrapping_mul(16)))
    }

    /// Find `p` in the region table (low bits masked).
    pub(crate) fn find_region(&self, pool: PoolRef, pointer: usize) -> Option<RegionRef> {
        if pool.regions() == 0 || pool.regions_total() == 0 {
            return None;
        }
        let mask = pool.regions_total().wrapping_sub(1);
        let target = pointer & !(PAGE_SIZE - 1);
        let mut index = Self::hash(target) & mask;
        let mut steps = 0usize;
        while steps < pool.regions_total() {
            let slot = self.region_at(pool, index);
            let p = slot.p();
            if p == 0 {
                return None;
            }
            if p != usize::MAX && p & !(PAGE_SIZE - 1) == target {
                return Some(slot);
            }
            index = index.wrapping_sub(1) & mask;
            steps = steps.wrapping_add(1);
        }
        None
    }

    fn grow_regions(&self, pool: PoolRef) -> bool {
        let old_total = pool.regions_total();
        let new_total = if old_total == 0 {
            INITIAL_REGIONS
        } else {
            old_total.saturating_mul(2).min(MAX_REGIONS)
        };
        if new_total <= old_total {
            return false;
        }
        let Some(new_size) = new_total
            .checked_mul(16)
            .and_then(|size| align_up(size, PAGE_SIZE))
        else {
            return false;
        };
        let Some(table) = self
            .provider
            .map_pages(new_size, PAGE_SIZE, crate::map_prot::READ_WRITE)
        else {
            return false;
        };
        let old_table = pool.regions();
        let mut free = new_total;
        let mut index = 0usize;
        while index < old_total {
            let region = self.region_at(pool, index);
            let p = region.p();
            if p != 0 && p != usize::MAX {
                let mask = new_total.wrapping_sub(1);
                let mut target = Self::hash(p & !(PAGE_SIZE - 1)) & mask;
                let mut slot = RegionRef::new(table.wrapping_add(target.wrapping_mul(16)));
                let mut steps = 0usize;
                while slot.p() != 0 && steps < new_total {
                    target = target.wrapping_sub(1) & mask;
                    slot = RegionRef::new(table.wrapping_add(target.wrapping_mul(16)));
                    steps = steps.wrapping_add(1);
                }
                slot.set_p(p);
                slot.set_size(region.size());
                free = free.saturating_sub(1);
            }
            index = index.wrapping_add(1);
        }
        if old_table != 0 {
            let old_size = old_total.saturating_mul(16);
            let _ = self.provider.unmap(old_table, old_size);
            pool.set_mapped(pool.mapped().saturating_sub(old_size));
        }
        pool.set_regions(table);
        pool.set_regions_total(new_total);
        pool.set_regions_free(free);
        pool.set_mapped(pool.mapped().saturating_add(new_size));
        true
    }

    /// Insert one region; false on allocation failure.
    pub(crate) fn insert_region(&self, pool: PoolRef, p: usize, size: usize) -> bool {
        if (pool.regions_total() == 0
            || pool.regions_free().saturating_mul(4) < pool.regions_total())
            && !self.grow_regions(pool)
        {
            return false;
        }
        let mask = pool.regions_total().wrapping_sub(1);
        let mut index = Self::hash(p & !(PAGE_SIZE - 1)) & mask;
        let mut slot = self.region_at(pool, index);
        let mut steps = 0usize;
        while slot.p() != 0 && slot.p() != usize::MAX && steps < pool.regions_total() {
            index = index.wrapping_sub(1) & mask;
            slot = self.region_at(pool, index);
            steps = steps.wrapping_add(1);
        }
        if slot.p() != 0 && slot.p() != usize::MAX {
            return false;
        }
        slot.set_p(p);
        slot.set_size(size);
        pool.set_regions_free(pool.regions_free().saturating_sub(1));
        true
    }

    /// Delete a region, leaving a tombstone for the probe chain.
    pub(crate) fn delete_region(&self, pool: PoolRef, region: RegionRef) {
        region.set_p(usize::MAX);
        region.set_size(0);
        pool.set_regions_free(pool.regions_free().saturating_add(1));
    }

    /// Map a page run, using the pool cache first.
    pub(crate) fn map_pages(&self, pool: PoolRef, size: usize, zero: bool) -> usize {
        let pages = size >> crate::consts::PAGE_SHIFT;
        if pages == 0 || size != align_up(size, PAGE_SIZE).unwrap_or(0) {
            self.error(pool, b"malloc", "map round");
        }
        let cache_len = pool.cache_count();
        let mut index = 0usize;
        while index < cache_len {
            let entry = pool.cache(index);
            if entry.pages as usize == pages {
                let page = entry.page;
                let last = cache_len.wrapping_sub(1);
                if index != last {
                    pool.set_cache(index, pool.cache(last));
                }
                pool.set_cache(last, CacheEntry::default());
                pool.set_cache_count(last);
                pool.set_cache_used(pool.cache_used().saturating_sub(pages));
                if zero {
                    // SAFETY: the cached run is mapped for `size` bytes.
                    raw::fill(page, 0, size);
                } else if self.options().malloc_freeunmap {
                    let _ = self
                        .provider
                        .protect(page, size, crate::map_prot::READ_WRITE);
                    // SAFETY: the cached run is writable again.
                    raw::fill(page, JUNK_ALLOC, size);
                }
                return page;
            }
            index = index.wrapping_add(1);
        }
        let Some(page) = self
            .provider
            .map_pages(size, PAGE_SIZE, crate::map_prot::READ_WRITE)
        else {
            return 0;
        };
        pool.set_mapped(pool.mapped().saturating_add(size));
        if !zero && !self.options().malloc_freeunmap {
            // SAFETY: provider.map_pages returned a writable run of `size` bytes.
            raw::fill(page, JUNK_ALLOC, size);
        }
        page
    }

    /// Release a page run, either into the pool cache or to the provider.
    pub(crate) fn unmap_pages(&self, pool: PoolRef, page: usize, size: usize, clear: usize) {
        let pages = size >> crate::consts::PAGE_SHIFT;
        if pages == 0 || size != align_up(size, PAGE_SIZE).unwrap_or(0) {
            self.error(pool, b"free", "munmap round");
        }
        if clear != 0 {
            // SAFETY: `clear` is bounded by the live allocation.
            raw::fill_volatile(page, 0, clear);
        }
        if pool.cache_count() < self.options().def_maxcache
            && pages <= crate::hardened::consts::MAX_CACHE
        {
            if self.options().malloc_freeunmap {
                let _ = self.provider.protect(page, size, crate::map_prot::NONE);
            } else {
                // SAFETY: the run is mapped and writable.
                raw::fill(page, JUNK_FREE, size);
            }
            let index = pool.cache_count();
            pool.set_cache(
                index,
                CacheEntry {
                    page,
                    pages: pages as u32,
                },
            );
            pool.set_cache_count(index.wrapping_add(1));
            pool.set_cache_used(pool.cache_used().saturating_add(pages));
            return;
        }
        if self.options().malloc_freeunmap {
            let _ = self.provider.protect(page, size, crate::map_prot::NONE);
        }
        if self.provider.unmap(page, size) {
            pool.set_mapped(pool.mapped().saturating_sub(size));
        } else {
            self.error(pool, b"free", "munmap");
        }
    }

    /// Allocate one chunk metadata record.
    pub(crate) fn alloc_chunk_info(&self, pool: PoolRef) -> Option<ChunkInfoRef> {
        let mut head = pool.chunk_info_free();
        if head == 0 {
            if pool.chunk_pages() == 0 || pool.chunk_pages_used() >= CHUNK_PAGES {
                let batch = self.provider.map_pages(
                    CHUNK_PAGES * PAGE_SIZE,
                    PAGE_SIZE,
                    crate::map_prot::READ_WRITE,
                )?;
                pool.set_chunk_pages(batch);
                pool.set_chunk_pages_used(0);
                pool.set_mapped(
                    pool.mapped()
                        .saturating_add(CHUNK_PAGES.saturating_mul(PAGE_SIZE)),
                );
            }
            let page = pool
                .chunk_pages()
                .wrapping_add(pool.chunk_pages_used().wrapping_mul(PAGE_SIZE));
            pool.set_chunk_pages_used(pool.chunk_pages_used().wrapping_add(1));
            let per_page = PAGE_SIZE / CHUNK_INFO_SIZE;
            let mut index = 0usize;
            while index < per_page {
                let address = page.wrapping_add(index.wrapping_mul(CHUNK_INFO_SIZE));
                let info = ChunkInfoRef::new(address);
                info.set_next(pool.chunk_info_free());
                pool.set_chunk_info_free(address);
                index = index.wrapping_add(1);
            }
            head = pool.chunk_info_free();
        }
        if head == 0 {
            return None;
        }
        let info = ChunkInfoRef::new(head);
        pool.set_chunk_info_free(info.next());
        info.set_next(0);
        Some(info)
    }

    /// Return a chunk metadata record to the free list.
    pub(crate) fn free_chunk_info(&self, pool: PoolRef, info: ChunkInfoRef) {
        info.set_next(pool.chunk_info_free());
        pool.set_chunk_info_free(info.address());
    }

    /// Map one chunk page and register it.
    pub(crate) fn make_chunks(&self, pool: PoolRef, bucket: usize) -> Option<ChunkInfoRef> {
        let page = self.map_pages(pool, PAGE_SIZE, true);
        if page == 0 {
            return None;
        }
        let Some(info) = self.alloc_chunk_info(pool) else {
            self.unmap_pages(pool, page, PAGE_SIZE, 0);
            return None;
        };
        info.initialize(page, bucket, pool.canary1());
        if !self.insert_region(pool, page | (bucket.wrapping_add(1)), info.address()) {
            self.unmap_pages(pool, page, PAGE_SIZE, 0);
            self.free_chunk_info(pool, info);
            return None;
        }
        let head = pool.chunk_dir(bucket);
        info.set_next(head);
        pool.set_chunk_dir(bucket, info.address());
        if bucket > 0 && pool.malloc_junk() != 0 {
            // SAFETY: the newly mapped page is writable for PAGE_SIZE bytes.
            raw::fill(page, JUNK_FREE, PAGE_SIZE);
        }
        Some(info)
    }

    /// Initialise a pool's mutable state.
    pub(crate) fn initialize_pool(&self, pool: PoolRef, index: usize, junk: i32) {
        pool.set_canary1(self.secret() ^ (pool.address() as u32));
        pool.set_canary2(!pool.canary1());
        pool.set_active(0);
        pool.set_mutex(index as u32);
        pool.set_malloc_junk(junk);
        pool.set_regions(0);
        pool.set_regions_total(0);
        pool.set_regions_free(0);
        pool.set_chunk_pages(0);
        pool.set_chunk_pages_used(0);
        pool.set_chunk_info_free(0);
        pool.set_rbytesused(32);
        pool.set_junk_loc(self.junk_location());
        pool.set_cache_count(0);
        pool.set_cache_used(0);
        pool.set_mapped(0);
        pool.set_in_use(0);
        pool.set_page_in_use(0);
        pool.set_page_count(0);
        let mut bucket = 0usize;
        while bucket < crate::hardened::consts::BUCKETS {
            pool.set_chunk_dir(bucket, 0);
            bucket = bucket.wrapping_add(1);
        }
        let mut slot = 0usize;
        while slot < crate::hardened::consts::DELAYED {
            pool.set_delayed(slot, 0);
            slot = slot.wrapping_add(1);
        }
    }
}

#[cfg(test)]
#[path = "pages_ut.rs"]
mod tests;
