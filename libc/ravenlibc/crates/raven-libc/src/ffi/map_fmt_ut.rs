// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `FmtErr` table: every variant of C43 4.6.

use super::*;
use crate::ffi::errno;

#[test]
fn invalid_is_einval() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(apply(FmtErr::Invalid), -1);
    assert_eq!(errno::get(), 22);
}

#[test]
fn no_mem_is_enomem() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(apply(FmtErr::NoMem), -1);
    assert_eq!(errno::get(), 12);
}

#[test]
fn overflow_is_eoverflow() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(apply(FmtErr::Overflow), -1);
    assert_eq!(errno::get(), 75);
}

#[test]
fn encoding_is_eilseq() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(apply(FmtErr::Encoding), -1);
    assert_eq!(errno::get(), 84);
}

#[test]
fn a_sink_failure_keeps_the_write_errno() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(5);
    assert_eq!(apply(FmtErr::Sink), -1);
    assert_eq!(errno::get(), 5);
}

#[test]
fn pct_n_abort_is_einval_at_the_shell_boundary() {
    let _guard = crate::ffi::TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(apply(FmtErr::PctNAbort), -1);
    assert_eq!(errno::get(), 22);
}

#[test]
fn a_missing_float_hook_fails_the_call() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(apply(FmtErr::FloatUnavailable), -1);
}
