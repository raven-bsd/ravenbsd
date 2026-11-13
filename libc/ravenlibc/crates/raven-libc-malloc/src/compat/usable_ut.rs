// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn usable_must_cover_the_request() {
    assert!(usable_for(16, 16));
    assert!(usable_for(32, 16));
    assert!(!usable_for(15, 16));
}
