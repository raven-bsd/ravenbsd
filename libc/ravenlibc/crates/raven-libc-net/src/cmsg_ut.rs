// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cmsghdr` tests.

use super::*;

#[test]
fn alignment_values() {
    assert_eq!(align(0), 0);
    assert_eq!(align(1), 8);
    assert_eq!(align(4), 8);
    assert_eq!(align(8), 8);
    assert_eq!(align(9), 16);
}

#[test]
fn space_and_len_values() {
    assert_eq!(len(0), 16);
    assert_eq!(space(0), 16);
    assert_eq!(space(1), 24);
    assert_eq!(space(4), 24);
    assert_eq!(space(8), 24);
    assert_eq!(space(9), 32);
    assert_eq!(len(4), 20);
}

#[test]
fn encode_and_iterate() {
    let mut buffer = [0u8; 32];
    let used = encode_cmsg(1, 2, &[9, 8, 7, 6], &mut buffer).expect("encode");
    assert_eq!(used, 20);
    let mut iter = iter_cmsgs(&buffer[..24]);
    let first = iter.next().expect("one").expect("ok");
    assert_eq!(first.level, 1);
    assert_eq!(first.ty, 2);
    assert_eq!(first.data, &[9, 8, 7, 6]);
    assert!(iter.next().is_none());
}

#[test]
fn encode_into_too_small_buffer() {
    let mut buffer = [0u8; 16];
    assert_eq!(encode_cmsg(1, 2, &[1], &mut buffer), None);
}

#[test]
fn iterate_rejects_short_and_overrun() {
    let mut short = [0u8; 16];
    short[0] = 15;
    let mut iter = iter_cmsgs(&short);
    assert_eq!(iter.next(), Some(Err(Errno::EINVAL)));

    let mut long = [0u8; 16];
    long[0] = 24;
    let mut iter = iter_cmsgs(&long);
    assert_eq!(iter.next(), Some(Err(Errno::EINVAL)));
}

#[test]
fn iterate_two_messages() {
    let mut buffer = [0u8; 48];
    let first = encode_cmsg(1, 1, &[1, 2, 3, 4], &mut buffer).expect("first");
    let second = encode_cmsg(1, 2, &[5], buffer.get_mut(24..).expect("tail")).expect("second");
    assert_eq!(first, 20);
    assert_eq!(second, 17);
    let messages: Vec<_> = iter_cmsgs(&buffer[..41])
        .map(|item| item.expect("ok"))
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].ty, 1);
    assert_eq!(messages[1].ty, 2);
}
