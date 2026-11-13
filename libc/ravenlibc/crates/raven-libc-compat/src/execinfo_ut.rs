// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `execinfo` tests with a fake stack: the four stop conditions and the text format.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

const WORD: usize = core::mem::size_of::<usize>();

/// A fake stack: a map from address to word.
struct Stack {
    memory: alloc::collections::BTreeMap<usize, usize>,
}

impl Stack {
    fn new() -> Self {
        Self {
            memory: alloc::collections::BTreeMap::new(),
        }
    }

    fn push(&mut self, fp: usize, next: usize, ret: usize) {
        self.memory.insert(fp, next);
        self.memory.insert(fp.wrapping_add(WORD), ret);
    }
}

#[test]
fn walks_a_chain() {
    let mut stack = Stack::new();
    stack.push(0x1000, 0x2000, 0xaaaa);
    stack.push(0x2000, 0x3000, 0xbbbb);
    stack.push(0x3000, 0, 0xcccc);
    let mut frames = [0usize; 8];
    let count = backtrace(&mut frames, 0x1000, &mut |address| {
        stack.memory.get(&address).copied()
    });
    assert_eq!(count, 3);
    assert_eq!(&frames[..3], &[0xaaaa, 0xbbbb, 0xcccc]);
}

#[test]
fn stops_at_a_null_frame() {
    let mut stack = Stack::new();
    stack.push(0x1000, 0, 0xaaaa);
    let mut frames = [0usize; 8];
    let count = backtrace(&mut frames, 0x1000, &mut |address| {
        stack.memory.get(&address).copied()
    });
    assert_eq!(count, 1);
    assert_eq!(frames[0], 0xaaaa);
}

#[test]
fn stops_at_an_unaligned_frame() {
    let mut stack = Stack::new();
    stack.push(0x1001, 0x2000, 0xaaaa);
    let mut frames = [0usize; 8];
    let count = backtrace(&mut frames, 0x1001, &mut |address| {
        stack.memory.get(&address).copied()
    });
    assert_eq!(count, 0);
}

#[test]
fn stops_at_a_non_increasing_frame() {
    let mut stack = Stack::new();
    stack.push(0x2000, 0x1000, 0xaaaa);
    let mut frames = [0usize; 8];
    let count = backtrace(&mut frames, 0x2000, &mut |address| {
        stack.memory.get(&address).copied()
    });
    // The first frame is stored, then the walk stops because 0x1000 <= 0x2000.
    assert_eq!(count, 1);
}

#[test]
fn stops_at_the_size() {
    let mut stack = Stack::new();
    stack.push(0x1000, 0x2000, 0xaaaa);
    stack.push(0x2000, 0x3000, 0xbbbb);
    stack.push(0x3000, 0, 0xcccc);
    let mut frames = [0usize; 2];
    let count = backtrace(&mut frames, 0x1000, &mut |address| {
        stack.memory.get(&address).copied()
    });
    assert_eq!(count, 2);
}

#[test]
fn unreadable_memory_stops() {
    let mut frames = [0usize; 8];
    let count = backtrace(&mut frames, 0x1000, &mut |_| None);
    assert_eq!(count, 0);
}

#[test]
fn text_format() {
    let mut out = Vec::new();
    format_symbol(&mut out, b"/lib/libc.so.1", b"malloc", 0x2a, 0x7f00);
    assert_eq!(out, b"/lib/libc.so.1(malloc+0x2a) [0x7f00]");
    let mut out = Vec::new();
    format_symbol(&mut out, b"", b"", 0, 0);
    assert_eq!(out, b"(+0x0) [0x0]");
}

#[test]
fn symbols_block_separates_lines() {
    let out = format_symbols(&[0x10, 0x20], b"obj", b"sym", 1);
    assert_eq!(out, b"obj(sym+0x1) [0x10]\nobj(sym+0x1) [0x20]");
}

#[test]
fn hex_has_no_leading_zeros() {
    let mut out = Vec::new();
    format_symbol(&mut out, b"o", b"s", 0x0f, 0x1000);
    assert_eq!(out, b"o(s+0xf) [0x1000]");
}
