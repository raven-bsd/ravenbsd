// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::open`]: the mode strings (T8), `fdopen` access checks, `freopen`
//! and the caller-allocated read-only FILE.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    clippy::undocumented_unsafe_blocks,
    clippy::uninlined_format_args
)]

use super::*;
use crate::fake_ut::{Kind, leaked_file, leaked_state};
use crate::file::{F_APP, F_LAZYBUF, F_NORD, F_NOWR, F_PERM};
use crate::traits::SysFile;

#[test]
fn fopen_modes_and_flags() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"some data", 1024);
    let read = crate::raw::file_ref(crate::open::fopen(state, b"/f", b"r").expect("r"));
    assert_ne!(read.flags() & F_NOWR, 0);
    assert_eq!(read.flags() & F_LAZYBUF, F_LAZYBUF);
    assert_eq!(read.fd(), 3);
    read.check_invariants();
    let read_plus = crate::raw::file_ref(crate::open::fopen(state, b"/f", b"r+b").expect("r+b"));
    assert_eq!(read_plus.flags() & (F_NOWR | F_NORD), 0);
    let write = crate::raw::file_ref(crate::open::fopen(state, b"/f", b"w").expect("w"));
    assert_ne!(write.flags() & F_NORD, 0);
    let append = crate::raw::file_ref(crate::open::fopen(state, b"/f", b"ab+").expect("ab+"));
    assert_ne!(append.flags() & F_APP, 0);
    assert_eq!(append.flags() & (F_NOWR | F_NORD), 0);
}

#[test]
fn open_modes_reject_a_bad_first_character() {
    let state = leaked_state();
    assert_eq!(
        crate::open::fopen(state, b"/f", b"q").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(
        crate::open::fopen(state, b"/f", b"").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn open_error_is_propagated() {
    let state = leaked_state();
    assert_eq!(
        crate::open::fopen(state, b"/missing", b"r").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn fdopen_rejects_a_contradicting_mode() {
    let state = leaked_state();
    let write_only = state.env().push_fd(b"", Kind::Regular(1024));
    state.env().setfl(write_only, 1).expect("setfl");
    assert_eq!(
        crate::open::fdopen(state, write_only, b"r").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    let read_only = state.env().push_fd(b"", Kind::Regular(1024));
    state.env().setfl(read_only, 0).expect("setfl");
    assert_eq!(
        crate::open::fdopen(state, read_only, b"w").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    let file = crate::raw::file_ref(crate::open::fdopen(state, read_only, b"r").expect("fdopen r"));
    assert_ne!(file.flags() & F_NOWR, 0);
}

#[test]
fn fdopen_append_sets_o_append() {
    let state = leaked_state();
    let fd = state.env().push_fd(b"", Kind::Regular(1024));
    state.env().setfl(fd, 1).expect("setfl");
    let file = crate::raw::file_ref(crate::open::fdopen(state, fd, b"a").expect("fdopen a"));
    assert_ne!(file.flags() & F_APP, 0);
    assert_ne!(state.env().getfl(fd).expect("getfl") & 0o2000, 0);
}

#[test]
fn freopen_without_a_path_reapplies_the_flags() {
    let state = leaked_state();
    let fd = state.env().push_fd(b"", Kind::Regular(1024));
    state.env().setfl(fd, 2).expect("setfl");
    let file = crate::raw::file_ref(crate::open::fdopen(state, fd, b"r+").expect("fdopen"));
    assert_eq!(
        crate::open::freopen(state, None, b"w", file).map(|_| ()),
        Ok(())
    );
    assert_eq!(state.env().getfl(fd).expect("getfl") & 3, 1);
    assert_eq!(file.st().mode, 0);
}

#[test]
fn freopen_with_a_path_replaces_the_descriptor() {
    let state = leaked_state();
    state.env().add_file(b"/a", b"aaa", 1024);
    state.env().add_file(b"/b", b"bbb", 1024);
    let file = crate::raw::file_ref(crate::open::fopen(state, b"/a", b"r").expect("fopen a"));
    let old_fd = file.fd();
    assert_eq!(
        crate::open::freopen(state, Some(b"/b"), b"r", file).map(|_| ()),
        Ok(())
    );
    assert_ne!(file.fd(), old_fd);
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'b')));
}

#[test]
fn open_rb_ca_builds_a_permanent_read_only_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"resolver", 1024);
    let file = leaked_file(state);
    let buffer = std::boxed::Box::leak(std::boxed::Box::new([0u8; 64]));
    crate::open::fopen_rb_ca(state, b"/f", file, (buffer.as_mut_ptr(), buffer.len()))
        .expect("fopen_rb_ca");
    assert_ne!(file.flags() & F_PERM, 0);
    assert_ne!(file.flags() & F_NOWR, 0);
    assert_eq!(file.st().buf_size, 64 - crate::file::UNGET);
    assert!(state.ofl_head().is_none());
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'r')));
}
