// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn fills_successive_iovecs() {
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut calls = 0usize;
    let total = readv_impl(vec![&mut first[..], &mut second[..]], |buffer| {
        calls += 1;
        if calls == 1 {
            buffer.fill(b'a');
            Ok(buffer.len())
        } else {
            buffer.get_mut(..2).unwrap().fill(b'b');
            Ok(2)
        }
    })
    .unwrap();
    assert_eq!(total, 6);
    assert_eq!(first, [b'a'; 4]);
    assert_eq!(second, [b'b', b'b', 0, 0]);
}

#[test]
fn a_short_read_stops_the_walk() {
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut calls = 0usize;
    let total = readv_impl(vec![&mut first[..], &mut second[..]], |buffer| {
        calls += 1;
        buffer[0] = b'x';
        Ok(1)
    })
    .unwrap();
    assert_eq!(total, 1);
    assert_eq!(calls, 1);
    assert_eq!(second, [0u8; 4]);
}

#[test]
fn an_error_before_data_is_reported() {
    let mut first = [0u8; 4];
    let result = readv_impl(vec![&mut first[..]], |_| Err(Errno::EIO));
    assert_eq!(result, Err(Errno::EIO));
}

#[test]
fn an_error_after_partial_data_returns_the_count() {
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut calls = 0usize;
    let total = readv_impl(vec![&mut first[..], &mut second[..]], |buffer| {
        calls += 1;
        if calls == 1 {
            buffer.fill(b'z');
            Ok(buffer.len())
        } else {
            Err(Errno::EIO)
        }
    })
    .unwrap();
    assert_eq!(total, 4);
    assert_eq!(first, [b'z'; 4]);
}

#[test]
fn empty_iovecs_are_skipped() {
    let mut data = [0u8; 3];
    let mut calls = 0usize;
    let total = readv_impl(vec![&mut [], &mut data[..]], |buffer| {
        calls += 1;
        buffer.fill(b'q');
        Ok(buffer.len())
    })
    .unwrap();
    assert_eq!(total, 3);
    assert_eq!(calls, 1);
    assert_eq!(data, [b'q'; 3]);
}
