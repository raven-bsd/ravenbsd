// SPDX-License-Identifier: ISC
// Copyright (c) 2008, 2010, 2011, 2016, 2023 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2012 Matthew Dempsky <matthew@openbsd.org>
// Copyright (c) 2008 Damien Miller <djm@openbsd.org>
// Copyright (c) 2000 Poul-Henning Kamp <phk@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: malloc.c,v 1.302 2026/11/04 12:37:25 otto Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// If we meet some day, and you think this stuff is worth it, you
// can buy me a beer in return. Poul-Henning Kamp

//! The OpenBSD-derived hardened backend.
//!
//! The entry points keep the donor's names (`omalloc`, `ofree`, `orealloc`, `omemalign`,
//! `malloc_bytes`, `find_chunksize`, `map`, `unmap`, `getrbyte`, `wrterror`) and the structure
//! facts of `openbsd:lib/libc/stdlib/malloc.c`: 4 KiB pages, 16-byte chunks, 2048-byte largest
//! chunk, delayed free, junk, optional canaries and guard pages, pools and cross-pool free.
//!
//! `MALLOC_STATS` is deliberately not translated (A-malloc-16).

pub mod chunks;
pub mod consts;
pub mod opts;
pub mod pages;
pub mod pool;
pub mod stats;

use crate::compat::options::Options;
use crate::compat::{Heap, Mallinfo2};
use crate::consts::{MAX_ALLOC, PAGE_SIZE, align_up};
use crate::errors::{Message, wrterror};
use crate::hardened::consts::{DEFAULT_POOLS, JUNK_ALLOC, MAX_CHUNK, MAX_POOLS, bucket_bytes};
use crate::hardened::pool::{ChunkInfoRef, PoolRef, RegionRef};
use crate::provider::{Entropy, Lock, PageProvider, Platform};
use crate::raw::{self, SyncCell};
use crate::tiny::config::MALLOC_CHUNK_ALIGN;

/// The hardened heap.
pub struct Hardened<P: PageProvider, L: Lock, E: Entropy, X: Platform> {
    provider: P,
    lock: L,
    entropy: E,
    platform: X,
    options: SyncCell<Options>,
    build: SyncCell<[u8; 64]>,
    build_len: SyncCell<usize>,
    pools: SyncCell<[usize; MAX_POOLS]>,
    pool_count: SyncCell<usize>,
    initialized: SyncCell<bool>,
    secret: SyncCell<u32>,
    junk_loc: SyncCell<u32>,
    canary: SyncCell<u8>,
}

impl<P: PageProvider, L: Lock, E: Entropy, X: Platform> Hardened<P, L, E, X> {
    /// Create a hardened heap with a build-time option string.
    pub fn new(provider: P, lock: L, entropy: E, platform: X, build_options: &[u8]) -> Self {
        let mut build = [0u8; 64];
        let length = build_options.len().min(64);
        let mut index = 0usize;
        while let (Some(slot), Some(&byte)) = (build.get_mut(index), build_options.get(index)) {
            *slot = byte;
            index = index.wrapping_add(1);
        }
        Self {
            provider,
            lock,
            entropy,
            platform,
            options: SyncCell::new(Options {
                malloc_mutexes: DEFAULT_POOLS,
                ..Options::default()
            }),
            build: SyncCell::new(build),
            build_len: SyncCell::new(length),
            pools: SyncCell::new([0; MAX_POOLS]),
            pool_count: SyncCell::new(0),
            initialized: SyncCell::new(false),
            secret: SyncCell::new(1),
            junk_loc: SyncCell::new(0),
            canary: SyncCell::new(JUNK_ALLOC.wrapping_add(1)),
        }
    }

    /// The platform seam.
    #[must_use]
    pub const fn platform(&self) -> &X {
        &self.platform
    }

    pub(crate) fn options(&self) -> Options {
        self.options.get()
    }

    pub(crate) fn secret(&self) -> u32 {
        self.secret.get()
    }

    pub(crate) fn junk_location(&self) -> u32 {
        self.junk_loc.get()
    }

    pub(crate) fn chunk_canary(&self) -> u8 {
        self.canary.get()
    }

    fn build_string(&self) -> [u8; 64] {
        self.build.get()
    }

    fn build_length(&self) -> usize {
        self.build_len.get()
    }

    /// Initialise the secret, pools and option page once.
    fn init_once(&self) -> bool {
        if self.initialized.get() {
            return true;
        }
        self.lock.lock(1);
        if self.initialized.get() {
            self.lock.unlock(1);
            return true;
        }
        if self.provider.page_size() != PAGE_SIZE {
            self.error_no_pool(b"malloc", "malloc: page size");
        }
        let build = self.build_string();
        let options = opts::parse(
            &self.platform,
            build.get(..self.build_length()).unwrap_or(&[]),
        );
        self.options.set(options);
        let count = opts::pool_count(&options);
        let mut bytes = [0u8; 32];
        if !self.entropy.fill(&mut bytes) {
            self.error_no_pool(b"malloc", "malloc: no entropy");
        }
        let mut secret = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        while secret == 0 {
            if !self.entropy.fill(&mut bytes[4..8]) {
                self.error_no_pool(b"malloc", "malloc: no entropy");
            }
            secret = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        }
        self.secret.set(secret);
        self.junk_loc.set(u32::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11],
        ]));
        let mut canary = bytes[12];
        while canary == 0 || canary == JUNK_ALLOC || canary == consts::JUNK_FREE {
            if !self.entropy.fill(&mut bytes[12..13]) {
                self.error_no_pool(b"malloc", "malloc: no entropy");
            }
            canary = bytes[12];
        }
        self.canary.set(canary);
        let pool_size = core::mem::size_of::<pool::Pool>();
        let Some(body) = pool_size
            .checked_mul(count)
            .and_then(|v| align_up(v, PAGE_SIZE))
        else {
            self.error_no_pool(b"malloc", "malloc_init mmap1 failed");
        };
        let Some(total) = body.checked_add(2 * PAGE_SIZE) else {
            self.error_no_pool(b"malloc", "malloc_init mmap1 failed");
        };
        let Some(base) = self
            .provider
            .map_pages(total, PAGE_SIZE, crate::map_prot::READ_WRITE)
        else {
            self.error_no_pool(b"malloc", "malloc_init mmap1 failed");
        };
        if !self
            .provider
            .protect(base, PAGE_SIZE, crate::map_prot::NONE)
            || !self.provider.protect(
                base.wrapping_add(PAGE_SIZE).wrapping_add(body),
                PAGE_SIZE,
                crate::map_prot::NONE,
            )
        {
            self.error_no_pool(b"malloc", "malloc_init mprotect1 failed");
        }
        let first = base.wrapping_add(PAGE_SIZE);
        let mut pools = [0usize; MAX_POOLS];
        let mut index = 0usize;
        while index < count {
            let address = first.wrapping_add(index.wrapping_mul(pool_size));
            if let Some(slot) = pools.get_mut(index) {
                *slot = address;
            }
            let pool = PoolRef::new(address);
            self.initialize_pool(pool, index, options.def_malloc_junk as i32);
            index = index.wrapping_add(1);
        }
        self.pools.set(pools);
        self.pool_count.set(count);
        self.initialized.set(true);
        self.lock.unlock(1);
        true
    }

    fn pool_ref(&self, index: usize) -> PoolRef {
        PoolRef::new(self.pools.get().get(index).copied().unwrap_or(0))
    }

    fn home_pool(&self) -> PoolRef {
        let count = self.pool_count.get().max(2);
        let index = 1usize.wrapping_add(self.platform.thread_id() % count.wrapping_sub(1));
        self.pool_ref(index)
    }

    /// Find the pool that owns `pointer` and return it with its lock held.
    fn locate(&self, pointer: usize) -> (PoolRef, RegionRef) {
        let home = self.home_pool();
        self.lock.lock(home.mutex() as usize);
        if let Some(region) = self.find_region(home, pointer) {
            return (home, region);
        }
        self.lock.unlock(home.mutex() as usize);
        let count = self.pool_count.get();
        let mut index = 0usize;
        while index < count {
            let pool = self.pool_ref(index);
            if pool.address() != home.address() {
                self.lock.lock(pool.mutex() as usize);
                if let Some(region) = self.find_region(pool, pointer) {
                    return (pool, region);
                }
                self.lock.unlock(pool.mutex() as usize);
            }
            index = index.wrapping_add(1);
        }
        self.error_ptr(home, b"free", "bogus pointer (double free?)", pointer);
    }

    /// Allocate from one locked pool.
    fn omalloc(&self, pool: PoolRef, size: usize, zero_fill: bool) -> usize {
        if size > MAX_CHUNK {
            let guard = self.options().malloc_guard;
            let Some(total) = size.checked_add(guard) else {
                return 0;
            };
            if total >= usize::MAX - PAGE_SIZE {
                return 0;
            }
            let Some(page_size) = align_up(total, PAGE_SIZE) else {
                return 0;
            };
            let pointer = self.map_pages(pool, page_size, zero_fill);
            if pointer == 0 {
                return 0;
            }
            if !self.insert_region(pool, pointer, total) {
                self.unmap_pages(pool, pointer, page_size, 0);
                return 0;
            }
            if guard != 0
                && !self.provider.protect(
                    pointer.wrapping_add(page_size).wrapping_sub(guard),
                    guard,
                    crate::map_prot::NONE,
                )
            {
                self.error(pool, b"malloc", "mprotect");
            }
            if self.options().chunk_canaries && size > 0 {
                self.fill_canary(pointer, size, page_size.saturating_sub(guard));
            }
            pool.set_in_use(pool.in_use().saturating_add(total));
            pool.set_page_in_use(pool.page_in_use().saturating_add(total));
            pool.set_page_count(pool.page_count().saturating_add(1));
            return pointer;
        }
        let pointer = self.malloc_bytes(pool, size);
        if pointer != 0 {
            pool.set_in_use(
                pool.in_use()
                    .saturating_add(bucket_bytes(crate::hardened::consts::find_bucket(size))),
            );
            if zero_fill && size > 0 {
                // SAFETY: the chunk is live and `size` bytes are writable.
                raw::fill(pointer, 0, size);
            }
        }
        pointer
    }

    /// Free one pointer, returning whether it belonged to this pool.
    fn ofree(&self, pool: PoolRef, pointer: usize, clear: bool, check: bool, size: usize) -> bool {
        let before = self.find_region(pool, pointer);
        let old_usable = before.map(Self::region_size).unwrap_or(0);
        let result = self.free_pointer(pool, pointer, clear, check, size);
        if result {
            pool.set_in_use(pool.in_use().saturating_sub(old_usable));
        }
        result
    }

    /// `realloc` over pointee size.
    fn orealloc(&self, pool: PoolRef, pointer: usize, size: usize) -> usize {
        let Some(region) = self.find_region(pool, pointer) else {
            return 0;
        };
        let real = Self::region_size(region);
        let guard = self.options().malloc_guard;
        let usable = if Self::is_page(region) {
            real.saturating_sub(guard)
        } else if self.options().chunk_canaries {
            let info = ChunkInfoRef::new(region.size());
            let chunknum = (pointer & (PAGE_SIZE - 1)) / bucket_bytes(info.bucket() as usize);
            let stored = usize::from(info.size_at(chunknum));
            if stored == 0 { real } else { stored }
        } else {
            real
        };
        if size <= usable && !self.options().malloc_realloc {
            if !Self::is_page(region) && self.options().chunk_canaries && size > 0 {
                let info = ChunkInfoRef::new(region.size());
                let chunk_bytes = bucket_bytes(info.bucket() as usize);
                let chunknum = (pointer & (PAGE_SIZE - 1)) / chunk_bytes;
                info.set_size_at(chunknum, size as u16);
                self.fill_canary(pointer, size, chunk_bytes);
            }
            return pointer;
        }
        let fresh = self.omalloc(pool, size, false);
        if fresh == 0 {
            return 0;
        }
        let copy = if size < usable { size } else { usable };
        // SAFETY: both allocations are live and the copy is bounded.
        raw::copy(fresh, pointer, copy);
        let _ = self.ofree(pool, pointer, false, false, 0);
        fresh
    }

    /// `memalign` over one locked pool.
    fn omemalign(&self, pool: PoolRef, alignment: usize, size: usize) -> usize {
        if alignment == 0 || !alignment.is_power_of_two() {
            return 0;
        }
        if alignment <= PAGE_SIZE {
            let minimum = size.max(alignment).max(MALLOC_CHUNK_ALIGN);
            let Some(rounded) = minimum.checked_next_power_of_two() else {
                return 0;
            };
            return self.omalloc(pool, rounded.max(size), false);
        }
        if alignment > usize::MAX - size {
            return 0;
        }
        let size = size.max(PAGE_SIZE);
        let guard = self.options().malloc_guard;
        let Some(body) = size
            .checked_add(guard)
            .and_then(|v| v.checked_add(alignment))
        else {
            return 0;
        };
        let Some(total) = align_up(body, PAGE_SIZE) else {
            return 0;
        };
        let Some(base) = self
            .provider
            .map_pages(total, PAGE_SIZE, crate::map_prot::READ_WRITE)
        else {
            return 0;
        };
        let Some(aligned) = align_up(base, alignment) else {
            let _ = self.provider.unmap(base, total);
            return 0;
        };
        let head = aligned.wrapping_sub(base);
        let tail = total
            .wrapping_sub(head)
            .wrapping_sub(size.saturating_add(guard));
        if head != 0 {
            let _ = self.provider.unmap(base, head);
        }
        if tail != 0 {
            let _ = self
                .provider
                .unmap(aligned.wrapping_add(size).wrapping_add(guard), tail);
        }
        if !self.insert_region(pool, aligned, size.saturating_add(guard)) {
            let _ = self.provider.unmap(
                aligned,
                align_up(size.saturating_add(guard), PAGE_SIZE).unwrap_or(PAGE_SIZE),
            );
            return 0;
        }
        if guard != 0
            && !self.provider.protect(
                aligned.wrapping_add(PAGE_SIZE).wrapping_sub(guard),
                guard,
                crate::map_prot::NONE,
            )
        {
            self.error(pool, b"malloc", "mprotect");
        }
        pool.set_in_use(pool.in_use().saturating_add(size.saturating_add(guard)));
        pool.set_page_in_use(
            pool.page_in_use()
                .saturating_add(size.saturating_add(guard)),
        );
        pool.set_page_count(pool.page_count().saturating_add(1));
        aligned
    }

    /// `malloc_conceal`: pool 0 with full junk and no cache.
    pub fn malloc_conceal(&self, size: usize) -> usize {
        if !self.init_once() {
            return 0;
        }
        let pool = self.pool_ref(0);
        self.lock.lock(pool.mutex() as usize);
        let result = self.omalloc(pool, size, false);
        self.lock.unlock(pool.mutex() as usize);
        result
    }

    /// `freezero`: zero the requested bytes, verify the recorded size and free.
    pub fn freezero(&self, pointer: usize, size: usize) {
        if pointer == 0 {
            return;
        }
        if !self.init_once() {
            return;
        }
        let (pool, _region) = self.locate(pointer);
        let _ = self.ofree(pool, pointer, true, true, size);
        self.lock.unlock(pool.mutex() as usize);
    }

    /// `calloc_conceal`.
    pub fn calloc_conceal(&self, n: usize, m: usize) -> usize {
        let Some(size) = crate::compat::checks::array_size(n, m) else {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        };
        if !self.init_once() {
            return 0;
        }
        let pool = self.pool_ref(0);
        self.lock.lock(pool.mutex() as usize);
        let result = self.omalloc(pool, size, true);
        self.lock.unlock(pool.mutex() as usize);
        result
    }

    fn trim_pool(&self, pool: PoolRef) -> bool {
        self.lock.lock(pool.mutex() as usize);
        let count = pool.cache_count();
        let mut released = false;
        let mut index = 0usize;
        while index < count {
            let entry = pool.cache(index);
            let size = (entry.pages as usize).wrapping_mul(PAGE_SIZE);
            if self.provider.unmap(entry.page, size) {
                pool.set_mapped(pool.mapped().saturating_sub(size));
                released = true;
            }
            index = index.wrapping_add(1);
        }
        pool.set_cache_count(0);
        pool.set_cache_used(0);
        self.lock.unlock(pool.mutex() as usize);
        released
    }

    fn info_pool(&self, pool: PoolRef, info: &mut Mallinfo2) {
        self.lock.lock(pool.mutex() as usize);
        info.arena = info.arena.saturating_add(pool.mapped());
        info.uordblks = info.uordblks.saturating_add(pool.in_use());
        info.hblkhd = info.hblkhd.saturating_add(pool.page_in_use());
        info.hblks = info.hblks.saturating_add(pool.page_count());
        info.fordblks = info
            .fordblks
            .saturating_add(pool.cache_used().wrapping_mul(PAGE_SIZE));
        info.ordblks = info.ordblks.saturating_add(pool.cache_count());
        let mut slot = 0usize;
        while slot < consts::DELAYED {
            if pool.delayed(slot) != 0 {
                info.ordblks = info.ordblks.saturating_add(1);
            }
            slot = slot.wrapping_add(1);
        }
        self.lock.unlock(pool.mutex() as usize);
    }

    /// Error helper with no pool context.
    pub(crate) fn error_no_pool(&self, function: &[u8], text: &str) -> ! {
        let mut message = Message::new();
        message.push_str(text);
        wrterror(&self.platform, function, &message)
    }

    /// Error helper.
    pub(crate) fn error(&self, _pool: PoolRef, function: &[u8], text: &str) -> ! {
        self.error_no_pool(function, text)
    }

    /// Error with one pointer.
    pub(crate) fn error_ptr(
        &self,
        _pool: PoolRef,
        function: &[u8],
        text: &str,
        pointer: usize,
    ) -> ! {
        let mut message = Message::new();
        message.push_str(text);
        message.push(b' ');
        message.push_ptr(pointer);
        wrterror(&self.platform, function, &message)
    }

    /// Error with two sizes.
    pub(crate) fn error_sizes(
        &self,
        _pool: PoolRef,
        function: &[u8],
        text: &str,
        left: usize,
        right: usize,
    ) -> ! {
        let mut message = Message::new();
        message.push_str(text);
        message.push(b' ');
        message.push_dec(left);
        message.push_str(" < ");
        message.push_dec(right);
        wrterror(&self.platform, function, &message)
    }

    /// Error for a sampled junk word.
    pub(crate) fn error_offset(
        &self,
        _pool: PoolRef,
        function: &[u8],
        text: &str,
        pointer: usize,
        index: usize,
    ) -> ! {
        let mut message = Message::new();
        message.push_str(text);
        message.push(b' ');
        message.push_ptr(pointer);
        message.push(b'[');
        message.push_dec(index.wrapping_mul(8));
        message.push_str("..");
        message.push_dec(index.wrapping_mul(8).wrapping_add(7));
        message.push(b']');
        wrterror(&self.platform, function, &message)
    }

    /// Error for a canary mismatch.
    pub(crate) fn error_canary(
        &self,
        _pool: PoolRef,
        pointer: usize,
        requested: usize,
        allocated: usize,
        offset: usize,
        value: u8,
    ) -> ! {
        let mut message = Message::new();
        message.push_str("canary corrupted ");
        message.push_ptr(pointer);
        message.push(b'[');
        message.push_dec(offset);
        message.push_str("]@");
        message.push_dec(requested);
        message.push(b'/');
        message.push_dec(allocated);
        if value == consts::JUNK_FREE {
            message.push_str(" (double free?)");
        }
        wrterror(&self.platform, b"free", &message)
    }
}

impl<P: PageProvider, L: Lock, E: Entropy, X: Platform> Heap for Hardened<P, L, E, X> {
    fn malloc(&self, size: usize) -> usize {
        if !self.init_once() {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        if size > MAX_ALLOC {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        let pool = self.home_pool();
        self.lock.lock(pool.mutex() as usize);
        let result = self.omalloc(pool, size, false);
        self.lock.unlock(pool.mutex() as usize);
        if result == 0 {
            if self.options().malloc_xmalloc {
                self.error(pool, b"malloc", "out of memory");
            }
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    fn free(&self, p: usize) {
        if p == 0 {
            return;
        }
        if !self.init_once() {
            return;
        }
        let (pool, _region) = self.locate(p);
        let _ = self.ofree(pool, p, false, false, 0);
        self.lock.unlock(pool.mutex() as usize);
    }

    fn realloc(&self, p: usize, size: usize) -> usize {
        if p == 0 {
            return self.malloc(size);
        }
        if size > MAX_ALLOC {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        if !self.init_once() {
            return 0;
        }
        let (pool, _region) = self.locate(p);
        let result = if size == 0 {
            self.omalloc(pool, 1, false)
        } else {
            self.orealloc(pool, p, size)
        };
        self.lock.unlock(pool.mutex() as usize);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    fn calloc(&self, n: usize, m: usize) -> usize {
        let Some(size) = crate::compat::checks::array_size(n, m) else {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        };
        if !self.init_once() {
            return 0;
        }
        let pool = self.home_pool();
        self.lock.lock(pool.mutex() as usize);
        let result = self.omalloc(pool, size, true);
        self.lock.unlock(pool.mutex() as usize);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    fn memalign(&self, align: usize, size: usize) -> usize {
        if !self.init_once() {
            return 0;
        }
        if size > MAX_ALLOC {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            return 0;
        }
        let pool = self.home_pool();
        self.lock.lock(pool.mutex() as usize);
        let result = self.omemalign(pool, align, size);
        self.lock.unlock(pool.mutex() as usize);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    fn usable_size(&self, p: usize) -> usize {
        if p == 0 {
            return 0;
        }
        if !self.init_once() {
            return 0;
        }
        let (pool, region) = self.locate(p);
        let result = if Self::is_page(region) {
            region.size().saturating_sub(self.options().malloc_guard)
        } else {
            let info = ChunkInfoRef::new(region.size());
            let chunk_bytes = bucket_bytes(info.bucket() as usize);
            if self.options().chunk_canaries {
                let chunknum = (p & (PAGE_SIZE - 1)) / chunk_bytes;
                let stored = usize::from(info.size_at(chunknum));
                if stored == 0 { chunk_bytes } else { stored }
            } else {
                chunk_bytes
            }
        };
        self.lock.unlock(pool.mutex() as usize);
        result
    }

    fn trim(&self) -> bool {
        if !self.init_once() {
            return false;
        }
        let count = self.pool_count.get();
        let mut released = false;
        let mut index = 0usize;
        while index < count {
            let pool = self.pool_ref(index);
            if self.trim_pool(pool) {
                released = true;
            }
            index = index.wrapping_add(1);
        }
        released
    }

    fn info(&self) -> Mallinfo2 {
        if !self.init_once() {
            return Mallinfo2::default();
        }
        let mut info = Mallinfo2::default();
        let count = self.pool_count.get();
        let mut index = 0usize;
        while index < count {
            self.info_pool(self.pool_ref(index), &mut info);
            index = index.wrapping_add(1);
        }
        info
    }

    fn free_sized(&self, p: usize, size: usize) {
        if p == 0 {
            return;
        }
        let usable = self.usable_size(p);
        if usable < size {
            self.platform.set_errno(raven_libc_abi::errno::EINVAL);
            let home = self.home_pool();
            self.error_sizes(home, b"free", "recorded size", usable, size);
        }
        self.free(p);
    }

    fn free_aligned_sized(&self, p: usize, _align: usize, size: usize) {
        self.free_sized(p, size);
    }

    fn prefork(&self) {
        if !self.init_once() {
            return;
        }
        let count = self.pool_count.get();
        let mut index = 0usize;
        while index < count {
            self.lock.lock(index);
            index = index.wrapping_add(1);
        }
    }

    fn postfork_parent(&self) {
        let count = self.pool_count.get();
        let mut index = count;
        while index != 0 {
            index = index.wrapping_sub(1);
            self.lock.unlock(index);
        }
    }

    fn postfork_child(&self) {
        let count = self.pool_count.get();
        let mut index = 0usize;
        while index < count {
            self.lock.reset(index);
            index = index.wrapping_add(1);
        }
    }
}

#[cfg(test)]
#[path = "hardened_ut.rs"]
mod tests;
