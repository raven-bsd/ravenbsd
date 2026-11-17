// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The typed error values and the one size limit this slice owns.
//!
//! Every "not supported" answer is the Linux `ENOSYS` value from `raven-libc-abi`; `EBADF` is
//! used for a message-queue descriptor, because no descriptor can ever name a queue while the
//! kernel has no mqueue primitive.

use raven_libc_abi::errno::Errno;

/// No kernel primitive implements the operation yet.
pub const ENOSYS: Errno = Errno(raven_libc_abi::errno::ENOSYS as u16);
/// An invalid argument value (name, speed, queue selection, flag word).
pub const EINVAL: Errno = Errno(raven_libc_abi::errno::EINVAL as u16);
/// A descriptor that cannot name the requested object.
pub const EBADF: Errno = Errno(raven_libc_abi::errno::EBADF as u16);
/// A required caller pointer was null.
pub const EFAULT: Errno = Errno(raven_libc_abi::errno::EFAULT as u16);
/// A POSIX IPC name was longer than [`NAME_MAX`].
pub const ENAMETOOLONG: Errno = Errno(raven_libc_abi::errno::ENAMETOOLONG as u16);

/// The POSIX IPC name limit used by `mqueue.h` (`NAME_MAX`, without the leading slash).
pub const NAME_MAX: usize = 255;

/// The `errno` number as the C `int` the shell writes.
#[must_use]
pub const fn number(error: Errno) -> i32 {
    error.0 as i32
}

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
