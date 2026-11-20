// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Resume-frame request checks.

use super::*;

#[test]
fn accepts_an_aligned_resume_frame() {
    assert_eq!(check_request(0x1000, 0x7fff_fff0, 0), Ok(()));
}

#[test]
fn rejects_a_zero_entry_or_stack() {
    assert_eq!(check_request(0, 0x7fff_fff0, 0), Err(AbiErrno::EINVAL));
    assert_eq!(check_request(0x1000, 0, 0), Err(AbiErrno::EINVAL));
}

#[test]
fn rejects_a_misaligned_stack() {
    assert_eq!(check_request(0x1000, 0x7fff_fff8, 0), Err(AbiErrno::EINVAL));
}

#[test]
fn rejects_fork_flags() {
    assert_eq!(check_request(0x1000, 0x7fff_fff0, 1), Err(AbiErrno::EINVAL));
}
