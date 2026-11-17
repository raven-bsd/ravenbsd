// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `stropts.h` policy: STREAMS are not present on RavenBSD.
//!
//! `isastream` is a real query with a truthful answer: no descriptor is a STREAMS device, so a
//! valid descriptor answers "not a stream". The data-transfer calls (`putmsg`, `getmsg`,
//! `putpmsg`, `getpmsg`) and the mount helpers (`fattach`, `fdetach`) validate their arguments
//! and then report [`unsupported`].

use raven_libc_abi::errno::Errno;

use crate::consts::{EBADF, EFAULT, EINVAL, ENOSYS};

/// The answer the STREAMS data-transfer and mount entry points give.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// Whether `fd` is a STREAMS device. RavenBSD has no STREAMS, so only the descriptor check can
/// fail.
///
/// # Errors
/// `EBADF` when `fd` is negative.
pub const fn isastream(fd: i32) -> Result<bool, Errno> {
    if fd < 0 { Err(EBADF) } else { Ok(false) }
}

/// Validate one `struct strbuf` (`maxlen`, `len`, and whether `buf` is non-null).
///
/// # Errors
/// `EINVAL` for a negative `maxlen`, a negative `len` or `len > maxlen`; `EFAULT` when `len` is
/// non-zero and the buffer is null.
pub const fn validate_strbuf(maxlen: i32, len: i32, has_buffer: bool) -> Result<(), Errno> {
    if maxlen < 0 || len < 0 {
        return Err(EINVAL);
    }
    if len > maxlen {
        return Err(EINVAL);
    }
    if len > 0 && !has_buffer {
        return Err(EFAULT);
    }
    Ok(())
}

#[cfg(test)]
#[path = "stropts_ut.rs"]
mod tests;
