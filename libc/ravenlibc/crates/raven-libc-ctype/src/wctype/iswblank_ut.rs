// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn only_ascii_space_and_tab() {
    assert!(iswblank(0x20));
    assert!(iswblank(0x09));
    assert!(!iswblank(0x0a));
    assert!(!iswblank(0xa0));
    assert!(!iswblank(0x3000));
}
