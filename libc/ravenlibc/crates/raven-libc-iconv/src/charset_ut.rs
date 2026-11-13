// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Charset lookup tests.

use super::*;

#[test]
fn debug_charset_lookup() {
    for name in [
        b"ISO-8859-1".as_slice(),
        b"latin1",
        b"UTF-8",
        b"ASCII",
        b"ISO-8859-2",
    ] {
        let resolved = resolve(name);
        println!("{name:?} -> {resolved:?}");
        if let Some((kind, offset)) = resolved {
            println!(
                "  type_code={} legacy_e9={:x}",
                type_code(offset),
                legacy_map(offset, 0xe9)
            );
            let _ = kind;
        }
    }
}
