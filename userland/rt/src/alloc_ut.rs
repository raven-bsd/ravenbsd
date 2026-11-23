// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `alloc` host tests: mixed-size reuse, split, coalesce and bump-tail salvage.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// A source that counts the mapped bytes, so a test can assert the allocator reuses its chunks.
struct CountingSource {
    chunks: Vec<Box<[u8]>>,
    mapped: Arc<AtomicUsize>,
}

impl CountingSource {
    fn new(mapped: Arc<AtomicUsize>) -> Self {
        Self {
            chunks: Vec::new(),
            mapped,
        }
    }
}

impl PageSource for CountingSource {
    fn map(&mut self, len: usize) -> Option<*mut u8> {
        let mut chunk = vec![0u8; len + 64].into_boxed_slice();
        let base = chunk.as_mut_ptr() as usize;
        let pointer = (base + 63) & !63;
        self.chunks.push(chunk);
        self.mapped.fetch_add(len, Ordering::AcqRel);
        Some(pointer as *mut u8)
    }

    fn unmap(&mut self, _pointer: *mut u8, len: usize) {
        self.mapped.fetch_sub(len, Ordering::AcqRel);
    }
}

fn test_alloc<S: PageSource + Send>(allocator: &Allocator<S>, layout: Layout) -> *mut u8 {
    // SAFETY: `layout` is a valid layout and the allocator outlives the allocation.
    unsafe { allocator.alloc(layout) }
}

fn test_dealloc<S: PageSource + Send>(allocator: &Allocator<S>, pointer: *mut u8, layout: Layout) {
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
    assert!((first as usize + 48 <= second as usize) || (second as usize + 48 <= first as usize));
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
    assert!(a == second || a == first);
    assert!(b == second || b == first);
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

/// The vfsd pattern: a large transient block is freed, then a small request arrives before the
/// next large one. Without split/coalesce the small request consumes the large block and strands
/// its remainder, so the next large request maps a new chunk (Mark's K434 rev107 measurement:
/// +20 mapped chunks per SSH session).
#[test]
fn mixed_size_churn_reuses_mapped_chunks() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let big = Layout::from_size_align(64 * 1024, 8).unwrap();
    let small = Layout::from_size_align(8 * 1024, 8).unwrap();

    // Warm both sizes: one mapped chunk must serve the whole churn.
    let pointer = test_alloc(&allocator, big);
    assert!(!pointer.is_null());
    test_dealloc(&allocator, pointer, big);
    let pointer = test_alloc(&allocator, small);
    assert!(!pointer.is_null());
    test_dealloc(&allocator, pointer, small);
    let baseline = mapped.load(Ordering::Acquire);

    for _ in 0..32 {
        let pointer = test_alloc(&allocator, big);
        assert!(!pointer.is_null());
        test_write(pointer, 0x11, 64 * 1024);
        test_dealloc(&allocator, pointer, big);
        let pointer = test_alloc(&allocator, small);
        assert!(!pointer.is_null());
        test_write(pointer, 0x22, 8 * 1024);
        test_dealloc(&allocator, pointer, small);
    }

    assert_eq!(
        mapped.load(Ordering::Acquire),
        baseline,
        "mixed-size churn must reuse the mapped chunk"
    );
}

/// Two adjacent frees must merge so a following larger request does not map a new chunk.
#[test]
fn adjacent_frees_coalesce() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let half = Layout::from_size_align(16, 8).unwrap();
    let whole = Layout::from_size_align(32, 8).unwrap();
    let first = test_alloc(&allocator, half);
    let second = test_alloc(&allocator, half);
    assert_ne!(first, second);
    test_dealloc(&allocator, first, half);
    test_dealloc(&allocator, second, half);
    let baseline = mapped.load(Ordering::Acquire);
    let merged = test_alloc(&allocator, whole);
    assert!(!merged.is_null());
    assert_eq!(
        merged, first,
        "the coalesced pair must satisfy the larger request at the first block"
    );
    assert_eq!(
        mapped.load(Ordering::Acquire),
        baseline,
        "the coalesced pair must not map another chunk"
    );
    test_dealloc(&allocator, merged, whole);
}

/// Freeing the latest allocation must roll the bump pointer back so the tail is reusable.
#[test]
fn bump_tail_is_salvaged() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let small = Layout::from_size_align(16, 8).unwrap();
    let page = Layout::from_size_align(4096, 8).unwrap();
    let pointer = test_alloc(&allocator, small);
    test_dealloc(&allocator, pointer, small);
    let baseline = mapped.load(Ordering::Acquire);
    let pointer = test_alloc(&allocator, page);
    assert!(!pointer.is_null());
    assert_eq!(
        mapped.load(Ordering::Acquire),
        baseline,
        "the salvaged tail must satisfy the page request"
    );
    test_dealloc(&allocator, pointer, page);
}
