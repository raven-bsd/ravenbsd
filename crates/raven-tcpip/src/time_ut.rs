// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the time wrappers.

use super::{Duration, Instant};

#[test]
fn instant_arithmetic_saturates() {
    let now = Instant::from_millis(1000);
    assert_eq!(
        now.checked_add(Duration::from_millis(500)),
        Some(Instant::from_millis(1500))
    );
    assert_eq!(
        Instant::from_millis(i64::MAX).checked_add(Duration::from_millis(1)),
        None
    );
    assert_eq!(
        Instant::from_millis(i64::MAX).saturating_add(Duration::from_millis(1)),
        Instant::from_millis(i64::MAX)
    );
    assert_eq!(
        Instant::from_millis(100).saturating_since(Instant::from_millis(400)),
        Duration::ZERO
    );
}
