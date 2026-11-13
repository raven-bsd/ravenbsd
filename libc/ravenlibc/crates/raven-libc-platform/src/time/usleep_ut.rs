// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `usleep` tests.

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::time::usleep;

#[test]
fn zero_microseconds() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(usleep(&platform, 0), Ok(()));
    assert_eq!(platform.backend().sleeps(), alloc::vec![0]);
}

#[test]
fn one_microsecond() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(usleep(&platform, 1), Ok(()));
    assert_eq!(platform.backend().sleeps(), alloc::vec![1_000]);
}

#[test]
fn nine_hundred_ninety_nine_microseconds() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(usleep(&platform, 999_999), Ok(()));
    assert_eq!(platform.backend().sleeps(), alloc::vec![999_999_000]);
}
