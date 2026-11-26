// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;
use raven_libc_abi::errno;

#[test]
fn numbers_match_the_abi() {
    assert_eq!(ENOSYS, Errno(errno::ENOSYS as u16));
    assert_eq!(EINVAL, Errno(errno::EINVAL as u16));
    assert_eq!(EBADF, Errno(errno::EBADF as u16));
    assert_eq!(EFAULT, Errno(errno::EFAULT as u16));
    assert_eq!(ENAMETOOLONG, Errno(errno::ENAMETOOLONG as u16));
}

#[test]
fn number_is_the_positive_c_value() {
    // FreeBSD errno.h: ENOSYS is 78 (DR-69). Linux's 38 is ENOTSOCK.
    assert_eq!(number(ENOSYS), 78);
    assert_eq!(number(EINVAL), 22);
    assert_eq!(NAME_MAX, 255);
}
