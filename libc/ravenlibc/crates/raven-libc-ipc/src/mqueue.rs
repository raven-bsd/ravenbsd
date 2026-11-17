// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mqueue.h` policy: POSIX message-queue names and attributes.
//!
//! No message-queue server exists, so `mq_open`/`mq_unlink` report [`unsupported`] and every
//! operation on a queue descriptor reports [`queue_descriptor_error`] (`EBADF`). The validation
//! here is real, so a caller gets `EINVAL`/`ENAMETOOLONG` for a malformed name before the
//! honest "no queue service" answer.

use raven_libc_abi::errno::Errno;

use crate::consts::{EINVAL, ENAMETOOLONG, ENOSYS, NAME_MAX};

/// `O_NONBLOCK` from `<bits/fcntl.h>`.
pub const O_NONBLOCK: i64 = 0o4000;

/// The answer `mq_open`/`mq_unlink` give while no queue service exists.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// The answer every operation on a message-queue descriptor gives.
#[must_use]
pub const fn queue_descriptor_error() -> Errno {
    crate::consts::EBADF
}

/// Validate a POSIX message-queue name (without its terminating NUL).
///
/// The name must begin with `/`, contain no other `/`, and carry `1..=NAME_MAX` characters
/// after the leading slash.
///
/// # Errors
/// `EINVAL` for a malformed name, `ENAMETOOLONG` when the name part exceeds [`NAME_MAX`].
pub fn validate_name(name: &[u8]) -> Result<(), Errno> {
    let Some((first, rest)) = name.split_first() else {
        return Err(EINVAL);
    };
    if *first != b'/' {
        return Err(EINVAL);
    }
    if rest.is_empty() || rest == b"." || rest == b".." {
        return Err(EINVAL);
    }
    if rest.len() > NAME_MAX {
        return Err(ENAMETOOLONG);
    }
    if rest.contains(&b'/') {
        return Err(EINVAL);
    }
    Ok(())
}

/// Validate an `mq_attr` flag word: only `O_NONBLOCK` is defined.
///
/// # Errors
/// `EINVAL` when an undefined flag bit is set.
pub const fn validate_flags(flags: i64) -> Result<(), Errno> {
    if flags & !O_NONBLOCK == 0 {
        Ok(())
    } else {
        Err(EINVAL)
    }
}

/// Validate the queue geometry of an `mq_attr`.
///
/// # Errors
/// `EINVAL` for a negative `mq_maxmsg`, `mq_msgsize` or `mq_curmsgs`.
pub const fn validate_attr(maxmsg: i64, msgsize: i64, curmsgs: i64) -> Result<(), Errno> {
    if maxmsg < 0 || msgsize < 0 || curmsgs < 0 {
        Err(EINVAL)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "mqueue_ut.rs"]
mod tests;
