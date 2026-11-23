// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Optional bump + first-fit free-list allocator over `PageSource`.
//!
//! The production source maps anonymous memory with `MAP`; host tests use a `Vec`-backed source.
//! The allocator is serialized by a spin lock, as required by `GlobalAlloc`'s `&self` methods.

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

/// Smallest remainder worth keeping as a free node.
const MIN_FREE: usize = size_of::<FreeNode>();

/// Round `size` up to `align` (both non-zero, `align` a power of two).
fn round_up(size: usize, align: usize) -> Option<usize> {
    size.checked_add(align - 1)
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

    /// Insert a free node in address order and coalesce contiguous neighbours.
    ///
    /// Donor contract: NetBSD `lib/libc/stdlib/malloc.c:free_bytes` returns a
    /// freed chunk to its page's size class so the same class can reuse it, and
    /// `:malloc_bytes` only allocates a new page when that class is empty. This
    /// allocator is byte-granular rather than size-class paged, so it preserves the
    /// same property by splitting a reused block and merging adjacent free blocks.
    ///
    /// # Safety
    ///
    /// `pointer` must point to a mapped region of at least `size` bytes that is no
    /// longer reachable by the caller, and must be aligned for [`FreeNode`].
    unsafe fn insert_free(&mut self, pointer: *mut FreeNode, size: usize) {
        if size < size_of::<FreeNode>() {
            return;
        }
        let mut previous: *mut FreeNode = ptr::null_mut();
        let mut node = self.free;
        while !node.is_null() && (node as usize) < (pointer as usize) {
            previous = node;
            // SAFETY: every node in the list is a live `FreeNode` inside a mapped chunk.
            node = unsafe { (*node).next };
        }
        // SAFETY: the caller transferred this region; writing the node header is the only use.
        unsafe {
            pointer.write(FreeNode { size, next: node });
        }
        if previous.is_null() {
            self.free = pointer;
        } else {
            // SAFETY: `previous` is a node in the list and points below `pointer`.
            unsafe { (*previous).next = pointer };
        }
        // Merge with the following node when exactly adjacent.
        if !node.is_null() && (pointer as usize).saturating_add(size) == node as usize {
            // SAFETY: `node` follows `pointer` and both are live free nodes.
            unsafe {
                (*pointer).size = (*pointer).size.saturating_add((*node).size);
                (*pointer).next = (*node).next;
            }
        }
        // Merge with the preceding node when exactly adjacent.
        if !previous.is_null() {
            // SAFETY: `previous` is a live free node in the address-ordered list.
            let previous_end = (previous as usize).saturating_add(unsafe { (*previous).size });
            if previous_end == pointer as usize {
                // SAFETY: `previous` and `pointer` are adjacent live free nodes.
                unsafe {
                    (*previous).size = (*previous).size.saturating_add((*pointer).size);
                    (*previous).next = (*pointer).next;
                }
            }
        }
    }

    /// Take a free block of at least `size` bytes, splitting the tail back to the list.
    fn take_free(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let mut previous: *mut FreeNode = ptr::null_mut();
        let mut node = self.free;
        while !node.is_null() {
            // SAFETY: every free-list node is a live `FreeNode` in a mapped chunk.
            let node_size = unsafe { (*node).size };
            let node_align = (node as usize).is_multiple_of(align);
            if node_size >= size && node_align {
                // SAFETY: `node` is a live free node whose size was read above.
                let next = unsafe { (*node).next };
                if previous.is_null() {
                    self.free = next;
                } else {
                    // SAFETY: `previous` is a live list node.
                    unsafe { (*previous).next = next };
                }
                let split = round_up(size, core::mem::align_of::<FreeNode>())?;
                if node_size >= split.saturating_add(MIN_FREE) {
                    // SAFETY: the split offset stays inside the live `node_size` block.
                    let remainder = unsafe { (node as *mut u8).add(split) }.cast::<FreeNode>();
                    // SAFETY: the tail is inside the same live block, aligned for `FreeNode`,
                    // and is no longer reachable through the caller's allocation.
                    unsafe { self.insert_free(remainder, node_size - split) };
                }
                return Some(node.cast::<u8>());
            }
            previous = node;
            // SAFETY: `node` is a live free node from the list.
            node = unsafe { (*node).next };
        }
        None
    }

    fn alloc(&mut self, source: &mut dyn PageSource, layout: Layout) -> *mut u8 {
        let align = layout.align().max(core::mem::align_of::<FreeNode>());
        let Some(size) = round_up(layout.size().max(1), align) else {
            return ptr::null_mut();
        };

        if let Some(pointer) = self.take_free(size, align) {
            return pointer;
        }

        let base = self.chunk as usize;
        let Some(start) = base.checked_add(self.chunk_offset) else {
            return ptr::null_mut();
        };
        let Some(aligned) = round_up(start, align) else {
            return ptr::null_mut();
        };
        let Some(end) = aligned.checked_add(size) else {
            return ptr::null_mut();
        };
        let Some(offset_end) = end.checked_sub(base) else {
            return ptr::null_mut();
        };
        if self.chunk.is_null() || offset_end > self.chunk_len {
            // Salvage the unallocated tail before switching to a new chunk, so a later
            // small allocation can still use it.
            if !self.chunk.is_null() && self.chunk_offset < self.chunk_len {
                let tail = self.chunk_len - self.chunk_offset;
                if tail >= size_of::<FreeNode>() {
                    // SAFETY: the offset is inside the current mapped chunk.
                    let tail_pointer = unsafe { self.chunk.add(self.chunk_offset) };
                    // SAFETY: the tail is mapped, aligned by the allocation rounding, and
                    // no live allocation starts inside it.
                    unsafe { self.insert_free(tail_pointer.cast::<FreeNode>(), tail) };
                }
            }
            let Some(map_len) = size
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
        let Some(size) = round_up(layout.size().max(1), align) else {
            return;
        };
        let node = pointer.cast::<FreeNode>();
        // SAFETY: the caller hands back the live allocation and the rounded size matches the
        // size reserved by `alloc`, so the node stays inside that allocation.
        unsafe { self.insert_free(node, size) };
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
mod tests {
    use super::*;

    struct TestSource {
        chunks: Vec<Box<[u8]>>,
    }

    impl TestSource {
        fn new() -> Self {
            Self { chunks: Vec::new() }
        }
    }

    impl PageSource for TestSource {
        fn map(&mut self, len: usize) -> Option<*mut u8> {
            let mut chunk = vec![0u8; len + 64].into_boxed_slice();
            let base = chunk.as_mut_ptr() as usize;
            let pointer = (base + 63) & !63;
            self.chunks.push(chunk);
            Some(pointer as *mut u8)
        }

        fn unmap(&mut self, _pointer: *mut u8, _len: usize) {}
    }

    fn test_alloc(allocator: &Allocator<TestSource>, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is a valid layout and the allocator outlives the allocation.
        unsafe { allocator.alloc(layout) }
    }

    fn test_dealloc(allocator: &Allocator<TestSource>, pointer: *mut u8, layout: Layout) {
        // SAFETY: `pointer` came from `test_alloc` with the same layout and is still live.
        unsafe { allocator.dealloc(pointer, layout) }
    }

    fn test_write(pointer: *mut u8, byte: u8, len: usize) {
        // SAFETY: the caller passes a live allocation of at least `len` bytes.
        unsafe { pointer.write_bytes(byte, len) }
    }

    fn test_read(pointer: *const u8, index: usize) -> u8 {
        // SAFETY: the caller passes a live allocation with `index` in bounds.
        unsafe { *pointer.add(index) }
    }

    fn allocator() -> Allocator<TestSource> {
        Allocator::new(TestSource::new())
    }

    #[test]
    fn basic_alignment_and_size() {
        let allocator = allocator();
        let layout = Layout::from_size_align(100, 16).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!(!pointer.is_null());
        assert!((pointer as usize).is_multiple_of(16));
        test_write(pointer, 0xAA, 100);
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn alignment_one() {
        let allocator = allocator();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!(!pointer.is_null());
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn alignment_64() {
        let allocator = allocator();
        let layout = Layout::from_size_align(33, 64).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!((pointer as usize).is_multiple_of(64));
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn alignment_beyond_page_size() {
        let allocator = allocator();
        let layout = Layout::from_size_align(33, 8192).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!(!pointer.is_null());
        assert!((pointer as usize).is_multiple_of(8192));
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn free_list_reuses_exact_block() {
        let allocator = allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let first = test_alloc(&allocator, layout);
        test_dealloc(&allocator, first, layout);
        let second = test_alloc(&allocator, layout);
        assert_eq!(first, second);
        test_dealloc(&allocator, second, layout);
    }

    #[test]
    fn large_allocation_gets_another_chunk() {
        let allocator = allocator();
        let layout = Layout::from_size_align(8192, 8).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!(!pointer.is_null());
        test_write(pointer, 0x55, 8192);
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn zero_size_returns_writable_pointer() {
        let allocator = allocator();
        let layout = Layout::from_size_align(0, 1).unwrap();
        let pointer = test_alloc(&allocator, layout);
        assert!(!pointer.is_null());
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn multiple_allocations_do_not_overlap() {
        let allocator = allocator();
        let layout = Layout::from_size_align(48, 8).unwrap();
        let first = test_alloc(&allocator, layout);
        let second = test_alloc(&allocator, layout);
        assert_ne!(first, second);
        assert!(
            (first as usize + 48 <= second as usize) || (second as usize + 48 <= first as usize)
        );
        test_dealloc(&allocator, first, layout);
        test_dealloc(&allocator, second, layout);
    }

    #[test]
    fn freeing_small_layout_is_ignored() {
        let allocator = allocator();
        let layout = Layout::from_size_align(1, 1).unwrap();
        let pointer = test_alloc(&allocator, layout);
        test_dealloc(&allocator, pointer, layout);
        let next = test_alloc(&allocator, layout);
        assert!(!next.is_null());
        test_dealloc(&allocator, next, layout);
    }

    #[test]
    fn free_list_handles_multiple_blocks() {
        let allocator = allocator();
        let layout = Layout::from_size_align(24, 8).unwrap();
        let first = test_alloc(&allocator, layout);
        let second = test_alloc(&allocator, layout);
        test_dealloc(&allocator, first, layout);
        test_dealloc(&allocator, second, layout);
        let a = test_alloc(&allocator, layout);
        let b = test_alloc(&allocator, layout);
        assert!(
            a == second || a == first,
            "a={a:p} first={first:p} second={second:p}"
        );
        assert!(
            b == second || b == first,
            "b={b:p} first={first:p} second={second:p}"
        );
        assert_ne!(a, b);
    }

    #[test]
    fn writes_are_visible() {
        let allocator = allocator();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let pointer = test_alloc(&allocator, layout);
        test_write(pointer, 0x5A, 16);
        for index in 0..16 {
            assert_eq!(test_read(pointer, index), 0x5A);
        }
        test_dealloc(&allocator, pointer, layout);
    }

    #[test]
    fn allocation_after_dealloc_can_reuse_chunk() {
        let allocator = allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let pointer = test_alloc(&allocator, layout);
        test_dealloc(&allocator, pointer, layout);
        let again = test_alloc(&allocator, layout);
        assert_eq!(pointer, again);
        test_dealloc(&allocator, again, layout);
    }
}

#[cfg(test)]
#[path = "alloc_ut.rs"]
mod alloc_ut;
