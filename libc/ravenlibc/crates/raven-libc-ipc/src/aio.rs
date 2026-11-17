// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `aio.h` policy: constants, request validation and the typed unsupported answer.
//!
//! RavenBSD has no asynchronous-I/O primitive. `aio_read`/`aio_write`/... therefore validate
//! what they can (a non-negative list count, a known opcode) and then report
//! [`unsupported`]; they never claim a request was queued.

use raven_libc_abi::errno::Errno;

use crate::consts::{EINVAL, ENOSYS};

/// The request completed (or was cancelled) successfully.
pub const AIO_CANCELED: i32 = 0;
/// The request could not be cancelled.
pub const AIO_NOTCANCELED: i32 = 1;
/// Every request had already completed.
pub const AIO_ALLDONE: i32 = 2;

/// `lio_listio` opcode: read.
pub const LIO_READ: i32 = 0;
/// `lio_listio` opcode: write.
pub const LIO_WRITE: i32 = 1;
/// `lio_listio` opcode: no operation.
pub const LIO_NOP: i32 = 2;

/// `lio_listio` mode: wait for completion.
pub const LIO_WAIT: i32 = 0;
/// `lio_listio` mode: return immediately.
pub const LIO_NOWAIT: i32 = 1;

/// The answer every AIO entry point gives while the kernel has no AIO primitive.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// Validate a `lio_listio` list length.
///
/// # Errors
/// `EINVAL` when `count` is negative.
pub const fn validate_count(count: i32) -> Result<(), Errno> {
    if count < 0 { Err(EINVAL) } else { Ok(()) }
}

/// Validate an `aio_lio_opcode` value.
///
/// # Errors
/// `EINVAL` when `opcode` is not one of `LIO_READ`, `LIO_WRITE` or `LIO_NOP`.
pub const fn validate_opcode(opcode: i32) -> Result<(), Errno> {
    if opcode == LIO_READ || opcode == LIO_WRITE || opcode == LIO_NOP {
        Ok(())
    } else {
        Err(EINVAL)
    }
}

/// Validate a `lio_listio` mode.
///
/// # Errors
/// `EINVAL` when `mode` is not `LIO_WAIT` or `LIO_NOWAIT`.
pub const fn validate_mode(mode: i32) -> Result<(), Errno> {
    if mode == LIO_WAIT || mode == LIO_NOWAIT {
        Ok(())
    } else {
        Err(EINVAL)
    }
}

#[cfg(test)]
#[path = "aio_ut.rs"]
mod tests;
