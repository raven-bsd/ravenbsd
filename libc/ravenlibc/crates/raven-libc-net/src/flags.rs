// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MSG_*` validation and socket-type flags.
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{
    MSG_CMSG_CLOEXEC, MSG_CONFIRM, MSG_CTRUNC, MSG_DONTROUTE, MSG_DONTWAIT, MSG_EOR, MSG_MORE,
    MSG_NOSIGNAL, MSG_OOB, MSG_PEEK, MSG_TRUNC, MSG_WAITALL, SOCK_CLOEXEC, SOCK_DGRAM,
    SOCK_NONBLOCK, SOCK_STREAM,
};

/// Every `MSG_*` bit the crate understands.
pub const KNOWN: i32 = MSG_OOB
    | MSG_PEEK
    | MSG_DONTROUTE
    | MSG_CTRUNC
    | MSG_TRUNC
    | MSG_DONTWAIT
    | MSG_EOR
    | MSG_WAITALL
    | MSG_NOSIGNAL
    | MSG_MORE
    | MSG_CONFIRM
    | MSG_CMSG_CLOEXEC as i32;

/// The descriptor flags packed into the `socket(domain, type, proto)` type argument.
pub const SOCK_FLAGS: i32 = SOCK_CLOEXEC | SOCK_NONBLOCK;

/// Reject bits outside [`KNOWN`].
pub fn validate(flags: i32) -> Result<(), Errno> {
    if flags & !KNOWN != 0 {
        Err(Errno::EINVAL)
    } else {
        Ok(())
    }
}

/// Split a socket type into `(type & 0xf, cloexec, nonblock)`.
pub fn split_type(ty: i32) -> Result<(i32, bool, bool), Errno> {
    if ty & !(0xf | SOCK_FLAGS) != 0 {
        return Err(Errno::EINVAL);
    }
    let base = ty & 0xf;
    if base != SOCK_STREAM && base != SOCK_DGRAM {
        return Err(Errno::ESOCKTNOSUPPORT);
    }
    Ok((base, ty & SOCK_CLOEXEC != 0, ty & SOCK_NONBLOCK != 0))
}

/// Whether a `sendmsg` flag combination needs the socket layer to perform the write.
pub fn send_supported(flags: i32) -> Result<(), Errno> {
    validate(flags)?;
    if flags & (MSG_OOB | MSG_MORE | MSG_CONFIRM) != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    Ok(())
}

/// Whether a `recvmsg` flag combination can be served by today's backend.
pub fn recv_supported(flags: i32) -> Result<(), Errno> {
    validate(flags)?;
    if flags & MSG_OOB != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    if flags & (MSG_CTRUNC | MSG_TRUNC) != 0 {
        // Output-only bits; accepting them from the caller is harmless.
    }
    Ok(())
}

#[cfg(test)]
#[path = "flags_ut.rs"]
mod tests;
