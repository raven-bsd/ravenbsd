// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strmode;
use raven_libc_abi::stat::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFREG, S_IFSOCK};

fn expected(mode: u32) -> [u8; 12] {
    let kind = mode & 0o170000;
    let kind_byte = match kind {
        0o040000 => b'd',
        0o020000 => b'c',
        0o060000 => b'b',
        0o100000 => b'-',
        0o120000 => b'l',
        0o140000 => b's',
        0o010000 => b'p',
        _ => b'?',
    };
    let bit = |mask: u32| mode & mask != 0;
    let triple = |read: u32, write: u32, execute: u32, special: u32, special_char: (u8, u8)| {
        let r = if bit(read) { b'r' } else { b'-' };
        let w = if bit(write) { b'w' } else { b'-' };
        let x = if bit(execute) && bit(special) {
            special_char.1
        } else if bit(execute) {
            b'x'
        } else if bit(special) {
            special_char.0
        } else {
            b'-'
        };
        [r, w, x]
    };
    let mut out = [0u8; 12];
    out[0] = kind_byte;
    out[1..4].copy_from_slice(&triple(0o400, 0o200, 0o100, 0o4000, (b'S', b's')));
    out[4..7].copy_from_slice(&triple(0o040, 0o020, 0o010, 0o2000, (b'S', b's')));
    out[7..10].copy_from_slice(&triple(0o004, 0o002, 0o001, 0o1000, (b'T', b't')));
    out[10] = b' ';
    out[11] = 0;
    out
}

#[test]
fn all_permission_patterns_for_every_file_type() {
    let kinds = [
        S_IFDIR, S_IFCHR, S_IFBLK, S_IFREG, S_IFLNK, S_IFSOCK, S_IFIFO,
    ];
    for kind in kinds {
        for permission in 0u32..4096 {
            let mode = kind | permission;
            let mut out = [0xAAu8; 12];
            strmode(mode, &mut out);
            assert_eq!(out, expected(mode), "mode {mode:#o}");
        }
    }
}

#[test]
fn unknown_file_type_is_question_mark() {
    let mut out = [0u8; 12];
    strmode(0o777, &mut out);
    assert_eq!(out[0], b'?');
    assert_eq!(&out[..11], b"?rwxrwxrwx ");
}

#[test]
fn mode_3_vectors() {
    let mut out = [0u8; 12];
    strmode(0o100644, &mut out);
    assert_eq!(&out[..11], b"-rw-r--r-- ");
    strmode(0o040755, &mut out);
    assert_eq!(&out[..11], b"drwxr-xr-x ");
    strmode(0o120777, &mut out);
    assert_eq!(&out[..11], b"lrwxrwxrwx ");
}

#[test]
fn special_bits() {
    let mut out = [0u8; 12];
    strmode(0o104755, &mut out);
    assert_eq!(&out[..11], b"-rwsr-xr-x ");
    strmode(0o104644, &mut out);
    assert_eq!(&out[..11], b"-rwSr--r-- ");
    strmode(0o102755, &mut out);
    assert_eq!(&out[..11], b"-rwxr-sr-x ");
    strmode(0o102645, &mut out);
    assert_eq!(&out[..11], b"-rw-r-Sr-x ");
    strmode(0o101777, &mut out);
    assert_eq!(&out[..11], b"-rwxrwxrwt ");
    strmode(0o101776, &mut out);
    assert_eq!(&out[..11], b"-rwxrwxrwT ");
}
