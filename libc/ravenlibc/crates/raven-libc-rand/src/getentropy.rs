// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure `getentropy`/`getrandom` flag logic (A-sys-13, A-sys-14).
//!
//! The syscall wrapper, the `rand:` fallback and the cancellation points belong to C55.

/// Largest single `getentropy` request.
pub const GETENTROPY_MAX: usize = 256;
/// `GRND_NONBLOCK`.
pub const GRND_NONBLOCK: u32 = 1;
/// `GRND_RANDOM`.
pub const GRND_RANDOM: u32 = 2;
/// `GRND_INSECURE`.
pub const GRND_INSECURE: u32 = 4;

/// A random-source error value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandError {
    /// `EIO`, used for an oversized `getentropy` request.
    Io,
    /// `EINVAL`, used for malformed flags.
    Inval,
    /// `EAGAIN`, used for a non-blocking call before the source is ready.
    Again,
}

/// `getentropy` length rule: more than 256 bytes is `EIO`.
pub fn getentropy_check(len: usize) -> Result<(), RandError> {
    if len > GETENTROPY_MAX {
        Err(RandError::Io)
    } else {
        Ok(())
    }
}

/// The decoded `getrandom` flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrndFlags {
    /// `GRND_NONBLOCK`.
    pub nonblock: bool,
    /// `GRND_RANDOM`.
    pub random: bool,
    /// `GRND_INSECURE`.
    pub insecure: bool,
}

/// Decode `getrandom` flags; unknown bits and `random|insecure` are `EINVAL`.
pub fn getrandom_flags(flags: u32) -> Result<GrndFlags, RandError> {
    const KNOWN: u32 = GRND_NONBLOCK | GRND_RANDOM | GRND_INSECURE;
    if flags & !KNOWN != 0 {
        return Err(RandError::Inval);
    }
    if flags & GRND_RANDOM != 0 && flags & GRND_INSECURE != 0 {
        return Err(RandError::Inval);
    }
    Ok(GrndFlags {
        nonblock: flags & GRND_NONBLOCK != 0,
        random: flags & GRND_RANDOM != 0,
        insecure: flags & GRND_INSECURE != 0,
    })
}

/// Clamp one `getrandom` call to 256 bytes; the caller loops for a larger request.
#[must_use]
pub const fn getrandom_len(len: usize) -> usize {
    if len > GETENTROPY_MAX {
        GETENTROPY_MAX
    } else {
        len
    }
}

#[cfg(test)]
#[path = "getentropy_ut.rs"]
mod tests;
