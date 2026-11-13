// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `AF_UNIX` seam (PG-43).
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;

use crate::Sock;

/// Kernel channel and VFS operations for Unix-domain sockets.
pub trait UnixBackend {
    /// Create an unnamed stream or datagram pair.
    fn pair(&self, ty: i32) -> Result<(Sock, Sock), Errno>;
    /// Bind a path.
    fn bind(&self, path: &[u8]) -> Result<Sock, Errno>;
    /// Connect to a bound path.
    fn connect(&self, path: &[u8]) -> Result<Sock, Errno>;
}

/// The default: PG-43 is absent, so every method returns `EAFNOSUPPORT`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoUnix;

impl UnixBackend for NoUnix {
    fn pair(&self, _ty: i32) -> Result<(Sock, Sock), Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn bind(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn connect(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }
}

#[cfg(test)]
#[path = "unix_ut.rs"]
mod tests;
