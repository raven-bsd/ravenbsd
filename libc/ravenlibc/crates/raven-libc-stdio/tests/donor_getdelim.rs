// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD `getdelim_test.c` expectations over the crate's `LineBuf`.
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

use raven_libc_stdio::{Fail, LineBuf, fclose, fseek, getdelim, getline};

#[test]
fn freebsd_getdelim_delimiter_in_the_middle() {
    let (state, env) = support::state();
    env.add(b"/f", b"alpha,beta,gamma");
    let file = support::open(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getdelim(file, &mut line, i32::from(b',')), Ok(6));
    assert_eq!(line.line(), b"alpha,");
    assert_eq!(getdelim(file, &mut line, i32::from(b',')), Ok(5));
    assert_eq!(line.line(), b"beta,");
    assert_eq!(getdelim(file, &mut line, i32::from(b',')), Ok(5));
    assert_eq!(line.line(), b"gamma");
    assert_eq!(getdelim(file, &mut line, i32::from(b',')), Err(Fail::Eof));
    let _ = fclose(file);
}

#[test]
fn freebsd_getdelim_partial_last_line() {
    let (state, env) = support::state();
    env.add(b"/f", b"one\ntwo");
    let file = support::open(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getline(file, &mut line), Ok(4));
    assert_eq!(line.line(), b"one\n");
    assert_eq!(getline(file, &mut line), Ok(3));
    assert_eq!(line.line(), b"two");
    assert!(raven_libc_stdio::feof(file));
    let _ = fclose(file);
}

#[test]
fn freebsd_getdelim_delimiter_at_offset_zero() {
    let (state, env) = support::state();
    env.add(b"/f", b"\nabc");
    let file = support::open(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getline(file, &mut line), Ok(1));
    assert_eq!(line.line(), b"\n");
    let _ = fclose(file);
}

#[test]
fn freebsd_getdelim_grows_the_caller_buffer() {
    let (state, env) = support::state();
    let long = b"0123456789abcdefghijklmnopqrstuvwxyz\n".to_vec();
    env.add(b"/f", &long);
    let file = support::open(state, b"/f", b"r");
    let mut data = vec![0u8; 2];
    let mut line = LineBuf::new(&mut data, 2);
    assert_eq!(getline(file, &mut line), Ok(37));
    assert_eq!(line.line(), long.as_slice());
    assert!(line.capacity() >= 38);
    let _ = fclose(file);
}

#[test]
fn freebsd_getdelim_counts_embedded_nul_bytes() {
    let (state, env) = support::state();
    env.add(b"/f", b"a\0b\nc");
    let file = support::open(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getline(file, &mut line), Ok(4));
    assert_eq!(line.line(), b"a\0b\n");
    let _ = fclose(file);
}

#[test]
fn freebsd_getdelim_after_seek() {
    let (state, env) = support::state();
    env.add(b"/f", b"one\ntwo\n");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fseek(file, 4, 0), Ok(()));
    let mut data = Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getline(file, &mut line), Ok(4));
    assert_eq!(line.line(), b"two\n");
    let _ = fclose(file);
}
