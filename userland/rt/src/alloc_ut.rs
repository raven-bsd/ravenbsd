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

/// Keep one small block live so a test can observe free-list reuse without the allocator
/// returning the now-fully-free chunk to the page source.
fn anchor<S: PageSource + Send>(allocator: &Allocator<S>) -> (*mut u8, Layout) {
    let layout = Layout::from_size_align(8, 8).unwrap();
    (test_alloc(allocator, layout), layout)
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
    let (hold, hold_layout) = anchor(&allocator);
    let layout = Layout::from_size_align(32, 8).unwrap();
    let first = test_alloc(&allocator, layout);
    test_dealloc(&allocator, first, layout);
    let second = test_alloc(&allocator, layout);
    assert_eq!(first, second);
    test_dealloc(&allocator, second, layout);
    test_dealloc(&allocator, hold, hold_layout);
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
fn small_allocations_are_reusable() {
    let allocator = allocator();
    let (hold, hold_layout) = anchor(&allocator);
    let layout = Layout::from_size_align(1, 1).unwrap();
    let pointer = test_alloc(&allocator, layout);
    test_dealloc(&allocator, pointer, layout);
    let next = test_alloc(&allocator, layout);
    assert_eq!(pointer, next, "a small freed block must be reusable");
    test_dealloc(&allocator, next, layout);
    test_dealloc(&allocator, hold, hold_layout);
}

#[test]
fn free_list_handles_multiple_blocks() {
    let allocator = allocator();
    let (hold, hold_layout) = anchor(&allocator);
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
    test_dealloc(&allocator, a, layout);
    test_dealloc(&allocator, b, layout);
    test_dealloc(&allocator, hold, hold_layout);
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
    let (hold, hold_layout) = anchor(&allocator);
    let layout = Layout::from_size_align(32, 8).unwrap();
    let pointer = test_alloc(&allocator, layout);
    test_dealloc(&allocator, pointer, layout);
    let again = test_alloc(&allocator, layout);
    assert_eq!(pointer, again);
    test_dealloc(&allocator, again, layout);
    test_dealloc(&allocator, hold, hold_layout);
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

    // Keep one block in the large chunk so the churn exercises split/reuse (not a full unmap),
    // and the small request cannot consume the large block without returning its tail.
    let pointer = test_alloc(&allocator, big);
    assert!(!pointer.is_null());
    let (keeper, keeper_layout) = anchor(&allocator);
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
    test_dealloc(&allocator, keeper, keeper_layout);
}

/// Two adjacent frees must merge so a following larger request does not map a new chunk.
#[test]
fn adjacent_frees_coalesce() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let (hold, hold_layout) = anchor(&allocator);
    let half = Layout::from_size_align(16, 8).unwrap();
    let whole = Layout::from_size_align(32, 8).unwrap();
    let first = test_alloc(&allocator, half);
    let second = test_alloc(&allocator, half);
    let (sentinel, sentinel_layout) = anchor(&allocator);
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
    test_dealloc(&allocator, sentinel, sentinel_layout);
    test_dealloc(&allocator, hold, hold_layout);
}

/// A page-aligned source like `SysPageSource`, so the stress exercises real chunk addresses.
struct AlignedSource {
    chunks: Vec<Box<[u8]>>,
    mapped: Arc<AtomicUsize>,
}

impl AlignedSource {
    fn new(mapped: Arc<AtomicUsize>) -> Self {
        Self {
            chunks: Vec::new(),
            mapped,
        }
    }
}

impl PageSource for AlignedSource {
    fn map(&mut self, len: usize) -> Option<*mut u8> {
        let mut chunk = vec![0u8; len + 4096].into_boxed_slice();
        let base = chunk.as_mut_ptr() as usize;
        let pointer = (base + 4095) & !4095;
        self.chunks.push(chunk);
        self.mapped.fetch_add(len, Ordering::AcqRel);
        Some(pointer as *mut u8)
    }

    fn unmap(&mut self, _pointer: *mut u8, len: usize) {
        self.mapped.fetch_sub(len, Ordering::AcqRel);
    }
}

/// Deterministic mixed-size alloc/free stress over page-aligned chunks. Every live block is
/// filled with its own byte and verified before it is freed, so any overlap or undersized block
/// fails the row.
#[test]
fn mixed_sizes_never_overlap() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(AlignedSource::new(Arc::clone(&mapped)));
    let sizes = [
        1usize, 2, 3, 5, 6, 8, 9, 15, 16, 17, 24, 31, 32, 48, 63, 64, 100, 127, 128, 255, 256, 512,
        1000, 4096, 8192,
    ];
    let aligns = [1usize, 2, 4, 8, 16, 32, 64, 128, 4096];
    let mut slots: Vec<Option<(*mut u8, Layout, u8)>> = vec![None; 48];
    let mut rng = 0x1234_5678_9abc_def0u64;
    for step in 0..20_000 {
        rng = rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let index = ((rng >> 33) as usize) % slots.len();
        if let Some((pointer, layout, byte)) = slots[index] {
            for offset in 0..layout.size() {
                assert_eq!(
                    test_read(pointer, offset),
                    byte,
                    "overlap or undersize at step {step}, offset {offset}"
                );
            }
            test_dealloc(&allocator, pointer, layout);
            slots[index] = None;
        } else {
            let size = sizes[((rng >> 8) as usize) % sizes.len()];
            let align = aligns[((rng >> 20) as usize) % aligns.len()];
            let layout = Layout::from_size_align(size, align).expect("valid layout");
            let pointer = test_alloc(&allocator, layout);
            assert!(!pointer.is_null(), "allocation failed at step {step}");
            assert!(
                (pointer as usize).is_multiple_of(align),
                "misaligned pointer at step {step}"
            );
            let byte = (rng >> 48) as u8;
            for offset in 0..size {
                // SAFETY: the allocator returned a block of at least `layout.size()` bytes.
                unsafe { *pointer.add(offset) = byte };
            }
            slots[index] = Some((pointer, layout, byte));
        }
    }
    for slot in slots.into_iter().flatten() {
        test_dealloc(&allocator, slot.0, slot.1);
    }
}

/// Freeing the only block in the current chunk must return the chunk mapping, not just the bump
/// pointer: vfsd's mapped user pages must not stay resident after the session's work is gone.
#[test]
fn a_fully_free_chunk_is_unmapped() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let layout = Layout::from_size_align(64 * 1024, 8).unwrap();
    let pointer = test_alloc(&allocator, layout);
    assert!(!pointer.is_null());
    assert!(
        mapped.load(Ordering::Acquire) >= 64 * 1024,
        "the allocation must map a chunk"
    );
    test_dealloc(&allocator, pointer, layout);
    assert_eq!(
        mapped.load(Ordering::Acquire),
        0,
        "the empty chunk must be unmapped"
    );
}

#[test]
fn bump_tail_is_salvaged() {
    let mapped = Arc::new(AtomicUsize::new(0));
    let allocator = Allocator::new(CountingSource::new(Arc::clone(&mapped)));
    let (hold, hold_layout) = anchor(&allocator);
    let target = Layout::from_size_align(4000, 8).unwrap();
    let pointer = test_alloc(&allocator, target);
    test_dealloc(&allocator, pointer, target);
    let baseline = mapped.load(Ordering::Acquire);
    let pointer = test_alloc(&allocator, target);
    assert!(!pointer.is_null());
    assert_eq!(
        mapped.load(Ordering::Acquire),
        baseline,
        "the salvaged tail must satisfy the next request"
    );
    test_dealloc(&allocator, pointer, target);
    test_dealloc(&allocator, hold, hold_layout);
}
