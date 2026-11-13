// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::Ordering;

#[test]
fn build_flag_matches_the_feature() {
    assert_eq!(ENTROPY_WEAK_BUILD, cfg!(feature = "weak-entropy-interim"));
}

#[cfg(feature = "weak-entropy-interim")]
#[test]
fn default_build_is_weak() {
    assert!(entropy_is_weak());
}

#[cfg(not(feature = "weak-entropy-interim"))]
#[test]
fn no_feature_build_is_not_weak() {
    assert!(!ENTROPY_WEAK_BUILD);
}

#[test]
fn runtime_flag_sets_weakness() {
    mark_entropy_weak();
    assert!(entropy_is_weak());
}

#[test]
fn gate_flag_is_visible_to_the_example() {
    let value = entropy_is_weak();
    assert_eq!(
        value,
        ENTROPY_WEAK_BUILD || RUNTIME_WEAK.load(Ordering::SeqCst)
    );
}
