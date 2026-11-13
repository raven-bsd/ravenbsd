// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn no_entropy_never_returns_weak_data() {
    let result = std::panic::catch_unwind(|| NoEntropy.no_entropy());
    assert!(result.is_err());
}

#[test]
fn trait_is_object_safe() {
    let platform: &dyn RandPlatform = &NoEntropy;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| platform.no_entropy()));
    assert!(result.is_err());
}
