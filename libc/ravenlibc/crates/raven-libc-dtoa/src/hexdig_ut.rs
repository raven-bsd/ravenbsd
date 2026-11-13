// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;

#[test]
fn table_matches_hexdig_init() {
    assert_eq!(HEXDIG[b'0' as usize], 0x10);
    assert_eq!(HEXDIG[b'9' as usize], 0x19);
    assert_eq!(HEXDIG[b'a' as usize], 0x1a);
    assert_eq!(HEXDIG[b'f' as usize], 0x1f);
    assert_eq!(HEXDIG[b'A' as usize], 0x1a);
    assert_eq!(HEXDIG[b'F' as usize], 0x1f);
    assert_eq!(HEXDIG[b'g' as usize], 0);
    assert_eq!(HEXDIG[b' ' as usize], 0);
    assert_eq!(HEXDIG[0], 0);
    let mut count = 0;
    for value in HEXDIG {
        if value != 0 {
            count += 1;
        }
    }
    assert_eq!(count, 22);
}
