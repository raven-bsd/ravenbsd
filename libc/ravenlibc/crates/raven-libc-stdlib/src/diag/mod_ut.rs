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

//! The libgen group over all `{a,/,.,..}`-style paths of length up to 6.

use super::*;
use crate::diag::{MESSAGE_CAPACITY, ProgName};

fn materialize(path: &[u8], result: GenName) -> Vec<u8> {
    match result {
        GenName::Static(text) => text.to_vec(),
        GenName::Slice { start, len } => path
            .get(start..start.wrapping_add(len))
            .unwrap_or(&[])
            .to_vec(),
    }
}

#[test]
fn generated_paths_stay_within_the_buffer() {
    let alphabet = *b"a/.";
    let mut checked = 0usize;
    for number in 0usize..3usize.pow(6) {
        let mut value = number;
        let mut path = Vec::new();
        for _ in 0..6 {
            if value == 0 && !path.is_empty() {
                break;
            }
            path.push(alphabet[value % alphabet.len()]);
            value /= alphabet.len();
            if value == 0 {
                break;
            }
        }
        for length in 0..=path.len() {
            let prefix = &path[..length];
            let base = materialize(prefix, basename(prefix));
            let dir = materialize(prefix, dirname(prefix));
            assert!(!base.is_empty());
            assert!(!dir.is_empty());
            checked += 1;
        }
    }
    assert!(checked > 1000);
}

#[test]
fn genname_variants_are_distinct() {
    assert_ne!(GenName::Static(b"."), GenName::Slice { start: 0, len: 0 });
}
#[test]
fn message_capacity_is_512() {
    assert_eq!(MESSAGE_CAPACITY, 512);
}

#[test]
fn progname_and_libgen_are_reexported() {
    let mut prog = ProgName::new();
    prog.set(b"/x/y");
    assert_eq!(prog.get(), b"y");
    assert_eq!(basename(b"a/b"), GenName::Slice { start: 2, len: 1 });
}

#[test]
fn static_and_slice_are_distinct() {
    let path = *b"a/b";
    let result = basename(&path);
    assert!(matches!(result, GenName::Slice { .. }));
    let empty: [u8; 0] = [];
    assert_eq!(basename(&empty), GenName::Static(b"."));
}

#[test]
fn dirname_dot_is_static() {
    assert_eq!(dirname(b"plain"), GenName::Static(b"."));
}
