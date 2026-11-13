// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The picolibc-derived tiny backend.
//!
//! One address-ordered singly linked free list, first fit, `map_exact`/`map_pages` growth and a
//! zeroing `malloc` (A-malloc-4). The page provider is the only source of memory; there is no
//! `sbrk` (T-2).

pub mod calloc;
pub mod chunk;
pub mod config;
pub mod error;
pub mod free;
pub mod info;
pub mod malloc;
pub mod memalign;
pub mod posix_memalign;
pub mod realloc;
pub mod sized;
pub mod usable_size;
pub mod valloc;

use crate::compat::checks::array_size;
use crate::compat::{Heap, Mallinfo2};
use crate::consts::{MAX_ALLOC, align_up};
use crate::provider::{Lock, PageProvider, Platform};
use crate::raw::{self, SyncCell};
use crate::tiny::chunk::{
    ALLOC_MAX, CHUNK_MIN, HEAD_SIZE, blob_to_chunk, chunk_after, chunk_size, chunk_to_ptr, is_free,
    mark_busy, mark_free, ptr_to_chunk, set_size, size, usable,
};
use crate::tiny::config::{PAGE_ALIGN, TINY_GROW_MIN};

/// A tiny heap over one page provider and one lock.
pub struct Tiny<P: PageProvider, L: Lock, X: Platform> {
    provider: P,
    lock: L,
    platform: X,
    free_list: SyncCell<usize>,
    sbrk_start: SyncCell<usize>,
    sbrk_top: SyncCell<usize>,
}

impl<P: PageProvider, L: Lock, X: Platform> Tiny<P, L, X> {
    /// Create an empty tiny heap.
    pub const fn new(provider: P, lock: L, platform: X) -> Self {
        Self {
            provider,
            lock,
            platform,
            free_list: SyncCell::new(0),
            sbrk_start: SyncCell::new(0),
            sbrk_top: SyncCell::new(0),
        }
    }

    /// The platform seam, for the debug wrapper.
    #[must_use]
    pub const fn platform(&self) -> &X {
        &self.platform
    }

    fn fail(&self, errno: i32) -> usize {
        self.platform.set_errno(errno);
        0
    }

    /// Map a new region, preferring to extend the top.
    fn grow(&self, need: usize) -> Option<(usize, usize)> {
        let with_header = need.saturating_add(crate::tiny::config::MALLOC_CHUNK_ALIGN);
        let minimum = if with_header > TINY_GROW_MIN {
            with_header
        } else {
            TINY_GROW_MIN
        };
        let len = align_up(minimum, PAGE_ALIGN)?;
        let top = self.sbrk_top.get();
        if top != 0
            && let Some(address) = self.provider.map_exact(top, len)
        {
            if address == top {
                self.sbrk_top.set(top.wrapping_add(len));
                return Some((address, len));
            }
            let _ = self.provider.unmap(address, len);
        }
        let address = self
            .provider
            .map_pages(len, PAGE_ALIGN, crate::map_prot::READ_WRITE)?;
        if self.sbrk_start.get() == 0 {
            self.sbrk_start.set(address);
        }
        self.sbrk_top.set(address.wrapping_add(len));
        Some((address, len))
    }

    /// Insert a free chunk in address order and coalesce with its neighbours.
    fn insert_free(&self, chunk: usize) {
        mark_free(chunk);
        let mut previous = 0usize;
        let mut current = self.free_list.get();
        while current != 0 && current < chunk {
            previous = current;
            // SAFETY: a free chunk stores its next pointer in its first word.
            current = raw::read_usize(current);
        }
        // SAFETY: the chunk is free and its first word is writable.
        raw::write_usize(chunk, current);
        if previous == 0 {
            self.free_list.set(chunk);
        } else {
            // SAFETY: the previous chunk is free and writable.
            raw::write_usize(previous, chunk);
        }
        if current != 0 && chunk_after(chunk) == current {
            set_size(chunk, size(chunk).saturating_add(size(current)));
            mark_free(chunk);
            // SAFETY: both chunks are in the free list.
            let next = raw::read_usize(current);
            // SAFETY: the merged chunk is writable.
            raw::write_usize(chunk, next);
        }
        if previous != 0 && chunk_after(previous) == chunk {
            set_size(previous, size(previous).saturating_add(size(chunk)));
            mark_free(previous);
            // SAFETY: both chunks are in the free list.
            let next = raw::read_usize(chunk);
            // SAFETY: the merged chunk is writable.
            raw::write_usize(previous, next);
        }
    }

    /// Allocate from the free list or map a fresh region.
    fn malloc_chunk(&self, request: usize) -> usize {
        let Some(alloc) = chunk_size(request) else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        if alloc > ALLOC_MAX {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        self.lock.lock(0);
        let mut previous = 0usize;
        let mut current = self.free_list.get();
        while current != 0 {
            // SAFETY: the free list holds readable chunk pointers.
            let next = raw::read_usize(current);
            let current_size = size(current);
            if current_size >= alloc {
                let remainder = current_size - alloc;
                if remainder >= CHUNK_MIN {
                    set_size(current, alloc);
                    let tail = current.wrapping_add(alloc);
                    set_size(tail, remainder);
                    if previous == 0 {
                        self.free_list.set(tail);
                    } else {
                        // SAFETY: `previous` is a free chunk.
                        raw::write_usize(previous, tail);
                    }
                    // SAFETY: the tail chunk is free.
                    raw::write_usize(tail, next);
                } else if previous == 0 {
                    self.free_list.set(next);
                } else {
                    // SAFETY: `previous` is a free chunk.
                    raw::write_usize(previous, next);
                }
                mark_busy(current);
                self.lock.unlock(0);
                self.zero(current);
                return chunk_to_ptr(current);
            }
            previous = current;
            current = next;
        }
        let Some((base, total)) = self.grow(alloc) else {
            self.lock.unlock(0);
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        let chunk = blob_to_chunk(base);
        let available = total.saturating_sub(chunk.saturating_sub(base));
        if available < alloc {
            self.lock.unlock(0);
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        set_size(chunk, alloc);
        if available > alloc {
            let rest = chunk.wrapping_add(alloc);
            set_size(rest, available - alloc);
            self.insert_free(rest);
        }
        mark_busy(chunk);
        self.lock.unlock(0);
        self.zero(chunk);
        chunk_to_ptr(chunk)
    }

    fn zero(&self, chunk: usize) {
        let length = size(chunk).saturating_sub(HEAD_SIZE);
        if length != 0 {
            // SAFETY: the payload of a live chunk is writable.
            raw::fill(chunk_to_ptr(chunk), 0, length);
        }
    }

    fn free_chunk(&self, pointer: usize) {
        if pointer == 0 {
            return;
        }
        if !pointer.is_multiple_of(config_align()) {
            self.invalid_free(pointer);
            return;
        }
        let chunk = ptr_to_chunk(pointer);
        self.lock.lock(0);
        if is_free(chunk) {
            self.lock.unlock(0);
            self.invalid_free(pointer);
            return;
        }
        let start = self.sbrk_start.get();
        let top = self.sbrk_top.get();
        if start != 0 && (pointer < start || pointer >= top) {
            self.lock.unlock(0);
            self.invalid_free(pointer);
            return;
        }
        set_size(chunk, size(chunk));
        self.insert_free(chunk);
        self.lock.unlock(0);
    }

    fn invalid_free(&self, pointer: usize) {
        #[cfg(feature = "debug")]
        {
            let mut message = crate::errors::Message::new();
            message.push_str("free: double free or bogus pointer ");
            message.push_ptr(pointer);
            crate::errors::wrterror(&self.platform, b"free", &message);
        }
        #[cfg(not(feature = "debug"))]
        {
            let _ = pointer;
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
    }

    fn realloc_chunk(&self, pointer: usize, request: usize) -> usize {
        if pointer == 0 {
            return self.malloc_chunk(request);
        }
        if request == 0 {
            self.free_chunk(pointer);
            return 0;
        }
        if request > ALLOC_MAX {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let chunk = ptr_to_chunk(pointer);
        if is_free(chunk) {
            self.invalid_free(pointer);
            return 0;
        }
        let old_usable = size(chunk).saturating_sub(HEAD_SIZE);
        let fresh = self.malloc_chunk(request);
        if fresh == 0 {
            return 0;
        }
        let copy = if request < old_usable {
            request
        } else {
            old_usable
        };
        // SAFETY: both allocations are live and the copy is bounded by both capacities.
        raw::copy(fresh, pointer, copy);
        self.free_chunk(pointer);
        fresh
    }

    fn memalign_chunk(&self, align: usize, request: usize) -> usize {
        if align == 0 || !align.is_power_of_two() {
            return self.fail(raven_libc_abi::errno::EINVAL);
        }
        if align > MAX_ALLOC || request > MAX_ALLOC - align {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let align = if align < config_align() {
            config_align()
        } else {
            align
        };
        let Some(chunk_bytes) = chunk_size(request) else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        let Some(padding) = chunk_bytes
            .checked_add(align)
            .and_then(|value| value.checked_add(CHUNK_MIN))
        else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        let pointer = self.malloc_chunk(padding);
        if pointer == 0 {
            return 0;
        }
        let original = ptr_to_chunk(pointer);
        let mut aligned = match align_up(pointer, align) {
            Some(value) => value,
            None => {
                self.free_chunk(pointer);
                return self.fail(raven_libc_abi::errno::ENOMEM);
            }
        };
        let mut offset = aligned - pointer;
        if offset != 0 {
            if offset < CHUNK_MIN {
                aligned = aligned.wrapping_add(align);
                offset = aligned - pointer;
            }
            let new_chunk = ptr_to_chunk(aligned);
            set_size(new_chunk, size(original).saturating_sub(offset));
            // The front padding stays reserved: returning it to the free list here would
            // require a header immediately before `new_chunk`, and a later coalesce could
            // swallow the aligned chunk. The waste is bounded by the requested alignment.
            let back = size(new_chunk).saturating_sub(chunk_bytes);
            if back >= CHUNK_MIN {
                set_size(new_chunk, chunk_bytes);
                let tail = new_chunk.wrapping_add(chunk_bytes);
                set_size(tail, back);
                self.lock.lock(0);
                self.insert_free(tail);
                self.lock.unlock(0);
            }
            return chunk_to_ptr(new_chunk);
        }
        let back = size(original).saturating_sub(chunk_bytes);
        if back >= CHUNK_MIN {
            set_size(original, chunk_bytes);
            let tail = original.wrapping_add(chunk_bytes);
            set_size(tail, back);
            self.lock.lock(0);
            self.insert_free(tail);
            self.lock.unlock(0);
        }
        pointer
    }

    fn calloc_chunk(&self, n: usize, m: usize) -> usize {
        let Some(bytes) = array_size(n, m) else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        self.malloc_chunk(bytes)
    }

    fn usable_chunk(&self, pointer: usize) -> usize {
        if pointer == 0 {
            return 0;
        }
        usable(ptr_to_chunk(pointer))
    }

    fn info_chunk(&self) -> Mallinfo2 {
        self.lock.lock(0);
        let mut free_bytes = 0usize;
        let mut free_blocks = 0usize;
        let mut current = self.free_list.get();
        while current != 0 {
            free_blocks = free_blocks.wrapping_add(1);
            free_bytes = free_bytes.saturating_add(size(current));
            // SAFETY: free-list traversal.
            current = raw::read_usize(current);
        }
        self.lock.unlock(0);
        let start = self.sbrk_start.get();
        let top = self.sbrk_top.get();
        let arena = if start == 0 || top < start {
            0
        } else {
            top - start
        };
        Mallinfo2 {
            arena,
            ordblks: free_blocks,
            fordblks: free_bytes,
            uordblks: arena.saturating_sub(free_bytes),
            ..Mallinfo2::default()
        }
    }
}

fn config_align() -> usize {
    crate::tiny::config::MALLOC_CHUNK_ALIGN
}

impl<P: PageProvider, L: Lock, X: Platform> Heap for Tiny<P, L, X> {
    fn malloc(&self, size: usize) -> usize {
        self.malloc_chunk(size)
    }

    fn free(&self, p: usize) {
        self.free_chunk(p);
    }

    fn realloc(&self, p: usize, size: usize) -> usize {
        self.realloc_chunk(p, size)
    }

    fn calloc(&self, n: usize, m: usize) -> usize {
        self.calloc_chunk(n, m)
    }

    fn memalign(&self, align: usize, size: usize) -> usize {
        self.memalign_chunk(align, size)
    }

    fn usable_size(&self, p: usize) -> usize {
        self.usable_chunk(p)
    }

    fn trim(&self) -> bool {
        false
    }

    fn info(&self) -> Mallinfo2 {
        self.info_chunk()
    }

    fn free_sized(&self, p: usize, _size: usize) {
        self.free_chunk(p);
    }

    fn free_aligned_sized(&self, p: usize, _align: usize, _size: usize) {
        self.free_chunk(p);
    }

    fn prefork(&self) {
        self.lock.lock(0);
    }

    fn postfork_parent(&self) {
        self.lock.unlock(0);
    }

    fn postfork_child(&self) {
        self.lock.reset(0);
    }
}

#[cfg(test)]
#[path = "tiny_ut.rs"]
mod tests;
