// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sleep` tests.

use raven_abi::errno::Errno;

use crate::backend::{FakeSys, Fault};
use crate::platform::Platform;
use crate::time::sleep;

#[test]
fn zero_seconds_does_not_call_the_kernel() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(sleep(&platform, 0), Ok(0));
    assert!(platform.backend().sleeps().is_empty());
}

#[test]
fn one_second_is_one_billion_nanoseconds() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(sleep(&platform, 1), Ok(0));
    assert_eq!(platform.backend().sleeps(), alloc::vec![1_000_000_000]);
}

#[test]
fn interrupted_sleep_returns_the_full_request_until_pg_17() {
    let fake = FakeSys::new();
    fake.fail_next(Fault::Nanosleep, Errno::EINTR);
    let platform = Platform::new(fake);
    assert_eq!(sleep(&platform, 7), Ok(7));
}

#[test]
fn large_requests_do_not_overflow() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    assert_eq!(sleep(&platform, 5), Ok(0));
    assert_eq!(platform.backend().sleeps(), alloc::vec![5_000_000_000]);
}
