// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn lock_round_trip() {
    let lock = SpinLock::new();
    assert!(!lock.is_held());
    lock.acquire();
    assert!(lock.is_held());
    lock.release();
    assert!(!lock.is_held());
}
