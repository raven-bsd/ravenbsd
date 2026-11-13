// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn nomax_ignores_the_cap() {
    assert_eq!(nomax(b"abcdef", 3), 6);
}

#[test]
fn off1_allows_one_more() {
    assert_eq!(off1(b"abcdef", 3), 4);
    assert_eq!(off1(b"ab\0def", 3), 2);
}
