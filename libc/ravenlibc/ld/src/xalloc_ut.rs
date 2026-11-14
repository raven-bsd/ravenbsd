// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::ldalloc::LdAlloc;

#[test]
fn xmalloc_returns_an_aligned_offset() {
    let mut alloc = LdAlloc::new();
    let at = xmalloc(&mut alloc, 24).expect("malloc");
    assert_eq!(at % 16, 0);
}

#[test]
fn xcalloc_rejects_an_overflow() {
    let mut alloc = LdAlloc::new();
    assert!(xcalloc(&mut alloc, usize::MAX, 2).is_none());
}

#[test]
fn xstrdup_allocates_the_nul() {
    let mut alloc = LdAlloc::new();
    let at = xstrdup(&mut alloc, b"abc").expect("strdup");
    assert!(at < crate::ldalloc::BOOTSTRAP_BYTES);
}
