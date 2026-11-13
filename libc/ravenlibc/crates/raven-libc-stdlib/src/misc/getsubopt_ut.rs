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

//! `getsubopt` from the BSD manual examples.

use super::*;

#[test]
fn comma_list_with_values() {
    let mut options = b"ro,br,onnx,opt=val\0".to_vec();
    let tokens: [&[u8]; 3] = [b"ro", b"br", b"opt"];
    let first = getsubopt(0, &mut options, &tokens);
    assert_eq!(first.index, Some(0));
    assert_eq!(first.token, Some((0, 2)));
    assert_eq!(first.next, 3);
    let second = getsubopt(first.next, &mut options, &tokens);
    assert_eq!(second.index, Some(1));
    assert_eq!(second.token, Some((3, 2)));
    assert_eq!(second.next, 6);
    let third = getsubopt(second.next, &mut options, &tokens);
    assert_eq!(third.index, None);
    assert_eq!(third.token, Some((6, 4)));
    let fourth = getsubopt(third.next, &mut options, &tokens);
    assert_eq!(fourth.index, Some(2));
    assert_eq!(fourth.token, Some((11, 3)));
    assert_eq!(fourth.value, Some((15, 3)));
    assert_eq!(&options[15..18], b"val");
    assert_eq!(fourth.next, 18);
    let fifth = getsubopt(fourth.next, &mut options, &tokens);
    assert_eq!(fifth.token, None);
    assert_eq!(fifth.index, None);
}

#[test]
fn skips_spaces_and_tabs_and_commas() {
    let mut options = b"  ,\tro\t, br=1\0".to_vec();
    let tokens: [&[u8]; 2] = [b"ro", b"br"];
    let first = getsubopt(0, &mut options, &tokens);
    assert_eq!(first.index, Some(0));
    let second = getsubopt(first.next, &mut options, &tokens);
    assert_eq!(second.index, Some(1));
    assert_eq!(second.value, Some((12, 1)));
}

#[test]
fn empty_and_terminated_lists() {
    let mut empty = b"\0".to_vec();
    assert_eq!(
        getsubopt(0, &mut empty, &[]),
        Subopt {
            index: None,
            token: None,
            value: None,
            next: 0
        }
    );
    let mut end = b"ro\0".to_vec();
    let first = getsubopt(0, &mut end, &[b"ro"]);
    assert_eq!(first.index, Some(0));
    let done = getsubopt(first.next, &mut end, &[b"ro"]);
    assert_eq!(done.token, None);
}

#[test]
fn leading_equals_is_token_text() {
    let mut options = b"=x,opt=y\0".to_vec();
    let first = getsubopt(0, &mut options, &[b"opt"]);
    assert_eq!(first.index, None);
    assert_eq!(first.token, Some((0, 2)));
    assert_eq!(first.value, None);
}

#[test]
fn double_equals() {
    let mut options = b"=,opt=y\0".to_vec();
    let first = getsubopt(0, &mut options, &[b"opt"]);
    assert_eq!(first.token, Some((0, 1)));
    assert_eq!(first.next, 2);
}

#[test]
fn tabs_are_separators() {
    let mut options = b"opt\t=\tvalue\0".to_vec();
    let result = getsubopt(0, &mut options, &[b"opt"]);
    assert_eq!(result.token, Some((0, 3)));
}

#[test]
fn empty_value() {
    let mut options = b"opt=\0".to_vec();
    let result = getsubopt(0, &mut options, &[b"opt"]);
    assert_eq!(result.value, Some((4, 0)));
}
