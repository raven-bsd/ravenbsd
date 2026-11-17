// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Regression tests for the lazily allocated `getttyent` string arena.

use super::*;
use core::ffi::CStr;

fn entry() -> TtyEnt {
    TtyEnt {
        name: b"ttyv0".to_vec(),
        getty: b"/usr/libexec/getty Pc".to_vec(),
        type_: b"cons25".to_vec(),
        status: 1,
        window: b"80x24".to_vec(),
        comment: b"".to_vec(),
        group: b"dialer".to_vec(),
    }
}

unsafe fn text(pointer: *mut c_char) -> Vec<u8> {
    // SAFETY: `fill` NUL-terminates every field in the private arena.
    unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec()
}

#[test]
fn successive_fills_reuse_one_heap_arena() {
    // SAFETY: the C API is non-reentrant and this test is the only caller.
    let first = unsafe { fill(&entry()) };
    // SAFETY: as above.
    let second = unsafe { fill(&entry()) };
    assert_eq!(first, second);
    // SAFETY: every pointer was produced by `fill`.
    unsafe {
        assert_eq!(text((*first).ty_name), b"ttyv0");
        assert_eq!(text((*first).ty_getty), b"/usr/libexec/getty Pc");
        assert_eq!(text((*first).ty_type), b"cons25");
        assert_eq!(text((*first).ty_window), b"80x24");
        assert_eq!(text((*first).ty_group), b"dialer");
    }
}

#[test]
fn a_later_fill_overwrites_the_previous_record() {
    let mut long = entry();
    long.name = b"ttyv1".to_vec();
    long.comment = b"local console".to_vec();
    // SAFETY: the C API is non-reentrant and this test is the only caller.
    unsafe {
        let pointer = fill(&long);
        assert_eq!(text((*pointer).ty_name), b"ttyv1");
        assert_eq!(text((*pointer).ty_comment), b"local console");
    }
}
