// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn no_descriptor_is_a_stream() {
    assert_eq!(isastream(0), Ok(false));
    assert_eq!(isastream(7), Ok(false));
    assert_eq!(isastream(-1), Err(EBADF));
}

#[test]
fn strbufs_are_bounded() {
    assert!(validate_strbuf(0, 0, false).is_ok());
    assert!(validate_strbuf(16, 16, true).is_ok());
    assert_eq!(validate_strbuf(-1, 0, false), Err(EINVAL));
    assert_eq!(validate_strbuf(16, -1, true), Err(EINVAL));
    assert_eq!(validate_strbuf(16, 17, true), Err(EINVAL));
    assert_eq!(validate_strbuf(16, 1, false), Err(EFAULT));
}

#[test]
fn streams_calls_are_not_faked() {
    assert_eq!(unsupported(), crate::consts::ENOSYS);
}
