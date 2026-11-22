// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `AF_UNIX` seam (PG-43).
//!
//! The kernel supplies a duplex endpoint pair (K208); this seam owns the libc-side policy:
//! socket-type flags at pair time, the shutdown selector, and the data/readiness operations that
//! are selected only for `AF_UNIX` records.
use alloc::vec::Vec;

use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;

use crate::{NetBackend, ReadDone, Sock, WaitItem};

/// The kernel-stamped creator credentials of one duplex endpoint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnixCred {
    /// Creator pid.
    pub pid: u64,
    /// Creator real uid.
    pub uid: u32,
    /// Creator real gid.
    pub gid: u32,
}

/// One received AF_UNIX message with its already-marshalled C control buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnixRecv {
    /// Data bytes copied into the caller's iovecs.
    pub len: usize,
    /// C `cmsghdr` records (already installed fds in `SCM_RIGHTS` payloads).
    pub control: Vec<u8>,
}

/// The POSIX `shutdown(2)` selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnixShutdown {
    /// `SHUT_RD`.
    Read,
    /// `SHUT_WR`.
    Write,
    /// `SHUT_RDWR`.
    Both,
}

impl UnixShutdown {
    /// Decode a POSIX selector; an unknown value is `None` (`EINVAL` at the caller).
    #[must_use]
    pub const fn from_posix(how: i32) -> Option<Self> {
        match how {
            0 => Some(Self::Read),
            1 => Some(Self::Write),
            2 => Some(Self::Both),
            _ => None,
        }
    }

    /// The POSIX selector value.
    #[must_use]
    pub const fn as_posix(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Write => 1,
            Self::Both => 2,
        }
    }
}

/// Kernel channel and VFS operations for Unix-domain sockets.
pub trait UnixBackend {
    /// Create an unnamed stream or datagram pair.
    fn pair(&self, ty: i32) -> Result<(Sock, Sock), Errno>;

    /// Create an unnamed pair with the socket-type flags.
    ///
    /// Backends whose pair primitive has no flag word (the fake and `NoUnix`) inherit this
    /// default; the target backend adds `SOCK_DGRAM`/`SOCK_NONBLOCK`/`SOCK_CLOEXEC` bits.
    fn pair_flags(&self, ty: i32, _nonblock: bool, _cloexec: bool) -> Result<(Sock, Sock), Errno> {
        self.pair(ty)
    }

    /// Bind a path.
    fn bind(&self, path: &[u8]) -> Result<Sock, Errno>;

    /// Connect to a bound path.
    fn connect(&self, path: &[u8]) -> Result<Sock, Errno>;

    /// Read one endpoint.
    ///
    /// The default uses the backend's raw `READ`; a target backend overrides this only if it has
    /// a different data path. The fake keeps the same default so the host tests exercise the
    /// routing without a kernel.
    fn unix_read(
        &self,
        backend: &dyn NetBackend,
        sock: Sock,
        buf: &mut [u8],
    ) -> Result<ReadDone, Errno> {
        backend.read(sock, buf)
    }

    /// Peek one endpoint without consuming it.
    fn unix_peek(
        &self,
        backend: &dyn NetBackend,
        sock: Sock,
        buf: &mut [u8],
    ) -> Result<ReadDone, Errno> {
        backend.peek(sock, buf)
    }

    /// Write one endpoint.
    fn unix_write(
        &self,
        backend: &dyn NetBackend,
        sock: Sock,
        data: &[u8],
    ) -> Result<usize, Errno> {
        backend.write(sock, data, None)
    }

    /// Shut down the read, write or both directions.
    fn shutdown(&self, _sock: Sock, _how: UnixShutdown) -> Result<(), Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    /// Wait for readability or writability.
    fn unix_wait(
        &self,
        backend: &dyn NetBackend,
        items: &mut [WaitItem],
        timeout_ms: Option<u64>,
    ) -> Result<usize, Errno> {
        backend.wait(items, timeout_ms)
    }

    /// Send data plus a C `msghdr` control buffer.
    ///
    /// The target backend parses the C `SCM_RIGHTS` records, resolves each descriptor to a kernel
    /// handle and calls the K210 control send. The fake has no hardware path and rejects control.
    fn unix_sendmsg<B: NetBackend + ?Sized>(
        &self,
        _backend: &B,
        _sock: Sock,
        _data: &[u8],
        _control: &[u8],
        _flags: i32,
    ) -> Result<usize, Errno> {
        Err(Errno::EOPNOTSUPP)
    }

    /// Receive one message plus its C control records.
    ///
    /// The target backend installs the kernel-returned handles as descriptors with `adopt`, then
    /// returns a ready-to-copy C `cmsghdr` buffer. `control_capacity` is the caller's
    /// `msg_controllen`.
    fn unix_recvmsg<B: NetBackend + ?Sized>(
        &self,
        _backend: &B,
        _sock: Sock,
        _data: &mut [u8],
        _control_capacity: usize,
        _flags: i32,
    ) -> Result<UnixRecv, Errno> {
        Err(Errno::EOPNOTSUPP)
    }

    /// The kernel-stamped credentials of the connected endpoint.
    fn peercred(&self, _sock: Sock) -> Result<UnixCred, Errno> {
        Err(Errno::EOPNOTSUPP)
    }
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
