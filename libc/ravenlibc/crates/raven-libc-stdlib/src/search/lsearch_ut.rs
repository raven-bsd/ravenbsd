// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `lsearch`/`lfind` vectors and the checked append offset.

use super::*;

fn cmp(a: &[u8], b: &[u8]) -> i32 {
    match a.cmp(b) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[test]
fn find_all_elements() {
    let base = b"alpha\0beta \0gamma\0";
    let count = 3usize;
    for (index, key) in [b"alpha\0".as_slice(), b"beta \0", b"gamma\0"]
        .iter()
        .enumerate()
    {
        assert_eq!(lfind(key, base, count, 6, &mut cmp), Some(index));
    }
    assert_eq!(lfind(b"delta\0", base, count, 6, &mut cmp), None);
    assert_eq!(count, 3);
}

#[test]
fn append_returns_the_new_index() {
    let mut base = vec![0u8; 4 * 4];
    let mut count = 0usize;
    assert_eq!(lsearch(b"one\0", &mut base, &mut count, 4, &mut cmp), Ok(0));
    assert_eq!(lsearch(b"two\0", &mut base, &mut count, 4, &mut cmp), Ok(1));
    assert_eq!(lsearch(b"one\0", &mut base, &mut count, 4, &mut cmp), Ok(0));
    assert_eq!(count, 2);
    assert_eq!(&base[..8], b"one\0two\0");
}

#[test]
fn no_space_is_refused() {
    let mut base = vec![0u8; 4];
    let mut count = 0usize;
    assert_eq!(lsearch(b"one\0", &mut base, &mut count, 4, &mut cmp), Ok(0));
    assert_eq!(
        lsearch(b"two\0", &mut base, &mut count, 4, &mut cmp),
        Err(LSearchError::NoSpace)
    );
    assert_eq!(count, 1);
}

#[test]
fn zero_width_is_invalid() {
    let mut base = vec![0u8; 4];
    let mut count = 0usize;
    assert_eq!(
        lsearch(b"x", &mut base, &mut count, 0, &mut cmp),
        Err(LSearchError::Inval)
    );
    assert_eq!(lfind(b"x", &base, 1, 0, &mut cmp), None);
}

#[test]
fn overflow_is_checked() {
    let mut base = vec![0u8; 8];
    let mut count = usize::MAX;
    assert_eq!(
        lsearch(b"x\0", &mut base, &mut count, 8, &mut cmp),
        Err(LSearchError::Inval)
    );
}

#[test]
fn lfind_returns_first_duplicate() {
    let base = b"aa\0aa\0";
    let mut count = 2usize;
    assert_eq!(lfind(b"aa\0", base, count, 3, &mut cmp), Some(0));
    let _ = &mut count;
}

#[test]
fn lsearch_appends_duplicate_only_when_absent() {
    let mut base = vec![0u8; 12];
    let mut count = 0usize;
    lsearch(b"a\0\0\0", &mut base, &mut count, 4, &mut cmp).unwrap();
    lsearch(b"a\0\0\0", &mut base, &mut count, 4, &mut cmp).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn lfind_width_beyond_buffer() {
    let base = b"abc";
    assert_eq!(lfind(b"a", base, 2, 8, &mut cmp), None);
}

#[test]
fn lsearch_key_shorter_than_width() {
    let mut base = vec![0u8; 8];
    let mut count = 0usize;
    assert_eq!(
        lsearch(b"a", &mut base, &mut count, 4, &mut cmp),
        Err(LSearchError::Inval)
    );
}
