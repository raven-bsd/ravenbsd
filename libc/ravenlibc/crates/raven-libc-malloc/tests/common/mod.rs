// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host test support: a leaked arena, a spin lock, deterministic entropy and a platform.

#![allow(dead_code)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

use raven_libc_malloc::compat::{Heap, Mallinfo2};
use raven_libc_malloc::{Entropy, Lock, PageProvider, Platform};

/// A leaked, page-aligned arena with a sorted free table.
pub struct VecProvider {
    base: usize,
    len: usize,
    used: AtomicUsize,
    map_calls: AtomicUsize,
    unmap_calls: AtomicUsize,
    protect_calls: AtomicUsize,
    align_calls: AtomicUsize,
}

impl VecProvider {
    /// A 512 MiB arena.
    pub fn new() -> Self {
        Self::with_len(512 * 1024 * 1024)
    }

    /// An arena of `len` bytes.
    pub fn with_len(len: usize) -> Self {
        let layout = Layout::from_size_align(len, 4096).expect("layout");
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null());
        Self {
            base: base as usize,
            len,
            used: AtomicUsize::new(0),
            map_calls: AtomicUsize::new(0),
            unmap_calls: AtomicUsize::new(0),
            protect_calls: AtomicUsize::new(0),
            align_calls: AtomicUsize::new(0),
        }
    }

    pub fn map_calls(&self) -> usize {
        self.map_calls.load(Ordering::Relaxed)
    }

    pub fn unmap_calls(&self) -> usize {
        self.unmap_calls.load(Ordering::Relaxed)
    }

    pub fn protect_calls(&self) -> usize {
        self.protect_calls.load(Ordering::Relaxed)
    }

    pub fn align_calls(&self) -> usize {
        self.align_calls.load(Ordering::Relaxed)
    }
}

impl Default for VecProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VecProvider {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.len, 4096).expect("layout");
        unsafe { dealloc(self.base as *mut u8, layout) };
    }
}

impl PageProvider for VecProvider {
    fn page_size(&self) -> usize {
        4096
    }

    fn map_pages(&self, len: usize, align: usize, _prot: u8) -> Option<usize> {
        self.map_calls.fetch_add(1, Ordering::Relaxed);
        if align > 4096 {
            self.align_calls.fetch_add(1, Ordering::Relaxed);
        }
        let used = self.used.load(Ordering::Relaxed);
        let aligned = (self.base + used + align - 1) & !(align - 1);
        let start = aligned - self.base;
        if start + len > self.len {
            return None;
        }
        self.used.store(start + len, Ordering::Relaxed);
        Some(aligned)
    }

    fn map_exact(&self, hint: usize, len: usize) -> Option<usize> {
        let used = self.used.load(Ordering::Relaxed);
        if hint == self.base + used && used + len <= self.len {
            self.used.store(used + len, Ordering::Relaxed);
            return Some(hint);
        }
        None
    }

    fn unmap(&self, _addr: usize, _len: usize) -> bool {
        self.unmap_calls.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
        self.protect_calls.fetch_add(1, Ordering::Relaxed);
        true
    }
}

/// A real spin-lock array, usable from several threads.
pub struct SpinLock {
    locks: [AtomicBool; 32],
    resets: AtomicUsize,
}

impl SpinLock {
    pub fn new() -> Self {
        Self {
            locks: core::array::from_fn(|_| AtomicBool::new(false)),
            resets: AtomicUsize::new(0),
        }
    }

    pub fn resets(&self) -> usize {
        self.resets.load(Ordering::Relaxed)
    }
}

impl Default for SpinLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Lock for SpinLock {
    fn lock(&self, index: usize) {
        let lock = &self.locks[index % 32];
        while lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn unlock(&self, index: usize) {
        self.locks[index % 32].store(false, Ordering::Release);
    }

    fn reset(&self, index: usize) {
        self.locks[index % 32].store(false, Ordering::Release);
        self.resets.fetch_add(1, Ordering::Relaxed);
    }
}

/// Deterministic xoshiro256** entropy.
pub struct TestEntropy {
    state: AtomicU64,
}

impl TestEntropy {
    pub fn new(seed: u64) -> Self {
        Self {
            state: AtomicU64::new(seed | 1),
        }
    }
}

impl Entropy for TestEntropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        let mut index = 0usize;
        while index < buf.len() {
            let mut value = self.state.load(Ordering::Relaxed);
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.state.store(value, Ordering::Relaxed);
            buf[index] = value as u8;
            index += 1;
        }
        true
    }
}

/// A platform that records errno, thread id and output without touching stdio.
pub struct TestPlatform {
    pub thread: AtomicUsize,
    pub errno: AtomicI32,
    pub writes: AtomicUsize,
    pub secure: bool,
}

impl TestPlatform {
    pub fn new(thread: usize) -> Self {
        Self {
            thread: AtomicUsize::new(thread),
            errno: AtomicI32::new(0),
            writes: AtomicUsize::new(0),
            secure: true,
        }
    }

    pub fn errno(&self) -> i32 {
        self.errno.load(Ordering::Relaxed)
    }

    pub fn writes(&self) -> usize {
        self.writes.load(Ordering::Relaxed)
    }
}

impl Platform for TestPlatform {
    fn thread_id(&self) -> usize {
        self.thread.load(Ordering::Relaxed)
    }
    fn set_errno(&self, e: i32) {
        self.errno.store(e, Ordering::Relaxed);
    }
    fn get_errno(&self) -> i32 {
        self.errno.load(Ordering::Relaxed)
    }
    fn write_stderr(&self, _b: &[u8]) {
        self.writes.fetch_add(1, Ordering::Relaxed);
    }
    fn abort(&self) -> ! {
        panic!("allocator abort")
    }
    fn progname(&self) -> &[u8] {
        b"malloc-test"
    }
    fn process_id(&self) -> u32 {
        7
    }
    fn is_secure_exec(&self) -> bool {
        self.secure
    }
}

/// A provider that allocates each mapping with the system allocator (Linux host only).
pub struct MmapProvider {
    maps: std::sync::Mutex<std::vec::Vec<(usize, usize)>>,
}

impl MmapProvider {
    pub fn new() -> Self {
        Self {
            maps: std::sync::Mutex::new(std::vec::Vec::new()),
        }
    }
}

impl Default for MmapProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PageProvider for MmapProvider {
    fn page_size(&self) -> usize {
        4096
    }
    fn map_pages(&self, len: usize, _align: usize, _prot: u8) -> Option<usize> {
        let layout = Layout::from_size_align(len.max(4096), 4096).ok()?;
        let pointer = unsafe { alloc_zeroed(layout) };
        if pointer.is_null() {
            return None;
        }
        self.maps
            .lock()
            .expect("map table")
            .push((pointer as usize, layout.size()));
        Some(pointer as usize)
    }
    fn map_exact(&self, _hint: usize, _len: usize) -> Option<usize> {
        None
    }
    fn unmap(&self, addr: usize, len: usize) -> bool {
        let layout = Layout::from_size_align(len.max(4096), 4096).expect("layout");
        unsafe { dealloc(addr as *mut u8, layout) };
        true
    }
    fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
        true
    }
}

/// A reference allocator for the model test.
pub struct ModelHeap {
    blocks: std::collections::BTreeMap<usize, Block>,
    next: usize,
    pattern: u8,
}

pub struct Block {
    pub size: usize,
    pub align: usize,
}

impl ModelHeap {
    pub fn new() -> Self {
        Self {
            blocks: std::collections::BTreeMap::new(),
            next: 0x1000,
            pattern: 0,
        }
    }

    pub fn allocate(&mut self, size: usize, align: usize) -> usize {
        let address = (self.next + align - 1) & !(align - 1);
        self.next = address + size.max(1) + 64;
        self.blocks.insert(address, Block { size, align });
        address
    }

    pub fn free(&mut self, address: usize) -> Option<Block> {
        self.blocks.remove(&address)
    }

    pub fn contains(&self, address: usize) -> bool {
        self.blocks.contains_key(&address)
    }

    pub fn overlaps(&self, address: usize, size: usize) -> bool {
        self.blocks.iter().any(|(base, block)| {
            let end = base.saturating_add(block.size.max(1));
            address < end && *base < address.saturating_add(size.max(1))
        })
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn pattern(&mut self) -> u8 {
        self.pattern = self.pattern.wrapping_add(1);
        self.pattern
    }
}

impl Default for ModelHeap {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple size-class heap for the thread test.
pub fn thread_heap()
-> raven_libc_malloc::Hardened<MmapProvider, SpinLock, TestEntropy, TestPlatform> {
    raven_libc_malloc::Hardened::new(
        MmapProvider::new(),
        SpinLock::new(),
        TestEntropy::new(9),
        TestPlatform::new(1),
        b"",
    )
}

/// Touch an allocation and report the first mismatch.
pub fn verify_pattern(heap: &dyn Heap, address: usize, size: usize, pattern: u8) -> bool {
    let mut index = 0usize;
    while index < size {
        let value = unsafe { *(address as *const u8).add(index) };
        if value != pattern {
            return false;
        }
        index += 1;
    }
    let _ = heap.info();
    true
}

/// `mallinfo2` baseline helper.
pub fn baseline(heap: &dyn Heap) -> Mallinfo2 {
    heap.info()
}
