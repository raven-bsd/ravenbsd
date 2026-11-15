// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn writes_successive_iovecs() {
    let first = [b'a'; 4];
    let second = [b'b'; 4];
    let mut calls = 0usize;
    let total = writev_impl(vec![&first[..], &second[..]], |buffer| {
        calls += 1;
        Ok(buffer.len())
    })
    .unwrap();
    assert_eq!(total, 8);
    assert_eq!(calls, 2);
}

#[test]
fn a_short_write_stops_the_walk() {
    let first = [b'a'; 4];
    let second = [b'b'; 4];
    let mut calls = 0usize;
    let total = writev_impl(vec![&first[..], &second[..]], |_| {
        calls += 1;
        Ok(2)
    })
    .unwrap();
    assert_eq!(total, 2);
    assert_eq!(calls, 1);
}

#[test]
fn an_error_before_data_is_reported() {
    let first = [b'a'; 4];
    let result = writev_impl(vec![&first[..]], |_| Err(Errno::EIO));
    assert_eq!(result, Err(Errno::EIO));
}

#[test]
fn an_error_after_partial_data_returns_the_count() {
    let first = [b'a'; 4];
    let second = [b'b'; 4];
    let mut calls = 0usize;
    let total = writev_impl(vec![&first[..], &second[..]], |buffer| {
        calls += 1;
        if calls == 1 {
            Ok(buffer.len())
        } else {
            Err(Errno::EIO)
        }
    })
    .unwrap();
    assert_eq!(total, 4);
}

#[test]
fn empty_iovecs_are_skipped() {
    let data = [b'q'; 3];
    let mut calls = 0usize;
    let total = writev_impl(vec![&[], &data[..]], |buffer| {
        calls += 1;
        Ok(buffer.len())
    })
    .unwrap();
    assert_eq!(total, 3);
    assert_eq!(calls, 1);
}
