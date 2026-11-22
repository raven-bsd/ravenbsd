// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `waitpid` pid selection.

use super::*;

#[test]
fn positive_pid_selects_one_child() {
    assert_eq!(wait_target(9), (P_PID, 9));
}

#[test]
fn minus_one_selects_any_child() {
    assert_eq!(wait_target(-1), (KERNEL_P_ALL, 0));
}

#[test]
fn zero_selects_the_calling_group() {
    assert_eq!(wait_target(0), (P_PGID, 0));
}

#[test]
fn negative_pid_selects_the_absolute_group() {
    assert_eq!(wait_target(-7), (P_PGID, 7));
}
