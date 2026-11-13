// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adversarial abort paths (the `malloc_errs` table, reduced to the reachable phrases).

#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use common::{SpinLock, TestEntropy, VecProvider};
use raven_libc_malloc::{Entropy, Hardened, Heap, Lock, PageProvider, Platform};

struct Recorder {
    text: Mutex<String>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            text: Mutex::new(String::new()),
        }
    }
    fn text(&self) -> String {
        self.text.lock().expect("text").clone()
    }
}

impl Platform for Recorder {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, _e: i32) {}
    fn write_stderr(&self, bytes: &[u8]) {
        self.text
            .lock()
            .expect("text")
            .push_str(&String::from_utf8_lossy(bytes));
    }
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"errs"
    }
    fn process_id(&self) -> u32 {
        1
    }
}

fn heap(options: &[u8]) -> Hardened<VecProvider, SpinLock, TestEntropy, Recorder> {
    Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(1),
        Recorder::new(),
        options,
    )
}

#[test]
fn double_free_reports_the_phrase() {
    let heap = heap(b"F");
    let pointer = heap.malloc(16);
    heap.free(pointer);
    let result = catch_unwind(AssertUnwindSafe(|| heap.free(pointer)));
    assert!(result.is_err());
    assert!(heap.platform().text().contains("double free"));
}

#[test]
fn bogus_pointer_reports_the_phrase() {
    let heap = heap(b"");
    let result = catch_unwind(AssertUnwindSafe(|| heap.free(0x1234)));
    assert!(result.is_err());
    assert!(heap.platform().text().contains("bogus pointer"));
}

#[test]
fn canary_reports_the_phrase() {
    let heap = heap(b"C");
    let pointer = heap.malloc(8);
    unsafe { core::ptr::write_bytes((pointer + 8) as *mut u8, 0, 8) };
    let result = catch_unwind(AssertUnwindSafe(|| heap.free(pointer)));
    assert!(result.is_err());
    assert!(heap.platform().text().contains("canary corrupted"));
}

#[test]
fn free_sized_size_mismatch_reports_the_phrase() {
    let heap = heap(b"");
    let pointer = heap.malloc(16);
    let result = catch_unwind(AssertUnwindSafe(|| heap.free_sized(pointer, 64)));
    assert!(result.is_err());
    assert!(heap.platform().text().contains("recorded size"));
}

#[test]
fn provider_failure_aborts_with_the_init_text() {
    struct NoPages;
    impl PageProvider for NoPages {
        fn page_size(&self) -> usize {
            4096
        }
        fn map_pages(&self, _len: usize, _align: usize, _prot: u8) -> Option<usize> {
            None
        }
        fn map_exact(&self, _hint: usize, _len: usize) -> Option<usize> {
            None
        }
        fn unmap(&self, _addr: usize, _len: usize) -> bool {
            false
        }
        fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
            false
        }
    }
    struct NoEntropy;
    impl Entropy for NoEntropy {
        fn fill(&self, _buf: &mut [u8]) -> bool {
            false
        }
    }
    struct NoLocks;
    impl Lock for NoLocks {
        fn lock(&self, _index: usize) {}
        fn unlock(&self, _index: usize) {}
        fn reset(&self, _index: usize) {}
    }
    let heap = Hardened::new(NoPages, NoLocks, NoEntropy, Recorder::new(), b"");
    let result = catch_unwind(AssertUnwindSafe(|| heap.malloc(1)));
    assert!(result.is_err());
}
