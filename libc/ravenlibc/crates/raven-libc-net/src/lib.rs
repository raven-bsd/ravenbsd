// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Sockets over the `net:` scheme, the resolver seams and the interface calls.
//!
//! The crate translates the BSD socket model onto `raven-proto-net` (doc 12 sections 3.1-3.2,
//! 3.7). Every call is explicit about the two worlds: a `Sock` is a kernel handle token, an
//! `Endpoint` is the wire IPv4 endpoint, and the C-ABI pointer conversion is the shell's.
//! All public methods take `&self`; the socket record table is behind a `RefCell` and the shell
//! supplies the process/thread lock.
//!
//! [`NetBackend`], [`SocketFds`] and [`NetEnv`] are the seams. `FakeNetd` implements them on the
//! host; `backend_platform` and `fdglue` adapt C55. The crate is class P
//! (`#![forbid(unsafe_code)]`), `no_std` plus `alloc`, and never calls a clock, an RNG or the
//! network except through a seam.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: all lengths and offsets are slice-checked.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;

use raven_libc_abi::errno::Errno;
use raven_proto_net::{Call, Endpoint, IfaceRequest, IfaceResponse};

pub mod addr;
pub mod backend_platform;
pub mod block;
pub mod cmsg;
pub mod dnsio;
pub mod fake;
pub mod fdglue;
pub mod flags;
pub mod iface;
pub mod ifnames;
pub mod itoa;
pub mod msg;
pub mod netdb;
pub mod opts;
pub mod probe;
pub mod ready;
pub mod record;
pub mod socket;
pub mod unix;

pub use addr::{SockAddr, parse_sockaddr, write_sockaddr};
pub use cmsg::{Cmsg, encode_cmsg, iter_cmsgs, len as cmsg_len, space as cmsg_space};
pub use dnsio::NetDns;
pub use fake::FakeNetd;
pub use iface::{IfAddr, MAX_IFNAME};
pub use ifnames::NetIfNames;
pub use netdb::{getaddrinfo, getnameinfo, make_env};

pub use probe::NetProbe;
pub use record::{SocketRec, SocketState, SocketTable};
pub use socket::{RecvMsg, SendErr};
pub use unix::{NoUnix, UnixBackend};

/// A kernel handle token for one `net:` socket.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sock(pub u32);

/// A completed `READ`: length, optional datagram source, truncation bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadDone {
    /// Number of bytes written into the caller's buffer.
    pub len: usize,
    /// Datagram source; `None` for a connected stream or a connected datagram socket.
    pub from: Option<Endpoint>,
    /// Whether the datagram was larger than the buffer (only with [`Caps::udp_trunc_flag`]).
    pub truncated: bool,
}

/// The non-`Err` reply of a `CALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallReply {
    /// Success without a body.
    Ok,
    /// An endpoint from `GETSOCKNAME`/`GETPEERNAME`.
    Endpoint(Endpoint),
    /// An integer from `GETSOCKOPT`.
    Value(i32),
    /// A newly accepted socket handle.
    Accepted(Sock),
}

/// One entry passed to [`NetBackend::wait`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaitItem {
    /// The socket.
    pub sock: Sock,
    /// Wait for readability.
    pub want_read: bool,
    /// Wait for writability.
    pub want_write: bool,
    /// The socket became readable.
    pub got_read: bool,
    /// The socket became writable.
    pub got_write: bool,
    /// An error condition was reported.
    pub got_err: bool,
    /// The peer closed the connection.
    pub got_hup: bool,
}

/// The netd capabilities the implementation switches on (PG-40/41/42).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    /// PG-40: `OPEN net:tcp`/`net:udp` without an endpoint; `BIND` port 0 picks a port.
    pub bare_open: bool,
    /// PG-41: `READ` flags bit 1 means the datagram was larger than the buffer.
    pub udp_trunc_flag: bool,
    /// PG-41: UDP `CONNECT` fails `ENETUNREACH` without a route.
    pub udp_route_fail: bool,
    /// PG-41: `ACCEPT` carries the peer endpoint.
    pub accept_peer: bool,
    /// PG-41: `READ_PEEK` is available.
    pub peek_flag: bool,
    /// PG-42: `IfaceInfo.index` is stable.
    pub ifindex: bool,
    /// PG-42: IPv6 endpoints (protocol v2).
    pub ipv6: bool,
}

/// The only way the crate reaches netd.
pub trait NetBackend {
    /// The advertised capability switches.
    fn caps(&self) -> Caps;
    /// `OPEN` a `net:` path.
    fn open(&self, path: &str) -> Result<Sock, Errno>;
    /// `READ` at most `buf.len()` bytes.
    fn read(&self, s: Sock, buf: &mut [u8]) -> Result<ReadDone, Errno>;
    /// `READ_PEEK`: inspect the next data without consuming it.
    ///
    /// The default consumes, so a backend without PG-41 must override this only when it
    /// advertises [`Caps::peek_flag`].
    fn peek(&self, s: Sock, buf: &mut [u8]) -> Result<ReadDone, Errno> {
        self.read(s, buf)
    }
    /// `WRITE` one stream segment or datagram.
    fn write(&self, s: Sock, data: &[u8], to: Option<Endpoint>) -> Result<usize, Errno>;
    /// Send a typed `CALL`.
    fn call(&self, s: Sock, c: Call) -> Result<CallReply, Errno>;
    /// Close a socket handle.
    fn close(&self, s: Sock);
    /// Wait for readiness; `Ok(0)` means the timeout expired.
    fn wait(&self, items: &mut [WaitItem], timeout_ms: Option<u64>) -> Result<usize, Errno>;
    /// Send one interface-control request.
    fn iface(&self, req: IfaceRequest) -> Result<IfaceResponse, Errno>;

    /// Send a `LIST` request and collect the whole stream.
    ///
    /// The wire reply is one `Count` followed by that many `Iface` records; the single-reply
    /// `iface` method cannot express it, so implementations that know the stream override this.
    fn iface_list(&self) -> Result<Vec<IfaceResponse>, Errno> {
        let first = self.iface(IfaceRequest::List)?;
        Ok(alloc::vec![first])
    }
}

/// The adapter over the C55 fd table.
pub trait SocketFds {
    /// Install a pending (`None`) or already-open socket and return the libc fd.
    fn install(&self, s: Option<Sock>, cloexec: bool, nonblock: bool) -> Result<i32, Errno>;
    /// Attach the kernel handle to a pending socket.
    fn attach(&self, ofd: u32, s: Sock);
    /// Resolve a libc fd.
    fn resolve(&self, fd: i32) -> Result<FdInfo, Errno>;
    /// The last reference to an OFD went away.
    fn release(&self, ofd: u32);
}

/// One resolved fd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdInfo {
    /// The OFD token used by the socket record table.
    pub ofd: u32,
    /// The kernel handle, absent for a pending socket.
    pub sock: Option<Sock>,
    /// Whether the descriptor has `O_NONBLOCK`.
    pub nonblock: bool,
}

/// The clock, entropy and wall-clock seam.
pub trait NetEnv {
    /// Milliseconds on an arbitrary monotonic origin.
    fn monotonic_ms(&self) -> u64;
    /// Fill `buf`; `false` means no entropy is available.
    fn entropy(&self, buf: &mut [u8]) -> bool;
    /// Seconds since the Unix epoch.
    fn now_secs(&self) -> i64;
}

/// The socket translation object.
pub struct Net<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend = NoUnix> {
    pub(crate) backend: B,
    pub(crate) fds: F,
    pub(crate) env: E,
    pub(crate) unix: Option<U>,
    pub(crate) table: RefCell<SocketTable>,
    pub(crate) dns: RefCell<dnsio::DnsState>,
}

impl<B: NetBackend, F: SocketFds, E: NetEnv> Net<B, F, E, NoUnix> {
    /// Build a net layer without an `AF_UNIX` backend (the on-target default).
    pub fn new(backend: B, fds: F, env: E) -> Self {
        Self::with_unix(backend, fds, env, None)
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Net<B, F, E, U> {
    /// Build a net layer with an optional `AF_UNIX` backend.
    pub fn with_unix(backend: B, fds: F, env: E, unix: Option<U>) -> Self {
        Self {
            backend,
            fds,
            env,
            unix,
            table: RefCell::new(SocketTable::new()),
            dns: RefCell::new(dnsio::DnsState::default()),
        }
    }

    /// The backend capabilities.
    #[must_use]
    pub fn caps(&self) -> Caps {
        self.backend.caps()
    }

    /// The environment seam.
    #[must_use]
    pub fn env(&self) -> &E {
        &self.env
    }

    /// The backend seam.
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The fd seam.
    #[must_use]
    pub fn fds(&self) -> &F {
        &self.fds
    }
}

/// Associated `errno` constants for the ABI newtype.
///
/// `raven-libc-abi` expresses the numbers as plain `i32` constants; the net layer wants the
/// typed `Errno` in matches and `Result`s.
pub trait ErrnoExt {
    /// Invalid argument.
    const EINVAL: Errno;
    /// Address family not supported.
    const EAFNOSUPPORT: Errno;
    /// Bad file descriptor.
    const EBADF: Errno;
    /// Try again.
    const EAGAIN: Errno;
    /// No buffer space.
    const ENOBUFS: Errno;
    /// Message too long.
    const EMSGSIZE: Errno;
    /// Socket is not connected.
    const ENOTCONN: Errno;
    /// Broken pipe.
    const EPIPE: Errno;
    /// Connection reset.
    const ECONNRESET: Errno;
    /// Connection refused.
    const ECONNREFUSED: Errno;
    /// Address not available.
    const EADDRNOTAVAIL: Errno;
    /// Already connected.
    const EISCONN: Errno;
    /// Operation in progress.
    const EINPROGRESS: Errno;
    /// Timed out.
    const ETIMEDOUT: Errno;
    /// Operation not supported.
    const EOPNOTSUPP: Errno;
    /// Protocol option not available.
    const ENOPROTOOPT: Errno;
    /// No such device.
    const ENODEV: Errno;
    /// No such device or address.
    const ENXIO: Errno;
    /// Not a socket.
    const ENOTSOCK: Errno;
    /// Destination address required.
    const EDESTADDRREQ: Errno;
    /// Protocol not supported.
    const EPROTONOSUPPORT: Errno;
    /// Socket type not supported.
    const ESOCKTNOSUPPORT: Errno;
    /// I/O error.
    const EIO: Errno;
    /// Protocol error.
    const EPROTO: Errno;
    /// Function not implemented.
    const ENOSYS: Errno;
    /// Too many open files.
    const EMFILE: Errno;
    /// Bad address.
    const EFAULT: Errno;
    /// No memory.
    const ENOMEM: Errno;
    /// Inappropriate ioctl.
    const ENOTTY: Errno;
    /// Network unreachable.
    const ENETUNREACH: Errno;
}

impl ErrnoExt for Errno {
    const EINVAL: Errno = Errno(raven_libc_abi::generated::errno::EINVAL as u16);
    const EAFNOSUPPORT: Errno = Errno(raven_libc_abi::generated::errno::EAFNOSUPPORT as u16);
    const EBADF: Errno = Errno(raven_libc_abi::generated::errno::EBADF as u16);
    const EAGAIN: Errno = Errno(raven_libc_abi::generated::errno::EAGAIN as u16);
    const ENOBUFS: Errno = Errno(raven_libc_abi::generated::errno::ENOBUFS as u16);
    const EMSGSIZE: Errno = Errno(raven_libc_abi::generated::errno::EMSGSIZE as u16);
    const ENOTCONN: Errno = Errno(raven_libc_abi::generated::errno::ENOTCONN as u16);
    const EPIPE: Errno = Errno(raven_libc_abi::generated::errno::EPIPE as u16);
    const ECONNRESET: Errno = Errno(raven_libc_abi::generated::errno::ECONNRESET as u16);
    const ECONNREFUSED: Errno = Errno(raven_libc_abi::generated::errno::ECONNREFUSED as u16);
    const EADDRNOTAVAIL: Errno = Errno(raven_libc_abi::generated::errno::EADDRNOTAVAIL as u16);
    const EISCONN: Errno = Errno(raven_libc_abi::generated::errno::EISCONN as u16);
    const EINPROGRESS: Errno = Errno(raven_libc_abi::generated::errno::EINPROGRESS as u16);
    const ETIMEDOUT: Errno = Errno(raven_libc_abi::generated::errno::ETIMEDOUT as u16);
    const EOPNOTSUPP: Errno = Errno(raven_libc_abi::generated::errno::EOPNOTSUPP as u16);
    const ENOPROTOOPT: Errno = Errno(raven_libc_abi::generated::errno::ENOPROTOOPT as u16);
    const ENODEV: Errno = Errno(raven_libc_abi::generated::errno::ENODEV as u16);
    const ENXIO: Errno = Errno(raven_libc_abi::generated::errno::ENXIO as u16);
    const ENOTSOCK: Errno = Errno(raven_libc_abi::generated::errno::ENOTSOCK as u16);
    const EDESTADDRREQ: Errno = Errno(raven_libc_abi::generated::errno::EDESTADDRREQ as u16);
    const EPROTONOSUPPORT: Errno = Errno(raven_libc_abi::generated::errno::EPROTONOSUPPORT as u16);
    const ESOCKTNOSUPPORT: Errno = Errno(raven_libc_abi::generated::errno::ESOCKTNOSUPPORT as u16);
    const EIO: Errno = Errno(raven_libc_abi::generated::errno::EIO as u16);
    const EPROTO: Errno = Errno(raven_libc_abi::generated::errno::EPROTO as u16);
    const ENOSYS: Errno = Errno(raven_libc_abi::generated::errno::ENOSYS as u16);
    const EMFILE: Errno = Errno(raven_libc_abi::generated::errno::EMFILE as u16);
    const EFAULT: Errno = Errno(raven_libc_abi::generated::errno::EFAULT as u16);
    const ENOMEM: Errno = Errno(raven_libc_abi::generated::errno::ENOMEM as u16);
    const ENOTTY: Errno = Errno(raven_libc_abi::generated::errno::ENOTTY as u16);
    const ENETUNREACH: Errno = Errno(raven_libc_abi::generated::errno::ENETUNREACH as u16);
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
