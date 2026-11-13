// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(unsafe_code)] // Host test arena allocation.

use super::*;
use core::cell::Cell;
use std::alloc::{Layout, alloc_zeroed};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Bump {
    base: usize,
    len: usize,
    used: Cell<usize>,
    frees: Cell<usize>,
}

impl Bump {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len, 4096).expect("layout");
        // SAFETY: the layout is non-zero and uses a valid alignment.
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null());
        Self {
            base: base as usize,
            len,
            used: Cell::new(0),
            frees: Cell::new(0),
        }
    }
}

impl Heap for Bump {
    fn malloc(&self, size: usize) -> usize {
        self.memalign(16, size)
    }
    fn free(&self, _p: usize) {
        self.frees.set(self.frees.get() + 1);
    }
    fn realloc(&self, _p: usize, size: usize) -> usize {
        self.malloc(size)
    }
    fn calloc(&self, n: usize, m: usize) -> usize {
        self.malloc(n.saturating_mul(m))
    }
    fn memalign(&self, align: usize, size: usize) -> usize {
        let used = self.used.get();
        let aligned = (self.base + used + align - 1) & !(align - 1);
        let start = aligned - self.base;
        if start + size + 64 > self.len {
            return 0;
        }
        self.used.set(start + size + 64);
        aligned
    }
    fn usable_size(&self, _p: usize) -> usize {
        0
    }
    fn trim(&self) -> bool {
        false
    }
    fn info(&self) -> Mallinfo2 {
        Mallinfo2::default()
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

struct Host {
    secure: bool,
    env: Option<&'static [u8]>,
    abort_text: Cell<usize>,
}

impl Platform for Host {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, _e: i32) {}
    fn write_stderr(&self, b: &[u8]) {
        self.abort_text.set(self.abort_text.get() + b.len());
    }
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"debug-test"
    }
    fn process_id(&self) -> u32 {
        7
    }
    fn is_secure_exec(&self) -> bool {
        self.secure
    }
    fn env_malloc_debug(&self) -> Option<&[u8]> {
        self.env
    }
}

fn host() -> Host {
    Host {
        secure: false,
        env: None,
        abort_text: Cell::new(0),
    }
}

fn options() -> DebugOptions {
    DebugOptions {
        redzone: true,
        fill: true,
        audit: 1,
        ..DebugOptions::default()
    }
}

#[test]
fn allocation_is_filled_and_free_is_quarantined() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(16);
    let mut bytes = [0u8; 16];
    crate::raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [ALLOC_FILL; 16]);
    debug.free(pointer);
    crate::raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [FREE_FILL; 16]);
}

#[test]
fn overrun_and_underrun_abort() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(8);
    crate::raw::write_u8(pointer + 8, 0xff);
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer))).is_err());

    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(8);
    crate::raw::write_u8(pointer - 1, 0xff);
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer))).is_err());
}

#[test]
fn double_free_aborts() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(8);
    debug.free(pointer);
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer))).is_err());
}

#[test]
fn interior_and_unknown_frees_abort() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(8);
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer + 1))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(0x1000))).is_err());
}

#[test]
fn use_after_free_is_caught_on_eviction() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(8);
    debug.free(pointer);
    crate::raw::write_u8(pointer, 0x11);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let second = debug.malloc(8);
        debug.free(second);
    }));
    assert!(result.is_err());
}

#[test]
fn fail_every_injects_enomem() {
    let debug = Debug::new(
        Bump::new(1024 * 1024),
        host(),
        DebugOptions {
            fail_every: 3,
            ..DebugOptions::default()
        },
    );
    assert_ne!(debug.malloc(8), 0);
    assert_ne!(debug.malloc(8), 0);
    assert_eq!(debug.malloc(8), 0);
    assert_ne!(debug.malloc(8), 0);
}

#[test]
fn leak_report_groups_live_buffers() {
    let debug = Debug::new(
        Bump::new(1024 * 1024),
        host(),
        DebugOptions {
            leak: true,
            ..DebugOptions::default()
        },
    );
    let _ = debug.malloc(8);
    let _ = debug.malloc(8);
    let _ = debug.malloc(16);
    let mut output = std::vec::Vec::new();
    debug.report_leaks(&mut |line| output.extend_from_slice(line));
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("2 x 8 bytes"));
    assert!(text.contains("1 x 16 bytes"));
}

#[test]
fn usable_size_is_the_requested_size() {
    let debug = Debug::new(Bump::new(1024 * 1024), host(), options());
    let pointer = debug.malloc(17);
    assert_eq!(debug.usable_size(pointer), 17);
}

#[test]
fn secure_exec_ignores_the_environment() {
    let debug = Debug::new(
        Bump::new(1024 * 1024),
        Host {
            secure: true,
            env: Some(b"redzone,fill"),
            abort_text: Cell::new(0),
        },
        DebugOptions::default(),
    );
    assert!(!debug.options().redzone);
    assert!(!debug.options().fill);
}

#[test]
fn malloc_with_stack_records_the_caller() {
    let debug = Debug::new(
        Bump::new(1024 * 1024),
        host(),
        DebugOptions {
            leak: true,
            ..DebugOptions::default()
        },
    );
    let _ = debug.malloc_with_stack(8, &[0x1234, 0x5678]);
    let mut output = std::vec::Vec::new();
    debug.report_leaks(&mut |line| output.extend_from_slice(line));
    let text = String::from_utf8(output).expect("utf8");
    assert!(text.contains("0x1234"));
}
