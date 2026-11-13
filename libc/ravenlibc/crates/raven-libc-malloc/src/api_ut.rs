// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::cell::Cell;

struct StubHeap {
    next: Cell<usize>,
    freed: Cell<usize>,
    realloc_result: Cell<usize>,
    memalign_result: Cell<usize>,
    recorded_align: Cell<usize>,
}

impl StubHeap {
    fn new() -> Self {
        Self {
            next: Cell::new(0x1000),
            freed: Cell::new(0),
            realloc_result: Cell::new(0),
            memalign_result: Cell::new(0x2000),
            recorded_align: Cell::new(0),
        }
    }
}

impl Heap for StubHeap {
    fn malloc(&self, _size: usize) -> usize {
        let value = self.next.get();
        self.next.set(value + 16);
        value
    }
    fn free(&self, p: usize) {
        self.freed.set(p);
    }
    fn realloc(&self, _p: usize, _size: usize) -> usize {
        self.realloc_result.get()
    }
    fn calloc(&self, _n: usize, _m: usize) -> usize {
        self.malloc(0)
    }
    fn memalign(&self, align: usize, _size: usize) -> usize {
        self.recorded_align.set(align);
        self.memalign_result.get()
    }
    fn usable_size(&self, _p: usize) -> usize {
        64
    }
    fn trim(&self) -> bool {
        true
    }
    fn info(&self) -> Mallinfo2 {
        Mallinfo2 {
            arena: usize::MAX,
            ..Mallinfo2::default()
        }
    }
    fn free_sized(&self, p: usize, _size: usize) {
        self.free(p);
    }
    fn free_aligned_sized(&self, p: usize, _align: usize, _size: usize) {
        self.free(p);
    }
    fn prefork(&self) {}
    fn postfork_parent(&self) {}
    fn postfork_child(&self) {}
}

struct StubPlatform {
    errno: Cell<i32>,
}

impl StubPlatform {
    fn new() -> Self {
        Self {
            errno: Cell::new(0),
        }
    }
}

impl Platform for StubPlatform {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, e: i32) {
        self.errno.set(e);
    }
    fn get_errno(&self) -> i32 {
        self.errno.get()
    }
    fn write_stderr(&self, _b: &[u8]) {}
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"api-test"
    }
    fn process_id(&self) -> u32 {
        7
    }
}

fn api() -> Api<StubHeap, StubPlatform> {
    Api::new(StubHeap::new(), StubPlatform::new())
}

#[test]
fn malloc_zero_is_non_null_and_too_large_is_enomem() {
    let api = api();
    assert_ne!(api.malloc(0), 0);
    assert_eq!(api.malloc(MAX_ALLOC + 1), 0);
    assert_eq!(api.heap().next.get(), 0x1010);
}

#[test]
fn free_null_does_not_touch_the_heap() {
    let api = api();
    api.free(0);
    assert_eq!(api.heap().freed.get(), 0);
}

#[test]
fn realloc_zero_returns_a_minimum_block_and_keeps_p() {
    let api = api();
    let first = api.malloc(8);
    assert_ne!(api.realloc(first, 0), 0);
    assert_eq!(api.heap().freed.get(), 0);
}

#[test]
fn realloc_failure_leaves_the_original() {
    let api = api();
    let first = api.malloc(8);
    assert_eq!(api.realloc(first, 16), 0);
    assert_eq!(api.heap().freed.get(), 0);
}

#[test]
fn calloc_and_reallocarray_overflow_leave_the_original() {
    let api = api();
    assert_eq!(api.calloc(usize::MAX / 2 + 1, 2), 0);
    let first = api.malloc(8);
    assert_eq!(api.reallocarray(first, usize::MAX / 2 + 1, 2), 0);
    assert_eq!(api.heap().freed.get(), 0);
}

#[test]
fn posix_memalign_codes_and_errno() {
    let api = api();
    let mut out = 0xdead;
    assert_eq!(
        api.posix_memalign(&mut out, 0, 8),
        raven_libc_abi::errno::EINVAL
    );
    assert_eq!(out, 0xdead);
    assert_eq!(
        api.posix_memalign(&mut out, 3, 8),
        raven_libc_abi::errno::EINVAL
    );
    assert_eq!(out, 0xdead);
    assert_eq!(
        api.posix_memalign(&mut out, 4, 8),
        raven_libc_abi::errno::EINVAL
    );
    assert_eq!(out, 0xdead);
    assert_eq!(api.posix_memalign(&mut out, 16, 8), 0);
    assert_eq!(out, 0x2000);
    assert_eq!(api.heap().recorded_align.get(), 16);
}

#[test]
fn aligned_alloc_accepts_any_size_but_requires_a_power_of_two() {
    let api = api();
    assert_eq!(api.aligned_alloc(3, 16), 0);
    assert_eq!(api.aligned_alloc(16, 17), 0x2000);
}

#[test]
fn memalign_rounds_up() {
    let api = api();
    assert_eq!(api.memalign(3, 16), 0x2000);
    assert_eq!(api.heap().recorded_align.get(), 4);
}

#[test]
fn valloc_and_pvalloc_use_pages() {
    let api = api();
    assert_eq!(api.valloc(1), 0x2000);
    assert_eq!(api.heap().recorded_align.get(), PAGE_SIZE);
    assert_eq!(api.pvalloc(1), 0x2000);
    assert_eq!(api.heap().recorded_align.get(), PAGE_SIZE);
}

#[test]
fn mallopt_accepts_the_documented_numbers() {
    let api = api();
    for parameter in [1, -1, -2, -3, -5, -6, -7, -8, -100, -101, -102] {
        assert_eq!(api.mallopt(parameter, 0), 1, "{parameter}");
    }
    assert_eq!(api.mallopt(2, 0), 0);
}

#[test]
fn mallinfo_clamps_to_i32_max() {
    let api = api();
    assert_eq!(api.mallinfo().arena, i32::MAX);
    assert_eq!(api.mallinfo2().arena, usize::MAX);
}

#[test]
fn malloc_trim_reports_the_backend_result() {
    let api = api();
    assert_eq!(api.malloc_trim(0), 1);
}

#[test]
fn usable_size_zeroes_null_and_tracks_the_heap() {
    let api = api();
    assert_eq!(api.malloc_usable_size(0), 0);
    assert_eq!(api.malloc_usable_size(0x1000), 64);
}

#[test]
fn recallocarray_shrinks_in_place_when_the_difference_is_small() {
    let api = Api::new(
        crate::compat::testheap::TestHeap::new(),
        StubPlatform::new(),
    );
    let pointer = api.malloc(64);
    assert_eq!(api.recallocarray(pointer, 64, 40, 1), pointer);
}

#[test]
fn recallocarray_overflow_codes() {
    let api = api();
    let pointer = api.malloc(8);
    assert_eq!(api.recallocarray(pointer, 8, usize::MAX / 2 + 1, 2), 0);
    assert_eq!(api.recallocarray(pointer, usize::MAX / 2 + 1, 8, 2), 0);
}

#[test]
fn reallocf_is_reachable_through_api() {
    let api = api();
    api.heap().realloc_result.set(0);
    assert_eq!(api.reallocf(0x1000, 8), 0);
}

#[test]
fn freezero_and_free_sized_are_reachable() {
    let api = api();
    let pointer = api.malloc(8);
    api.free_sized(pointer, 8);
    let pointer = api.malloc(8);
    api.free_aligned_sized(pointer, 16, 8);
}

#[test]
fn freezero_is_a_no_op_for_null() {
    let api = api();
    api.freezero(0, 8);
}

#[test]
fn malloc_stats_writes_without_panicking() {
    let api = api();
    api.malloc_stats();
}

#[test]
fn fork_hooks_are_reachable() {
    let api = api();
    api.prefork();
    api.postfork_parent();
    api.postfork_child();
}

#[test]
fn heap_accessor_exposes_the_backend() {
    let api = api();
    assert_eq!(api.heap().usable_size(0x1000), 64);
}
