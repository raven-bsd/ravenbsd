// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! `.mo` lookup tests.

use super::*;

fn builder(entries: &[(&[u8], &[u8])], big: bool) -> Vec<u8> {
    let n = entries.len();
    let header = 20usize;
    let orig = header;
    let trans = orig + 8 * n;
    let strings = trans + 8 * n;
    let mut data = vec![0u8; strings];
    let magic = 0x9504_12deu32;
    let put = |data: &mut Vec<u8>, at: usize, value: u32| {
        let bytes = if big {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        if let Some(slot) = data.get_mut(at..at + 4) {
            slot.copy_from_slice(&bytes);
        }
    };
    put(&mut data, 0, magic);
    put(&mut data, 4, 0);
    put(&mut data, 8, n as u32);
    put(&mut data, 12, orig as u32);
    put(&mut data, 16, trans as u32);
    for (index, (key, value)) in entries.iter().enumerate() {
        let key_at = data.len();
        data.extend_from_slice(key);
        data.push(0);
        let value_at = data.len();
        data.extend_from_slice(value);
        data.push(0);
        put(&mut data, orig + index * 8, key.len() as u32);
        put(&mut data, orig + index * 8 + 4, key_at as u32);
        put(&mut data, trans + index * 8, value.len() as u32);
        put(&mut data, trans + index * 8 + 4, value_at as u32);
    }
    data
}

#[test]
fn little_endian_lookup() {
    let data = builder(&[(b"hello", b"bonjour")], false);
    assert_eq!(mo_lookup(&data, b"hello"), Some(b"bonjour".as_slice()));
    assert_eq!(mo_lookup(&data, b"other"), None);
}

#[test]
fn big_endian_lookup() {
    let data = builder(&[(b"a", b"1"), (b"b", b"2")], true);
    assert_eq!(mo_lookup(&data, b"a"), Some(b"1".as_slice()));
    assert_eq!(mo_lookup(&data, b"b"), Some(b"2".as_slice()));
}

#[test]
fn bad_magic_and_truncation() {
    let mut data = builder(&[(b"hello", b"bonjour")], false);
    data[0] = 0;
    assert_eq!(mo_lookup(&data, b"hello"), None);
    let data = builder(&[(b"hello", b"bonjour")], false);
    for length in 0..data.len() {
        let _ = mo_lookup(data.get(..length).unwrap_or(&[]), b"hello");
    }
}

#[test]
fn plural_translation_is_returned_raw() {
    let data = builder(&[(b"hello", b"one\0many")], false);
    assert_eq!(mo_lookup(&data, b"hello"), Some(b"one\0many".as_slice()));
}
