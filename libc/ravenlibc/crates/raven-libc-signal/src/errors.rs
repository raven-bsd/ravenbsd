// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error helpers over `raven_libc_abi::errno::Errno`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno;

/// Build an [`Errno`] from a generated Linux error number.
#[must_use]
pub const fn errno(number: i32) -> Errno {
    Errno(number as u16)
}

/// `EINVAL`.
pub const EINVAL_ERR: Errno = errno(errno::EINVAL);
/// `EAGAIN`.
pub const EAGAIN_ERR: Errno = errno(errno::EAGAIN);
/// `ENOMEM`.
pub const ENOMEM_ERR: Errno = errno(errno::ENOMEM);
/// `ENOSYS`.
pub const ENOSYS_ERR: Errno = errno(errno::ENOSYS);
/// `EPERM`.
pub const EPERM_ERR: Errno = errno(errno::EPERM);
/// `EINTR`.
pub const EINTR_ERR: Errno = errno(errno::EINTR);
/// `ETIMEDOUT`.
pub const ETIMEDOUT_ERR: Errno = errno(errno::ETIMEDOUT);

/// The positive number of an error.
#[must_use]
pub const fn number(error: Errno) -> i32 {
    error.0 as i32
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod errors_ut;
