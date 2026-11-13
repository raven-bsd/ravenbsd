// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SocketFds` over C55's `Platform` fd table.
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::fcntl::O_NONBLOCK;
use raven_libc_platform::backend::SysBackend;
use raven_libc_platform::fd::OfdKind;
use raven_libc_platform::lock::Lock;
use raven_libc_platform::{Handle, Platform};

use crate::{FdInfo, Sock, SocketFds};

/// The C55 fd-table adapter.
///
/// C55 has no "install a pending OFD" primitive, so a lazy-mode `socket()` cannot be installed
/// through this adapter yet; `install(None)` reports `ENOSYS`. Attached sockets work through the
/// ordinary `Platform::install`. This is a QUESTION for C55/C64, recorded in the report.
pub struct FdGlue<'a, B: SysBackend, L: Lock> {
    platform: &'a Platform<B, L>,
}

impl<'a, B: SysBackend, L: Lock> FdGlue<'a, B, L> {
    /// Wrap one platform.
    #[must_use]
    pub fn new(platform: &'a Platform<B, L>) -> Self {
        Self { platform }
    }
}

impl<B: SysBackend, L: Lock> SocketFds for FdGlue<'_, B, L> {
    fn install(&self, s: Option<Sock>, cloexec: bool, nonblock: bool) -> Result<i32, Errno> {
        let sock = s.ok_or(Errno::ENOSYS)?;
        let status = 2u32 | if nonblock { O_NONBLOCK as u32 } else { 0 };
        self.platform
            .install(Handle(sock.0), OfdKind::Socket, status, None, cloexec)
            .map_err(|error| Errno(error.0))
    }

    fn attach(&self, _ofd: u32, _s: Sock) {
        // C55 has no operation that replaces the handle of an OFD; see the type comment.
    }

    fn resolve(&self, fd: i32) -> Result<FdInfo, Errno> {
        let reference = self.platform.get(fd).map_err(|error| Errno(error.0))?;
        if reference.kind() != OfdKind::Socket {
            return Err(Errno::ENOTSOCK);
        }
        Ok(FdInfo {
            ofd: reference.index(),
            sock: Some(Sock(reference.handle().0)),
            nonblock: reference.status() & O_NONBLOCK as u32 != 0,
        })
    }

    fn release(&self, _ofd: u32) {
        // The platform owns the OFD reference count and closes the handle at zero.
    }
}

#[cfg(test)]
#[path = "fdglue_ut.rs"]
mod tests;
