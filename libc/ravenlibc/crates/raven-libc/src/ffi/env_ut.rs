// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the shell stdio Env's thread seam.

use super::*;

#[test]
fn tid_fallback_is_nonzero() {
    assert_eq!(nonzero_tid(0), 1);
    assert_eq!(nonzero_tid(1), 1);
    assert_eq!(nonzero_tid(0x1234), 0x1234);
}

#[test]
fn threaded_uses_the_thread_state_gate() {
    assert_eq!(
        ShellEnv.threaded(),
        raven_libc_thread::sync::lock::threaded()
    );
}
