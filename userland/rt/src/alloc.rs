// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Optional bump + first-fit free-list allocator over `PageSource`.
//!
//! The production source maps anonymous memory with `MAP`; host tests use a `Vec`-backed source.
//! The allocator is serialized by a spin lock, as required by `GlobalAlloc`'s `&self` methods.
//!
//! Free blocks are address-ordered and coalesced, and a reused block is split when the remainder
//! can hold another free node. This is the Raven form of the per-size-class reuse contract in
//! NetBSD `lib/libc/stdlib/malloc.c:malloc_bytes`/`free_bytes`: a small request must not consume a
//! large chunk and strand the rest. **Recorded divergence**: Raven is byte-granular over lazily
//! mapped chunks, not paged size classes; split/coalesce preserves the same reuse outcome without
//! copying donor code.

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

use alloc_crate::alloc::GlobalAlloc;
use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};

use crate::sys::{syscall3, syscall6};

/// A source of page-aligned memory for the allocator.
pub trait PageSource {
    /// Map at least `len` bytes and return a page-aligned pointer, or `None` on failure.
    fn map(&mut self, len: usize) -> Option<*mut u8>;
    /// Release a mapping previously returned by [`PageSource::map`].
    fn unmap(&mut self, pointer: *mut u8, len: usize);
}

/// The production source: anonymous `MAP` memory in the current process.
pub struct SysPageSource;

impl PageSource for SysPageSource {
    fn map(&mut self, len: usize) -> Option<*mut u8> {
        let len = len.max(4096).checked_add(4095)? & !4095;
        // SAFETY: MAP takes scalar arguments; the runtime maps anonymous memory into this
        // process and returns the address or `-errno`.
        let result = unsafe {
            syscall6(
                Syscall::MAP as usize,
                Handle::SELF_PROC.0 as usize,
                Handle::NONE.0 as usize,
                0,
                0,
                len,
                map::pack(prot::READ | prot::WRITE, 0),
            )
        };
        Errno::from_ret(result)
            .ok()
            .map(|address| address as *mut u8)
    }

    fn unmap(&mut self, pointer: *mut u8, len: usize) {
        let len = len.max(4096).saturating_add(4095) & !4095;
        // SAFETY: UNMAP undoes a mapping previously returned by `SysPageSource::map`.
        unsafe {
            let _ = syscall3(
                Syscall::UNMAP as usize,
                Handle::SELF_PROC.0 as usize,
                pointer as usize,
                len,
            );
        }
    }
}

#[repr(C)]
struct FreeNode {
    size: usize,
    next: *mut FreeNode,
}

/// Round a request to the allocation alignment and at least one free node.
fn span(size: usize, align: usize) -> Option<usize> {
    let align = align.max(core::mem::align_of::<FreeNode>());
    size.max(1)
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
}

struct State {
    chunk: *mut u8,
    chunk_len: usize,
    chunk_offset: usize,
    free: *mut FreeNode,
}

impl State {
    const fn empty() -> Self {
        Self {
            chunk: ptr::null_mut(),
            chunk_len: 0,
            chunk_offset: 0,
            free: ptr::null_mut(),
        }
    }

    fn alloc(&mut self, source: &mut dyn PageSource, layout: Layout) -> *mut u8 {
        let align = layout.align().max(core::mem::align_of::<FreeNode>());
        let Some(need) = span(layout.size(), align) else {
            return ptr::null_mut();
        };

        let mut previous: *mut FreeNode = ptr::null_mut();
        let mut node = self.free;
        while !node.is_null() {
            // SAFETY: every free-list node was written by `dealloc` and remains in a mapped chunk.
            let current = unsafe { &mut *node };
            if current.size >= need && (node as usize).is_multiple_of(align) {
                let remainder = current.size - need;
                if remainder >= size_of::<FreeNode>() + core::mem::align_of::<FreeNode>() {
                    // Return the tail to the free list at its own size; the next request can use
                    // it without stealing this block's span.
                    let tail = (node as usize).wrapping_add(need) as *mut FreeNode;
                    // SAFETY: the tail is inside the same mapped block, after the allocation and
                    // far enough from the end to hold a `FreeNode`.
                    unsafe {
                        tail.write(FreeNode {
                            size: remainder,
                            next: current.next,
                        });
                        if previous.is_null() {
                            self.free = tail;
                        } else {
                            (*previous).next = tail;
                        }
                    }
                } else if previous.is_null() {
                    self.free = current.next;
                } else {
                    // SAFETY: `previous` is a free-list node.
                    unsafe { (*previous).next = current.next };
                }
                return node as *mut u8;
            }
            previous = node;
            node = current.next;
        }

        let base = self.chunk as usize;
        let Some(start) = base.checked_add(self.chunk_offset) else {
            return ptr::null_mut();
        };
        let Some(aligned) = start
            .checked_add(align - 1)
            .map(|value| value & !(align - 1))
        else {
            return ptr::null_mut();
        };
        let Some(end) = aligned.checked_add(need) else {
            return ptr::null_mut();
        };
        let Some(offset_end) = end.checked_sub(base) else {
            return ptr::null_mut();
        };
        if self.chunk.is_null() || offset_end > self.chunk_len {
            let Some(map_len) = need
                .checked_add(align)
                .and_then(|value| value.checked_add(4095))
                .map(|value| value & !4095)
            else {
                return ptr::null_mut();
            };
            let Some(chunk) = source.map(map_len) else {
                return ptr::null_mut();
            };
            self.chunk = chunk;
            self.chunk_len = map_len;
            self.chunk_offset = 0;
            return self.alloc(source, layout);
        }
        self.chunk_offset = offset_end;
        aligned as *mut u8
    }

    fn dealloc(&mut self, pointer: *mut u8, layout: Layout) {
        if pointer.is_null() {
            return;
        }
        let align = layout.align().max(core::mem::align_of::<FreeNode>());
        let Some(mut need) = span(layout.size(), align) else {
            return;
        };
        let start = pointer as usize;
        let Some(end) = start.checked_add(need) else {
            return;
        };

        let mut previous: *mut FreeNode = ptr::null_mut();
        let mut node = self.free;
        while !node.is_null() && (node as usize) < start {
            previous = node;
            // SAFETY: the free list is address-ordered and every node is live.
            node = unsafe { (*node).next };
        }
        let next_after = if !node.is_null() && end == node as usize {
            // SAFETY: `node` is the address-adjacent successor free node.
            let next = unsafe { (*node).next };
            // SAFETY: `node` is a live free node with a size field.
            need = need.saturating_add(unsafe { (*node).size });
            next
        } else {
            node
        };
        let previous_end = if previous.is_null() {
            0
        } else {
            // SAFETY: `previous` is a live free-list node with a size field.
            (previous as usize).saturating_add(unsafe { (*previous).size })
        };
        if !previous.is_null() && previous_end == start {
            // SAFETY: `previous` is the address-adjacent predecessor; extend it over this block
            // and the already-absorbed successor.
            unsafe {
                (*previous).size = (*previous).size.saturating_add(need);
                (*previous).next = next_after;
            }
        } else {
            let node = pointer.cast::<FreeNode>();
            // SAFETY: the caller hands back a live allocation of at least `need` bytes.
            unsafe {
                node.write(FreeNode {
                    size: need,
                    next: next_after,
                });
            }
            if previous.is_null() {
                self.free = node;
            } else {
                // SAFETY: `previous` is a free-list node below the new one.
                unsafe { (*previous).next = node };
            }
        }
        self.salvage_tail();
    }

    /// Reclaim a free block that ends at the bump pointer.
    ///
    /// Without this, freeing the most recent allocation leaves the chunk tail stranded and the
    /// next allocation maps another chunk even though the space is unused.
    fn salvage_tail(&mut self) {
        if self.chunk.is_null() || self.chunk_offset == 0 {
            return;
        }
        let tail = (self.chunk as usize).saturating_add(self.chunk_offset);
        let mut previous: *mut FreeNode = ptr::null_mut();
        let mut node = self.free;
        while !node.is_null() {
            // SAFETY: the free list is address-ordered and every node is live.
            let size = unsafe { (*node).size };
            let end = (node as usize).saturating_add(size);
            if end == tail {
                // SAFETY: `node` is a live free-list node with a next pointer.
                let next = unsafe { (*node).next };
                if previous.is_null() {
                    self.free = next;
                } else {
                    // SAFETY: `previous` precedes `node`.
                    unsafe { (*previous).next = next };
                }
                if (node as usize) >= self.chunk as usize {
                    self.chunk_offset = node as usize - self.chunk as usize;
                }
                return;
            }
            if end > tail {
                return;
            }
            previous = node;
            // SAFETY: the free list is address-ordered and every node is live.
            node = unsafe { (*node).next };
        }
    }
}

/// A spin-locked allocator over a [`PageSource`].
pub struct Allocator<S> {
    source: UnsafeCell<S>,
    state: UnsafeCell<State>,
    lock: AtomicBool,
}

impl<S> Allocator<S> {
    /// Create an allocator over `source`.
    pub const fn new(source: S) -> Self {
        Self {
            source: UnsafeCell::new(source),
            state: UnsafeCell::new(State::empty()),
            lock: AtomicBool::new(false),
        }
    }

    fn with_state<R>(&self, function: impl FnOnce(&mut State, &mut S) -> R) -> R {
        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        // SAFETY: the spin lock gives exclusive access to both cells for this closure.
        let result = unsafe { function(&mut *self.state.get(), &mut *self.source.get()) };
        self.lock.store(false, Ordering::Release);
        result
    }
}

// SAFETY: the spin lock serializes all access to the UnsafeCells; S only needs Send.
unsafe impl<S: Send> Sync for Allocator<S> {}

// SAFETY: `alloc` and `dealloc` honour the `GlobalAlloc` contract through the serialized
// `with_state` access; callers supply valid layouts and live pointers.
unsafe impl<S: PageSource + Send> GlobalAlloc for Allocator<S> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.with_state(|state, source| state.alloc(source, layout))
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.with_state(|state, _| state.dealloc(pointer, layout));
    }
}

/// The process-wide allocator (feature `alloc`). Not built for host tests, which use their own
/// `TestSource` allocations.
#[cfg(not(test))]
#[global_allocator]
static GLOBAL: Allocator<SysPageSource> = Allocator::new(SysPageSource);

#[cfg(test)]
#[path = "alloc_ut.rs"]
mod tests;
