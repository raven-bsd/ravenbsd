// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `waitpid` selector mapping.

use raven_libc_proc::consts::{P_ALL, P_PGID, P_PID};

use super::wait_id;

#[test]
fn waitpid_ids_match_posix() {
    assert_eq!(wait_id(-1, 42), (P_ALL, 0));
    assert_eq!(wait_id(0, 42), (P_PGID, 42));
    assert_eq!(wait_id(-7, 42), (P_PGID, 7));
    assert_eq!(wait_id(7, 42), (P_PID, 7));
}

#[test]
fn waitpid_min_i32_does_not_overflow_the_group_id() {
    let (kind, id) = wait_id(i32::MIN, 0);
    assert_eq!(kind, P_PGID);
    assert_eq!(id, i64::from(i32::MAX) + 1);
}
