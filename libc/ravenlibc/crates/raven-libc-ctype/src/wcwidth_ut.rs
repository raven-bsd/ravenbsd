// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn fast_path_matches_musl() {
    assert_eq!(wcwidth(0), 0);
    assert_eq!(wcwidth(0x20), 1);
    assert_eq!(wcwidth(0x7e), 1);
    assert_eq!(wcwidth(0x7f), -1);
    assert_eq!(wcwidth(0x9f), -1);
    assert_eq!(wcwidth(0xa0), 1);
    assert_eq!(wcwidth(0xad), 1);
    assert_eq!(wcwidth(0xfe), 1);
}

#[test]
fn plane_rules_and_noncharacters() {
    assert_eq!(wcwidth(0xfffe), -1);
    assert_eq!(wcwidth(0x1fffe), -1);
    assert_eq!(wcwidth(0x20000), 2);
    assert_eq!(wcwidth(0x2a6df), 2);
    assert_eq!(wcwidth(0xe0001), 0);
    assert_eq!(wcwidth(0xe0020), 0);
    assert_eq!(wcwidth(0xe0100), 0);
    assert_eq!(wcwidth(u32::MAX), -1);
}
