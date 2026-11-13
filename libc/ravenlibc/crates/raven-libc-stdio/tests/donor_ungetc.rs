// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! glibc `libio/bug-ungetc*.c`, `bug-rewind.c`, `bug-fseek.c` and `bug-ftell.c` expected
//! values (expected values only; never code) plus the OpenBSD `test_ungetwc.c` wide case.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::bool_assert_comparison
)]

mod support;

use raven_libc_stdio::{Fail, fclose, fgetc, fgetwc, fseek, ftell, rewind, ungetc, ungetwc};

#[test]
fn glibc_bug_ungetc_pushback_then_tell() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(ftell(file), Ok(1));
    assert_eq!(ungetc(file, i32::from(b'a')), Ok(i32::from(b'a')));
    assert_eq!(ftell(file), Ok(0));
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(ftell(file), Ok(1));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_ungetc2_seek_after_pushback() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(ungetc(file, i32::from(b'Z')), Ok(i32::from(b'Z')));
    assert_eq!(fseek(file, 0, 1), Ok(()));
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(ftell(file), Ok(1));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_ungetc3_multiple_pushbacks() {
    let (state, env) = support::state();
    env.add(b"/f", b"abc");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(ungetc(file, i32::from(b'x')), Ok(i32::from(b'x')));
    assert_eq!(ungetc(file, i32::from(b'y')), Ok(i32::from(b'y')));
    assert_eq!(fgetc(file), Ok(i32::from(b'y')));
    assert_eq!(fgetc(file), Ok(i32::from(b'x')));
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_ungetc4_eof_is_cleared() {
    let (state, env) = support::state();
    env.add(b"/f", b"");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Err(Fail::Eof));
    assert_eq!(ungetc(file, i32::from(b'q')), Ok(i32::from(b'q')));
    assert!(!raven_libc_stdio::feof(file));
    assert_eq!(fgetc(file), Ok(i32::from(b'q')));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_rewind_after_pushback() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(fgetc(file), Ok(i32::from(b'b')));
    assert_eq!(ungetc(file, i32::from(b'b')), Ok(i32::from(b'b')));
    rewind(file);
    assert_eq!(ftell(file), Ok(0));
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_fseek_cur_discards_pushback() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(ungetc(file, i32::from(b'a')), Ok(i32::from(b'a')));
    assert_eq!(fseek(file, 0, 1), Ok(()));
    assert_eq!(ftell(file), Ok(0));
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    let _ = fclose(file);
}

#[test]
fn glibc_bug_ftell_after_seek_end() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fseek(file, 0, 2), Ok(()));
    assert_eq!(ftell(file), Ok(6));
    assert_eq!(fseek(file, -2, 2), Ok(()));
    assert_eq!(ftell(file), Ok(4));
    let _ = fclose(file);
}

#[test]
fn openbsd_test_ungetwc_roundtrip() {
    let (state, env) = support::state();
    env.add(b"/f", "é!".as_bytes());
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetwc(file), Ok(0xe9));
    assert_eq!(ungetwc(file, 0xe9), Ok(0xe9));
    assert_eq!(fgetwc(file), Ok(0xe9));
    assert_eq!(fgetwc(file), Ok(u32::from(b'!')));
    assert_eq!(ungetwc(file, u32::MAX), Err(Fail::Eof));
    let _ = fclose(file);
}
