// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! EINTR, restart and cancellation wrapper tests.

use raven_abi::errno::Errno;

use crate::intr::{
    CANCELLATION_POINTS, CLASS_N, CLASS_R, CallClass, class_of, close, deadline_from_now,
    deadline_remaining_ns, is_cancellation_point, run,
};

#[test]
fn restart_classes() {
    assert_eq!(class_of("read"), Some(CallClass::Restartable));
    assert_eq!(class_of("write"), Some(CallClass::Restartable));
    assert_eq!(class_of("connect"), Some(CallClass::Restartable));
    assert_eq!(class_of("poll"), Some(CallClass::NeverRestarted));
    assert_eq!(class_of("nanosleep"), Some(CallClass::NeverRestarted));
    assert_eq!(class_of("close"), Some(CallClass::Close));
    assert_eq!(class_of("not_a_syscall"), None);
}

#[test]
fn class_lists_are_disjoint() {
    for name in CLASS_R {
        assert!(!CLASS_N.contains(name), "{name} is in both classes");
    }
}

#[test]
fn restartable_retries_with_permission() {
    let mut attempts = 0;
    let result = run(CallClass::Restartable, true, || {
        attempts += 1;
        if attempts < 3 {
            Err(Errno::EINTR)
        } else {
            Ok(42)
        }
    });
    assert_eq!(result, Ok(42));
    assert_eq!(attempts, 3);
}

#[test]
fn restartable_stops_without_permission() {
    let mut attempts = 0;
    let result = run(CallClass::Restartable, false, || {
        attempts += 1;
        Err::<(), _>(Errno::EINTR)
    });
    assert_eq!(result, Err(Errno::EINTR));
    assert_eq!(attempts, 1);
}

#[test]
fn class_n_never_restarts() {
    let mut attempts = 0;
    let result = run(CallClass::NeverRestarted, true, || {
        attempts += 1;
        Err::<(), _>(Errno::EINTR)
    });
    assert_eq!(result, Err(Errno::EINTR));
    assert_eq!(attempts, 1);
}

#[test]
fn non_eintr_errors_pass_through() {
    let result = run(CallClass::Restartable, true, || Err::<(), _>(Errno::EBADF));
    assert_eq!(result, Err(Errno::EBADF));
}

#[test]
fn close_treats_eintr_as_success() {
    assert_eq!(close(|| Err(Errno::EINTR)), Ok(()));
    assert_eq!(close(|| Err(Errno::EIO)), Err(Errno::EIO));
    assert_eq!(close(|| Ok(())), Ok(()));
}

#[test]
fn deadline_math() {
    assert_eq!(deadline_from_now(100, 1000), 1100);
    assert_eq!(deadline_remaining_ns(1100, 1000), 100);
    assert_eq!(deadline_remaining_ns(1000, 1000), 0);
    assert_eq!(deadline_remaining_ns(1000, 1200), 0);
}

#[test]
fn cancellation_points_include_the_posix_set() {
    for name in [
        "read",
        "write",
        "open",
        "close",
        "poll",
        "select",
        "nanosleep",
        "waitpid",
        "connect",
    ] {
        assert!(is_cancellation_point(name), "{name}");
    }
    assert!(!is_cancellation_point("strlen"));
    assert!(CANCELLATION_POINTS.len() >= 55);
}

#[test]
fn restart_wanted_is_false_without_hooks() {
    assert!(!crate::intr::restart_wanted());
}
