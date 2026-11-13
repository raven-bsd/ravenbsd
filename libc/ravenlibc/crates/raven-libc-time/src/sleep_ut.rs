// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Sleep tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::sys::testing::FakeSys;
use raven_libc_abi::time::{CLOCK_MONOTONIC, CLOCK_REALTIME};

fn ts(sec: i64, nsec: i64) -> Timespec {
    Timespec {
        tv_sec: sec,
        tv_nsec: nsec,
    }
}

#[test]
fn a_plain_sleep_succeeds_and_advances_the_fake() {
    let sys = FakeSys::epoch();
    assert_eq!(nanosleep(&sys, &ts(1, 0)), Ok(()));
    assert_eq!(sys.sleeps.get(), 1);
    assert_eq!(sys.clock.get().tv_sec, 1);
}

#[test]
fn invalid_specs_are_rejected() {
    let sys = FakeSys::epoch();
    assert_eq!(
        nanosleep(&sys, &ts(0, -1)).expect_err("negative").err,
        errno(EINVAL)
    );
    assert_eq!(
        nanosleep(&sys, &ts(0, 1_000_000_000))
            .expect_err("too large")
            .err,
        errno(EINVAL)
    );
    assert_eq!(
        nanosleep(&sys, &ts(-1, 0)).expect_err("negative sec").err,
        errno(EINVAL)
    );
}

#[test]
fn an_eintr_leaves_the_remaining_time() {
    let sys = FakeSys::epoch();
    sys.push_sleep(Err(errno(EINTR)));
    let error = nanosleep(&sys, &ts(1, 0)).expect_err("eintr");
    assert_eq!(error.err, errno(EINTR));
    assert_eq!(error.rem, ts(1, 0));
}

#[test]
fn a_scripted_clock_advance_is_subtracted_from_the_remainder() {
    let sys = FakeSys::epoch();
    sys.push_sleep(Err(errno(EINTR)));
    sys.eintr_advance.set(300_000_000);
    let error = nanosleep(&sys, &ts(1, 0)).expect_err("eintr");
    assert_eq!(error.rem, ts(0, 700_000_000));
}

#[test]
fn a_huge_request_saturates() {
    let sys = FakeSys::epoch();
    sys.push_sleep(Err(errno(EINTR)));
    let error = nanosleep(&sys, &ts(i64::MAX, 0)).expect_err("eintr");
    assert_eq!(error.err, errno(EINTR));
}

#[test]
fn an_absolute_deadline_in_the_past_returns_at_once() {
    let sys = FakeSys::new(ts(10, 0));
    assert_eq!(
        clock_nanosleep(&sys, CLOCK_REALTIME, true, &ts(5, 0)),
        Ok(())
    );
    assert_eq!(sys.sleeps.get(), 0);
}

#[test]
fn an_absolute_deadline_in_the_future_sleeps_until_it() {
    let sys = FakeSys::epoch();
    assert_eq!(
        clock_nanosleep(&sys, CLOCK_MONOTONIC, true, &ts(3, 0)),
        Ok(())
    );
    assert_eq!(sys.clock.get().tv_sec, 3);
}

#[test]
fn an_absolute_eintr_returns_the_remaining_deadline() {
    let sys = FakeSys::epoch();
    sys.push_sleep(Err(errno(EINTR)));
    let error = clock_nanosleep(&sys, CLOCK_MONOTONIC, true, &ts(2, 0)).expect_err("eintr");
    assert_eq!(error.err, errno(EINTR));
    assert_eq!(error.rem, ts(2, 0));
}

#[test]
fn cpu_clock_ids_are_rejected() {
    let sys = FakeSys::epoch();
    assert_eq!(
        clock_nanosleep(&sys, 2, false, &ts(1, 0))
            .expect_err("cpu")
            .err,
        errno(EINVAL)
    );
}
