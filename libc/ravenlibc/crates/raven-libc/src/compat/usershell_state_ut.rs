// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Regression tests for the lazily allocated `getusershell` string buffer.

use super::*;

#[test]
fn the_shell_buffer_is_one_stable_allocation() {
    let first = shell_buffer().as_mut_ptr();
    let second = shell_buffer().as_mut_ptr();
    assert!(!first.is_null());
    assert_eq!(first, second);
}

#[test]
fn the_buffer_starts_zeroed() {
    assert!(shell_buffer().iter().all(|byte| *byte == 0));
}
