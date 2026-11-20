// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn conversion_clamps_at_i32_max() {
    assert_eq!(from_soft(1024), 1024);
    assert_eq!(from_soft(u32::MAX), i32::MAX);
}
