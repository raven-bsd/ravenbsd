// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared errno mapping for the `<iconv.h>` exports.

use raven_libc_abi::errno::Errno;
use raven_libc_iconv::IconvError;

/// Map a conversion failure to the C errno.
pub(crate) fn set_errno(error: IconvError) {
    let value = match error {
        IconvError::TooBig => 7,
        IconvError::Unsupported | IconvError::Incomplete => 22,
        IconvError::IllegalSequence => 84,
    };
    crate::ffi::errno::set(Errno(value));
}
