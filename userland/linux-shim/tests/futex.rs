// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Futex operation tests.

mod common;

use common::MockHost;
use linux_shim::futex::futex;
use raven_abi::errno::Errno;
use raven_abi::types::Timespec;
use raven_linux_abi::flags::futex as flags;

const ADDR: usize = 0x1000;

fn ts(sec: i64, nsec: i64) -> Timespec {
    Timespec {
        tv_sec: sec,
        tv_nsec: nsec,
    }
}

fn wait(host: &mut MockHost, value: u32, timeout: Option<Timespec>) -> Result<isize, Errno> {
    futex(host, ADDR, flags::FUTEX_WAIT, value, timeout, 0, 0)
}

#[test]
fn wait_mismatch_is_eagain() {
    let mut host = MockHost::new();
    host.word(ADDR, 5);
    assert_eq!(wait(&mut host, 4, None), Err(Errno::EAGAIN));
    assert_eq!(host.waiters.get(&ADDR), None);
}

#[test]
fn wait_match_queues_and_wake_returns_the_count() {
    let mut host = MockHost::new();
    host.word(ADDR, 7);
    assert_eq!(wait(&mut host, 7, None), Ok(0));
    assert_eq!(wait(&mut host, 7, None), Ok(0));
    assert_eq!(wait(&mut host, 7, None), Ok(0));
    assert_eq!(host.waiters.get(&ADDR), Some(&3));
    assert_eq!(
        futex(&mut host, ADDR, flags::FUTEX_WAKE, 2, None, 0, 0),
        Ok(2)
    );
    assert_eq!(
        futex(&mut host, ADDR, flags::FUTEX_WAKE, 5, None, 0, 0),
        Ok(1)
    );
    assert_eq!(
        futex(&mut host, ADDR, flags::FUTEX_WAKE, 5, None, 0, 0),
        Ok(0)
    );
}

#[test]
fn private_flag_is_ignored_for_the_operation() {
    let mut host = MockHost::new();
    host.word(ADDR, 3);
    assert_eq!(
        futex(
            &mut host,
            ADDR,
            flags::FUTEX_WAIT | flags::FUTEX_PRIVATE_FLAG,
            3,
            None,
            0,
            0
        ),
        Ok(0)
    );
}

#[test]
fn unaligned_address_is_einval() {
    assert_eq!(
        futex(&mut host(), ADDR + 1, flags::FUTEX_WAKE, 1, None, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn unsupported_operations_are_enosys() {
    for operation in [
        flags::FUTEX_REQUEUE,
        flags::FUTEX_CMP_REQUEUE,
        flags::FUTEX_WAKE_OP,
        flags::FUTEX_LOCK_PI,
    ] {
        assert_eq!(
            futex(&mut host(), ADDR, operation, 1, None, 0, 0),
            Err(Errno::ENOSYS)
        );
    }
}

#[test]
fn bitset_wait_and_wake_require_a_nonzero_mask() {
    assert_eq!(
        futex(&mut host(), ADDR, flags::FUTEX_WAIT_BITSET, 1, None, 0, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        futex(&mut host(), ADDR, flags::FUTEX_WAKE_BITSET, 1, None, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn relative_timeout_is_converted_to_nanoseconds() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    assert_eq!(wait(&mut host, 1, Some(ts(1, 2))), Ok(0));
    assert_eq!(host.last_timeout, Some(Some(1_000_000_002)));
}

#[test]
fn negative_or_invalid_relative_timeout_is_einval() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    assert_eq!(wait(&mut host, 1, Some(ts(-1, 0))), Err(Errno::EINVAL));
    assert_eq!(wait(&mut host, 1, Some(ts(1, -1))), Err(Errno::EINVAL));
    assert_eq!(
        wait(&mut host, 1, Some(ts(1, 1_000_000_000))),
        Err(Errno::EINVAL)
    );
}

#[test]
fn absolute_bitset_timeout_uses_the_monotonic_clock_by_default() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    host.clock = (100, 0);
    assert_eq!(
        futex(
            &mut host,
            ADDR,
            flags::FUTEX_WAIT_BITSET,
            1,
            Some(ts(101, 5)),
            0,
            1
        ),
        Ok(0)
    );
    assert_eq!(host.last_clock, Some(1));
    assert_eq!(host.last_timeout, Some(Some(1_000_000_005)));
}

#[test]
fn absolute_bitset_timeout_honours_clock_realtime() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    host.clock = (100, 0);
    assert_eq!(
        futex(
            &mut host,
            ADDR,
            flags::FUTEX_WAIT_BITSET | flags::FUTEX_CLOCK_REALTIME,
            1,
            Some(ts(101, 5)),
            0,
            1
        ),
        Ok(0)
    );
    assert_eq!(host.last_clock, Some(0));
}

#[test]
fn absolute_timeout_in_the_past_is_etimedout() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    host.clock = (100, 0);
    assert_eq!(
        futex(
            &mut host,
            ADDR,
            flags::FUTEX_WAIT_BITSET,
            1,
            Some(ts(99, 0)),
            0,
            1
        ),
        Err(Errno::ETIMEDOUT)
    );
}

#[test]
fn bitset_wait_without_timeout_uses_no_deadline() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    assert_eq!(
        futex(&mut host, ADDR, flags::FUTEX_WAIT_BITSET, 1, None, 0, 1),
        Ok(0)
    );
    assert_eq!(host.last_timeout, Some(None));
}

#[test]
fn bitset_wake_returns_the_woken_count() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    for _ in 0..3 {
        assert_eq!(
            futex(&mut host, ADDR, flags::FUTEX_WAIT_BITSET, 1, None, 0, 1),
            Ok(0)
        );
    }
    assert_eq!(
        futex(&mut host, ADDR, flags::FUTEX_WAKE_BITSET, 2, None, 0, 1),
        Ok(2)
    );
}

#[test]
fn zero_wake_count_is_reported() {
    assert_eq!(
        futex(&mut host(), ADDR, flags::FUTEX_WAKE, 0, None, 0, 0),
        Ok(0)
    );
}

#[test]
fn large_relative_timeout_overflows_to_einval() {
    let mut host = MockHost::new();
    host.word(ADDR, 1);
    assert_eq!(
        wait(&mut host, 1, Some(ts(i64::MAX, 0))),
        Err(Errno::EINVAL)
    );
}

fn host() -> MockHost {
    MockHost::new()
}
