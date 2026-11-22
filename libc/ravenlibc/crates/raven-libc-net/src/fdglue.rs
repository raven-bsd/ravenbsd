// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SocketFds` over C55's `Platform` fd table.

use alloc::vec::Vec;

use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::fcntl::O_NONBLOCK;
use raven_libc_platform::backend::SysBackend;
use raven_libc_platform::fd::{InheritedSocket, OfdKind, SocketMeta};
use raven_libc_platform::lock::Lock;
use raven_libc_platform::{Handle, Platform};

use crate::{FdInfo, Sock, SocketFds};

/// The C55 fd-table adapter.
///
/// A lazy-mode `socket()` installs a pending socket OFD through
/// [`raven_libc_platform::Platform::install_pending`]; `bind`/`connect`/`listen` later attaches
/// the scheme handle through [`raven_libc_platform::Platform::attach_handle`].
///
/// Donor divergence (recorded for review): NetBSD `kern_descrip.c:fd_allocfile`/`fd_affix` and
/// `uipc_socket.c:socreate` create the socket first and publish a `file_t` whose `f_socket` is
/// already set; there is no handle-less descriptor. Raven's scheme ABI needs the fd before the
/// scheme path is known, so the pending OFD has `Handle::NONE` until `attach`. The donor
/// `closef`/`fd_close` reference-drop invariant is kept: one OFD, one handle, close at zero refs.
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
        let status = 2u32 | if nonblock { O_NONBLOCK as u32 } else { 0 };
        match s {
            Some(sock) => {
                let fd = self
                    .platform
                    .install(Handle(sock.0), OfdKind::Socket, status, None, cloexec)
                    .map_err(|error| Errno(error.0))?;
                if let Ok(info) = self.resolve(fd) {
                    self.event(info.ofd, sock);
                }
                Ok(fd)
            }
            None => self
                .platform
                .install_pending(OfdKind::Socket, status, None, cloexec)
                .map_err(|error| Errno(error.0)),
        }
    }

    fn attach(&self, ofd: u32, s: Sock) {
        // The event is attached after bind/listen/connect: the scheme rejects `Call::Event`
        // before the socket exists, and a second attach would replace the event the fd polls.
        let _ = self.platform.attach_handle(ofd, Handle(s.0));
    }

    fn event(&self, ofd: u32, s: Sock) {
        // The OFD is the authority after fork_child_inherit; the passed Sock may be the parent's
        // handle, which the server has already released (K409 EBADF).
        let handle = self.platform.ofd_handle(ofd).unwrap_or(Handle(s.0));
        if let Ok(event) =
            crate::backend_platform::event_handle(self.platform.backend(), Sock(handle.0))
        {
            let _ = self.platform.set_ofd_event(ofd, event);
        }
    }
    fn resolve(&self, fd: i32) -> Result<FdInfo, Errno> {
        let reference = self.platform.get(fd).map_err(|error| Errno(error.0))?;
        if reference.kind() != OfdKind::Socket {
            return Err(Errno::ENOTSOCK);
        }
        let handle = reference.handle();
        Ok(FdInfo {
            ofd: reference.index(),
            sock: (handle != Handle::NONE).then_some(Sock(handle.0)),
            nonblock: reference.status() & O_NONBLOCK as u32 != 0,
        })
    }

    fn release(&self, _ofd: u32) {
        // The platform owns the OFD reference count and closes the handle at zero.
    }

    fn inherited(&self) -> Vec<InheritedSocket> {
        self.platform.inherited_sockets()
    }

    fn set_meta(&self, ofd: u32, meta: SocketMeta) {
        self.platform.set_ofd_socket_meta(ofd, meta);
    }
}

#[cfg(test)]
#[path = "fdglue_ut.rs"]
mod tests;
