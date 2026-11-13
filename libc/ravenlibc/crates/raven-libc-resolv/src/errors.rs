// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getaddrinfo`/`getnameinfo` error codes and their mapping to errno and `h_errno`.

use core::fmt;

use crate::consts::{HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN};

/// The musl/glibc `EAI_*` values (A-net-17).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Eai {
    /// Invalid flags.
    BadFlags = -1,
    /// The name does not resolve.
    NoName = -2,
    /// A temporary failure; try again.
    Again = -3,
    /// A non-recoverable failure.
    Fail = -4,
    /// The name has no usable address.
    NoData = -5,
    /// Unrecognized address family or invalid length.
    Family = -6,
    /// Unrecognized socket type.
    SockType = -7,
    /// Unrecognized service.
    Service = -8,
    /// Out of memory.
    Memory = -10,
    /// A system error.
    System = -11,
    /// The caller's buffer is too small.
    Overflow = -12,
}

impl Eai {
    /// The numeric error code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Map a numeric code; `None` when the value is not one of the known codes.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            -1 => Some(Self::BadFlags),
            -2 => Some(Self::NoName),
            -3 => Some(Self::Again),
            -4 => Some(Self::Fail),
            -5 => Some(Self::NoData),
            -6 => Some(Self::Family),
            -7 => Some(Self::SockType),
            -8 => Some(Self::Service),
            -10 => Some(Self::Memory),
            -11 => Some(Self::System),
            -12 => Some(Self::Overflow),
            _ => None,
        }
    }

    /// The `h_errno` value this error maps to for the legacy `gethostby*` API.
    #[must_use]
    pub const fn to_herror(self) -> i32 {
        match self {
            Self::NoName | Self::NoData => HOST_NOT_FOUND,
            Self::Again => TRY_AGAIN,
            Self::System => NO_RECOVERY,
            _ => NO_RECOVERY,
        }
    }

    /// The errno a legacy `gethostby*_r` call reports for this error.
    #[must_use]
    pub const fn to_errno(self) -> crate::Errno {
        match self {
            Self::NoName | Self::NoData => 0,
            Self::Again => raven_libc_abi::errno::EAGAIN,
            Self::System => raven_libc_abi::errno::EIO,
            _ => raven_libc_abi::errno::EBADMSG,
        }
    }
}

impl fmt::Display for Eai {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(crate::gai::gai_strerror(self.code()))
    }
}

impl From<Eai> for i32 {
    fn from(value: Eai) -> Self {
        value.code()
    }
}

/// Map a `getnameinfo` DNS failure to `h_errno` (RCODE 1 is form error, 2 SERVFAIL, 3 NXDOMAIN,
/// 4 NOTIMP, 5 REFUSED; the legacy mapping is the musl one).
#[must_use]
pub const fn h_errno_from_rcode(rcode: u8) -> i32 {
    match rcode {
        3 => HOST_NOT_FOUND,
        2 => TRY_AGAIN,
        0 => NO_DATA,
        _ => NO_RECOVERY,
    }
}

/// Convert a raw errno to `EAI_SYSTEM`.
#[must_use]
pub const fn eai_from_io(_errno: crate::Errno) -> Eai {
    Eai::System
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod tests;
