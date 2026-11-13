// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The socket translation layer.
use crate::ErrnoExt;

use alloc::vec;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{
    AF_INET, AF_INET6, AF_UNIX, MSG_DONTWAIT, MSG_NOSIGNAL, MSG_PEEK, MSG_TRUNC, MSG_WAITALL,
    SO_ERROR, SOCK_CLOEXEC, SOCK_DGRAM, SOCK_NONBLOCK, SOCK_STREAM, SOL_SOCKET,
};
use raven_proto_net::{Call, Endpoint, Shutdown as WireShutdown, SockOpt};

use crate::addr::{SockAddr, parse_sockaddr, write_sockaddr};
use crate::block::Deadline;
use crate::flags;
use crate::msg::{gather, total_len};
use crate::opts::{self, RecordOption};
use crate::ready::{self, PollInterest};
use crate::record::{SocketRec, SocketState};
use crate::{CallReply, Caps, Net, NetBackend, NetEnv, Sock, SocketFds, UnixBackend, WaitItem};

/// Returned by `recvmsg`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecvMsg {
    /// Bytes received.
    pub len: usize,
    /// Full source-address length (POSIX truncation rule).
    pub name_len: usize,
    /// Bytes of control data.
    pub control_len: usize,
    /// Output `MSG_*` flags.
    pub flags: i32,
}

/// The `sendmsg` error, with the shell's SIGPIPE decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendErr {
    /// The errno to report.
    pub errno: Errno,
    /// Whether the shell must raise `SIGPIPE`.
    pub raise_sigpipe: bool,
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Net<B, F, E, U> {
    /// `socket`.
    pub fn socket(&self, domain: i32, ty: i32, proto: i32) -> Result<i32, Errno> {
        let (base, cloexec, nonblock) = flags::split_type(ty)?;
        if domain != AF_INET && domain != AF_INET6 {
            return Err(Errno::EAFNOSUPPORT);
        }
        if domain == AF_INET6 && !self.backend.caps().ipv6 {
            return Err(Errno::EAFNOSUPPORT);
        }
        validate_proto(base, proto)?;
        let caps = self.backend.caps();
        let (handle, state) = if caps.bare_open {
            let mut buffer = [0u8; 16];
            let path = crate::msg::bare_path(base, &mut buffer)?;
            (Some(self.backend.open(path)?), SocketState::Open)
        } else {
            (None, SocketState::Pending)
        };
        let fd = self.fds.install(handle, cloexec, nonblock)?;
        let info = self.fds.resolve(fd)?;
        let mut rec = SocketRec::new(domain, base, proto, state, fd);
        rec.sock = handle;
        rec.nonblock = nonblock || info.nonblock;
        if let Err(error) = self.table.borrow_mut().insert(info.ofd, rec) {
            if let Some(handle) = handle {
                self.backend.close(handle);
            }
            self.fds.release(info.ofd);
            return Err(error);
        }
        Ok(fd)
    }

    /// `socketpair(AF_UNIX, ...)`.
    pub fn socketpair(&self, domain: i32, ty: i32, proto: i32) -> Result<[i32; 2], Errno> {
        if domain != AF_UNIX {
            return Err(Errno::EAFNOSUPPORT);
        }
        let (base, cloexec, nonblock) = flags::split_type(ty)?;
        if proto != 0 {
            return Err(Errno::EPROTONOSUPPORT);
        }
        let unix = self.unix.as_ref().ok_or(Errno::EAFNOSUPPORT)?;
        let (left, right) = unix.pair(base)?;
        let first = self.install_pair_socket(base, left, cloexec, nonblock)?;
        let second = match self.install_pair_socket(base, right, cloexec, nonblock) {
            Ok(fd) => fd,
            Err(error) => {
                self.close_hook(self.fds.resolve(first)?.ofd);
                return Err(error);
            }
        };
        Ok([first, second])
    }

    fn install_pair_socket(
        &self,
        ty: i32,
        sock: Sock,
        cloexec: bool,
        nonblock: bool,
    ) -> Result<i32, Errno> {
        let fd = self.fds.install(Some(sock), cloexec, nonblock)?;
        let info = self.fds.resolve(fd)?;
        let mut rec = SocketRec::new(AF_UNIX, ty, 0, SocketState::Connected, fd);
        rec.sock = Some(sock);
        rec.nonblock = nonblock || info.nonblock;
        self.table.borrow_mut().insert(info.ofd, rec)?;
        Ok(fd)
    }

    /// `bind`.
    pub fn bind(&self, fd: i32, addr: &[u8]) -> Result<(), Errno> {
        let info = self.fds.resolve(fd)?;
        let parsed = parse_sockaddr(addr)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if rec.local.is_some() {
            return Err(Errno::EINVAL);
        }
        check_family(rec, &parsed)?;
        if matches!(parsed, SockAddr::Un { .. }) {
            return Err(Errno::EAFNOSUPPORT);
        }
        let endpoint = parsed.to_endpoint().ok_or(Errno::EAFNOSUPPORT)?;
        if self.backend.caps().bare_open {
            let sock = rec.sock.ok_or(Errno::EBADF)?;
            self.backend.call(sock, Call::Bind(endpoint))?;
            let mut local = endpoint;
            if endpoint.port == 0 {
                match self.backend.call(sock, Call::GetSockName) {
                    Ok(CallReply::Endpoint(chosen)) => local = chosen,
                    _ => return Err(Errno::EADDRNOTAVAIL),
                }
            }
            rec.local = Some(local);
            rec.state = SocketState::Bound;
            return Ok(());
        }
        if endpoint.port == 0 {
            rec.bound_port0 = true;
        }
        rec.local = Some(endpoint);
        rec.state = SocketState::Bound;
        Ok(())
    }

    /// `listen`.
    pub fn listen(&self, fd: i32, backlog: i32) -> Result<(), Errno> {
        let info = self.fds.resolve(fd)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if rec.ty != SOCK_STREAM {
            return Err(Errno::EOPNOTSUPP);
        }
        if !matches!(rec.state, SocketState::Bound | SocketState::Open) {
            return Err(Errno::EINVAL);
        }
        let local = rec.local.ok_or(Errno::EINVAL)?;
        if local.port == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let backlog = clamp_backlog(backlog)?;
        if rec.sock.is_none() {
            let mut buffer = [0u8; 64];
            let path = crate::msg::socket_path(rec.ty, local, &mut buffer)?;
            let sock = self.backend.open(path)?;
            self.fds.attach(info.ofd, sock);
            rec.sock = Some(sock);
            if let Err(error) = apply_wire(&self.backend, sock, &mut rec.wire) {
                self.backend.close(sock);
                rec.sock = None;
                return Err(error);
            }
        }
        let sock = rec.sock.ok_or(Errno::EBADF)?;
        self.backend.call(sock, Call::Listen { backlog })?;
        rec.state = SocketState::Listening;
        rec.accepting = true;
        Ok(())
    }

    /// `accept`/`accept4`.
    pub fn accept(
        &self,
        fd: i32,
        addr_out: &mut [u8],
        flags_in: i32,
    ) -> Result<(i32, usize), Errno> {
        if flags_in & !(SOCK_CLOEXEC | SOCK_NONBLOCK) != 0 {
            return Err(Errno::EINVAL);
        }
        let info = self.fds.resolve(fd)?;
        let (sock, domain, ty, proto, nonblock, rcvtimeo) = {
            let table = self.table.borrow();
            let rec = table.get(info.ofd).ok_or(Errno::ENOTSOCK)?;
            if rec.state != SocketState::Listening || rec.ty != SOCK_STREAM {
                return Err(Errno::EINVAL);
            }
            (
                rec.sock.ok_or(Errno::EBADF)?,
                rec.domain,
                rec.ty,
                rec.proto,
                rec.nonblock || info.nonblock,
                rec.rcvtimeo_ms,
            )
        };
        let deadline = Deadline::new(&self.env, rcvtimeo);
        loop {
            match self.backend.call(sock, Call::Accept) {
                Ok(CallReply::Accepted(new_sock)) => {
                    let peer = match self.backend.call(new_sock, Call::GetPeerName) {
                        Ok(CallReply::Endpoint(endpoint)) => Some(endpoint),
                        _ => None,
                    };
                    let new_nonblock = flags_in & SOCK_NONBLOCK != 0;
                    let cloexec = flags_in & SOCK_CLOEXEC != 0;
                    let new_fd =
                        self.fds
                            .install(Some(new_sock), cloexec, new_nonblock || nonblock)?;
                    let new_info = self.fds.resolve(new_fd)?;
                    let mut rec = SocketRec::new(domain, ty, proto, SocketState::Connected, new_fd);
                    rec.sock = Some(new_sock);
                    rec.peer = peer;
                    rec.nonblock = new_nonblock || nonblock || new_info.nonblock;
                    self.table.borrow_mut().insert(new_info.ofd, rec)?;
                    let name_len = match peer {
                        Some(endpoint) => {
                            write_sockaddr(&SockAddr::from_endpoint(endpoint), addr_out)
                        }
                        None => 0,
                    };
                    return Ok((new_fd, name_len));
                }
                Ok(_) => return Err(Errno::EPROTO),
                Err(error) if error == Errno::EAGAIN => {
                    if nonblock {
                        return Err(Errno::EAGAIN);
                    }
                    if deadline.expired(&self.env) {
                        return Err(Errno::EAGAIN);
                    }
                    self.wait_one(sock, true, deadline.remaining(&self.env))?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// `connect`.
    pub fn connect(&self, fd: i32, addr: &[u8]) -> Result<(), Errno> {
        let info = self.fds.resolve(fd)?;
        let parsed = parse_sockaddr(addr)?;
        let endpoint = match parsed {
            SockAddr::In { .. } => parsed.to_endpoint().ok_or(Errno::EAFNOSUPPORT)?,
            SockAddr::In6 { .. } => return Err(Errno::EAFNOSUPPORT),
            SockAddr::Un { .. } => return Err(Errno::EAFNOSUPPORT),
        };
        if endpoint.port == 0 {
            return Err(Errno::ECONNREFUSED);
        }
        let nonblock = info.nonblock;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        check_family(rec, &parsed)?;
        if rec.state == SocketState::Connected && rec.ty == SOCK_STREAM {
            return Err(Errno::EISCONN);
        }
        let was_pending = rec.sock.is_none();
        if was_pending {
            let mut buffer = [0u8; 64];
            let path = crate::msg::socket_path(rec.ty, endpoint, &mut buffer)?;
            let sock = self.backend.open(path)?;
            self.fds.attach(info.ofd, sock);
            rec.sock = Some(sock);
            if let (Some(local), true) = (rec.local, rec.ty == SOCK_STREAM)
                && local.port != 0
                && let Err(error) = self.backend.call(sock, Call::Bind(local))
            {
                return Err(error);
            }
            if let Err(error) = apply_wire(&self.backend, sock, &mut rec.wire) {
                self.backend.close(sock);
                rec.sock = None;
                return Err(error);
            }
            rec.peer = Some(endpoint);
            rec.state = SocketState::Connected;
            if nonblock || rec.nonblock {
                rec.state = SocketState::Connecting;
                rec.so_error = 0;
                return Err(Errno::EINPROGRESS);
            }
            return Ok(());
        }
        let sock = rec.sock.ok_or(Errno::EBADF)?;
        let result = self.backend.call(sock, Call::Connect(endpoint));
        match result {
            Ok(_) => {
                rec.peer = Some(endpoint);
                if nonblock || rec.nonblock {
                    rec.state = SocketState::Connecting;
                    return Err(Errno::EINPROGRESS);
                }
                rec.state = SocketState::Connected;
                Ok(())
            }
            Err(error) if error == Errno::EAGAIN || error == Errno::EINPROGRESS => {
                rec.state = SocketState::Connecting;
                rec.peer = Some(endpoint);
                if nonblock || rec.nonblock {
                    Err(Errno::EINPROGRESS)
                } else {
                    let deadline = Deadline::new(&self.env, rec.sndtimeo_ms);
                    if deadline.expired(&self.env) {
                        rec.state = SocketState::Open;
                        return Err(Errno::ETIMEDOUT);
                    }
                    self.wait_one(sock, false, deadline.remaining(&self.env))?;
                    match self.backend.call(
                        sock,
                        Call::GetSockOpt {
                            opt: SockOpt::Error,
                        },
                    ) {
                        Ok(CallReply::Value(0)) => {
                            rec.state = SocketState::Connected;
                            Ok(())
                        }
                        Ok(CallReply::Value(value)) => {
                            rec.so_error = value;
                            rec.state = SocketState::Open;
                            Err(error_from_i32(value))
                        }
                        Err(error) => Err(error),
                        _ => Err(Errno::EPROTO),
                    }
                }
            }
            Err(error) => Err(error),
        }
    }

    /// `sendmsg` (the shell's `send`/`sendto` wrappers use this).
    pub fn sendmsg(
        &self,
        fd: i32,
        name: Option<&[u8]>,
        iov: &[&[u8]],
        control: &[u8],
        flags_in: i32,
    ) -> Result<usize, SendErr> {
        if let Err(error) = flags::send_supported(flags_in) {
            return Err(SendErr {
                errno: error,
                raise_sigpipe: false,
            });
        }
        if !control.is_empty() {
            return Err(SendErr {
                errno: Errno::EOPNOTSUPP,
                raise_sigpipe: false,
            });
        }
        let total = total_len(iov);
        if total > raven_proto_net::MAX_DATAGRAM {
            return Err(SendErr {
                errno: Errno::EMSGSIZE,
                raise_sigpipe: false,
            });
        }
        let mut small = [0u8; 4096];
        let mut large: Vec<u8>;
        let data: &[u8] = if total <= small.len() {
            match gather(iov, &mut small) {
                Ok(used) => small.get(..used).unwrap_or(&[]),
                Err(error) => {
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            }
        } else {
            large = vec![0u8; total];
            match gather(iov, &mut large) {
                Ok(used) => large.get(..used).unwrap_or(&[]),
                Err(error) => {
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            }
        };
        let destination = match name {
            Some(bytes) if !bytes.is_empty() => match parse_sockaddr(bytes) {
                Ok(parsed) => parsed.to_endpoint(),
                Err(error) => {
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            },
            _ => None,
        };
        let info = match self.fds.resolve(fd) {
            Ok(info) => info,
            Err(error) => {
                return Err(SendErr {
                    errno: error,
                    raise_sigpipe: false,
                });
            }
        };
        let mut table = self.table.borrow_mut();
        let rec = match table.get_mut(info.ofd) {
            Some(rec) => rec,
            None => {
                return Err(SendErr {
                    errno: Errno::ENOTSOCK,
                    raise_sigpipe: false,
                });
            }
        };
        if rec.write_shut() {
            return Err(SendErr {
                errno: Errno::EPIPE,
                raise_sigpipe: flags_in & MSG_NOSIGNAL == 0,
            });
        }
        let mut opened_now = false;
        if rec.sock.is_none() {
            if rec.ty != SOCK_DGRAM || destination.is_none() {
                return Err(SendErr {
                    errno: Errno::EDESTADDRREQ,
                    raise_sigpipe: false,
                });
            }
            let endpoint = destination.unwrap_or_default();
            let mut buffer = [0u8; 64];
            let path = match crate::msg::socket_path(rec.ty, endpoint, &mut buffer) {
                Ok(path) => path,
                Err(error) => {
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            };
            let sock = match self.backend.open(path) {
                Ok(sock) => sock,
                Err(error) => {
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            };
            self.fds.attach(info.ofd, sock);
            rec.sock = Some(sock);
            rec.peer = Some(endpoint);
            rec.state = SocketState::Connected;
            opened_now = true;
            if let Err(error) = apply_wire(&self.backend, sock, &mut rec.wire) {
                return Err(SendErr {
                    errno: error,
                    raise_sigpipe: false,
                });
            }
        }
        let sock = rec.sock.ok_or(Errno::EBADF);
        let sock = match sock {
            Ok(sock) => sock,
            Err(error) => {
                return Err(SendErr {
                    errno: error,
                    raise_sigpipe: false,
                });
            }
        };
        let nonblock = rec.nonblock || info.nonblock || flags_in & MSG_DONTWAIT != 0;
        let deadline = Deadline::new(&self.env, rec.sndtimeo_ms);
        loop {
            let write_to = if opened_now { None } else { destination };
            match self.backend.write(sock, data, write_to) {
                Ok(count) => return Ok(count),
                Err(Errno::EAGAIN) => {
                    if nonblock {
                        return Err(SendErr {
                            errno: Errno::EAGAIN,
                            raise_sigpipe: false,
                        });
                    }
                    if deadline.expired(&self.env) {
                        return Err(SendErr {
                            errno: Errno::EAGAIN,
                            raise_sigpipe: false,
                        });
                    }
                    if let Err(error) = self.wait_one(sock, false, deadline.remaining(&self.env)) {
                        return Err(SendErr {
                            errno: error,
                            raise_sigpipe: false,
                        });
                    }
                }
                Err(Errno::EPIPE) => {
                    return Err(SendErr {
                        errno: Errno::EPIPE,
                        raise_sigpipe: flags_in & MSG_NOSIGNAL == 0,
                    });
                }
                Err(error) => {
                    rec.state = SocketState::Closed;
                    return Err(SendErr {
                        errno: error,
                        raise_sigpipe: false,
                    });
                }
            }
        }
    }

    /// `recvmsg` (the shell's `recv`/`recvfrom` wrappers use this).
    pub fn recvmsg(
        &self,
        fd: i32,
        name_out: &mut [u8],
        iov: &mut [&mut [u8]],
        control_out: &mut [u8],
        flags_in: i32,
    ) -> Result<RecvMsg, Errno> {
        flags::recv_supported(flags_in)?;
        if flags_in & MSG_PEEK != 0 && !self.backend.caps().peek_flag {
            return Err(Errno::EOPNOTSUPP);
        }
        let info = self.fds.resolve(fd)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if rec.read_shut() {
            return Ok(RecvMsg {
                len: 0,
                name_len: 0,
                control_len: 0,
                flags: 0,
            });
        }
        let sock = rec.sock.ok_or(Errno::ENOTCONN)?;
        let nonblock = rec.nonblock || info.nonblock || flags_in & MSG_DONTWAIT != 0;
        let want_all = flags_in & MSG_WAITALL != 0 && rec.ty == SOCK_STREAM;
        let max_read = total_len_iov(iov).min(raven_proto_net::MAX_DATAGRAM);
        let deadline = Deadline::new(&self.env, rec.rcvtimeo_ms);
        let mut collected = Vec::new();
        let mut first_from = None;
        let mut first_truncated = false;
        loop {
            let mut stack = [0u8; 4096];
            let target_len = max_read.saturating_sub(collected.len());
            let mut large = Vec::new();
            let read_len = target_len;
            if read_len == 0 {
                break;
            }
            let peek = flags_in & MSG_PEEK != 0;
            let result = if read_len <= stack.len() {
                if peek {
                    self.backend
                        .peek(sock, stack.get_mut(..read_len).unwrap_or(&mut []))
                } else {
                    self.backend
                        .read(sock, stack.get_mut(..read_len).unwrap_or(&mut []))
                }
            } else {
                large = vec![0u8; read_len];
                if peek {
                    self.backend.peek(sock, &mut large)
                } else {
                    self.backend.read(sock, &mut large)
                }
            };
            let done = match result {
                Ok(done) => done,
                Err(Errno::EAGAIN) => {
                    if nonblock {
                        return Err(Errno::EAGAIN);
                    }
                    if deadline.expired(&self.env) {
                        return Err(Errno::EAGAIN);
                    }
                    self.wait_one(sock, true, deadline.remaining(&self.env))?;
                    continue;
                }
                Err(error) => {
                    rec.state = SocketState::Closed;
                    return Err(error);
                }
            };
            let data: &[u8] = if read_len <= stack.len() {
                stack.get(..done.len).unwrap_or(&[])
            } else {
                large.get(..done.len).unwrap_or(&[])
            };
            if collected.is_empty() {
                first_from = done.from.or(rec.peer);
                first_truncated = done.truncated;
            }
            collected.extend_from_slice(data);
            if collected.len() >= max_read || rec.ty == SOCK_DGRAM || !want_all || done.len == 0 {
                break;
            }
        }
        let used = scatter(&collected, iov);
        let name_len = match first_from {
            Some(endpoint) => write_sockaddr(&SockAddr::from_endpoint(endpoint), name_out),
            None => 0,
        };
        if let Some(slot) = control_out.get_mut(..0) {
            slot.fill(0);
        }
        let mut out_flags = 0i32;
        if first_truncated && self.backend.caps().udp_trunc_flag {
            out_flags |= MSG_TRUNC;
        }
        Ok(RecvMsg {
            len: used,
            name_len,
            control_len: 0,
            flags: out_flags,
        })
    }

    /// `shutdown`.
    pub fn shutdown(&self, fd: i32, how: i32) -> Result<(), Errno> {
        let info = self.fds.resolve(fd)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        let wire = match how {
            0 => WireShutdown::Read,
            1 => WireShutdown::Write,
            2 => WireShutdown::Both,
            _ => return Err(Errno::EINVAL),
        };
        let sock = rec.sock.ok_or(Errno::ENOTCONN)?;
        self.backend.call(sock, Call::Shutdown { how: wire })?;
        let (read, write) = match rec.state {
            SocketState::Shutdown { read, write } => (read, write),
            _ => (false, false),
        };
        rec.state = SocketState::Shutdown {
            read: read || matches!(wire, WireShutdown::Read | WireShutdown::Both),
            write: write || matches!(wire, WireShutdown::Write | WireShutdown::Both),
        };
        Ok(())
    }

    /// `getsockname`.
    pub fn getsockname(&self, fd: i32, out: &mut [u8]) -> Result<usize, Errno> {
        let info = self.fds.resolve(fd)?;
        let table = self.table.borrow();
        let rec = table.get(info.ofd).ok_or(Errno::ENOTSOCK)?;
        let local = if let Some(local) = rec.local {
            Some(local)
        } else if let Some(sock) = rec.sock {
            match self.backend.call(sock, Call::GetSockName) {
                Ok(CallReply::Endpoint(endpoint)) => Some(endpoint),
                Ok(_) => return Err(Errno::EPROTO),
                Err(error) => return Err(error),
            }
        } else {
            Some(Endpoint {
                ip: [0, 0, 0, 0],
                port: 0,
            })
        };
        Ok(match local {
            Some(endpoint) => write_sockaddr(&SockAddr::from_endpoint(endpoint), out),
            None => 0,
        })
    }

    /// `getpeername`.
    pub fn getpeername(&self, fd: i32, out: &mut [u8]) -> Result<usize, Errno> {
        let info = self.fds.resolve(fd)?;
        let table = self.table.borrow();
        let rec = table.get(info.ofd).ok_or(Errno::ENOTSOCK)?;
        let peer = if let Some(peer) = rec.peer {
            Some(peer)
        } else if let Some(sock) = rec.sock {
            match self.backend.call(sock, Call::GetPeerName) {
                Ok(CallReply::Endpoint(endpoint)) => Some(endpoint),
                Ok(_) => None,
                Err(Errno::ENOTCONN) => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let peer = peer.ok_or(Errno::ENOTCONN)?;
        Ok(write_sockaddr(&SockAddr::from_endpoint(peer), out))
    }

    /// `getsockopt`.
    pub fn getsockopt(
        &self,
        fd: i32,
        level: i32,
        name: i32,
        out: &mut [u8],
    ) -> Result<usize, Errno> {
        let info = self.fds.resolve(fd)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if level == SOL_SOCKET && name == SO_ERROR {
            let mut value = rec.so_error;
            rec.so_error = 0;
            if rec.state == SocketState::Connecting {
                rec.state = SocketState::Connected;
                value = 0;
            }
            return opts::write_i32(out, value);
        }
        if let Some(is_read) = opts::timeout_slot(name) {
            let millis = if is_read {
                rec.rcvtimeo_ms
            } else {
                rec.sndtimeo_ms
            };
            let (sec, usec) = match millis {
                Some(value) => ((value / 1000) as i64, ((value % 1000) * 1000) as i64),
                None => (0, 0),
            };
            return opts::write_timeval(out, sec, usec);
        }
        if opts::stored_option(level, name) {
            return match name {
                13 => {
                    let slot = out.get_mut(..8).ok_or(Errno::EINVAL)?;
                    slot.get_mut(..4)
                        .ok_or(Errno::EINVAL)?
                        .copy_from_slice(&rec.opts.linger[0].to_le_bytes());
                    slot.get_mut(4..8)
                        .ok_or(Errno::EINVAL)?
                        .copy_from_slice(&rec.opts.linger[1].to_le_bytes());
                    Ok(8)
                }
                9 => opts::write_i32(out, rec.opts.keepalive),
                15 => opts::write_i32(out, rec.opts.reuseport),
                6 => opts::write_i32(out, rec.opts.broadcast),
                10 => opts::write_i32(out, rec.opts.oobinline),
                26 => opts::write_i32(out, rec.opts.v6only),
                _ => Err(Errno::ENOPROTOOPT),
            };
        }
        if let Some(option) = opts::record_option(name) {
            let value = match option {
                RecordOption::Type => rec.ty,
                RecordOption::Domain => rec.domain,
                RecordOption::Protocol => default_proto(rec.ty, rec.proto),
                RecordOption::AcceptConn => i32::from(rec.state == SocketState::Listening),
            };
            return opts::write_i32(out, value);
        }
        let wire = opts::wire_option(level, name).ok_or(Errno::ENOPROTOOPT)?;
        if rec.sock.is_none() {
            if let Some((_, value)) = rec.wire.pending.iter().rev().find(|(opt, _)| *opt == wire) {
                return opts::write_i32(out, *value);
            }
            return opts::write_i32(out, 0);
        }
        let sock = rec.sock.ok_or(Errno::EBADF)?;
        match self.backend.call(sock, Call::GetSockOpt { opt: wire }) {
            Ok(CallReply::Value(value)) => opts::write_i32(out, value),
            Ok(_) => Err(Errno::EPROTO),
            Err(error) => Err(error),
        }
    }

    /// `setsockopt`.
    pub fn setsockopt(&self, fd: i32, level: i32, name: i32, value: &[u8]) -> Result<(), Errno> {
        let info = self.fds.resolve(fd)?;
        let mut table = self.table.borrow_mut();
        let rec = table.get_mut(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if let Some(is_read) = opts::timeout_slot(name) {
            let (sec, usec) = opts::read_timeval(value)?;
            let millis = opts::timeout_ms(sec, usec);
            if is_read {
                rec.rcvtimeo_ms = millis;
            } else {
                rec.sndtimeo_ms = millis;
            }
            return Ok(());
        }
        if opts::stored_option(level, name) {
            match name {
                13 => rec.opts.linger = opts::read_linger(value)?,
                9 => rec.opts.keepalive = opts::read_i32(value)?,
                15 => rec.opts.reuseport = opts::read_i32(value)?,
                6 => rec.opts.broadcast = opts::read_i32(value)?,
                10 => rec.opts.oobinline = opts::read_i32(value)?,
                26 => {
                    if !self.backend.caps().ipv6 {
                        return Err(Errno::ENOPROTOOPT);
                    }
                    rec.opts.v6only = opts::read_i32(value)?;
                }
                _ => return Err(Errno::ENOPROTOOPT),
            }
            return Ok(());
        }
        let wire = opts::wire_option(level, name).ok_or(Errno::ENOPROTOOPT)?;
        if !wire.is_writable() {
            return Err(Errno::EINVAL);
        }
        let parsed = opts::read_i32(value)?;
        if parsed < 0 {
            return Err(Errno::EINVAL);
        }
        if let Some(sock) = rec.sock {
            match self.backend.call(
                sock,
                Call::SetSockOpt {
                    opt: wire,
                    value: parsed,
                },
            ) {
                Ok(CallReply::Ok) => Ok(()),
                Ok(_) => Err(Errno::EPROTO),
                Err(error) => Err(error),
            }
        } else {
            rec.wire.pending.push((wire, parsed));
            Ok(())
        }
    }

    /// `sockatmark`.
    pub fn sockatmark(&self, fd: i32) -> Result<i32, Errno> {
        let info = self.fds.resolve(fd)?;
        let table = self.table.borrow();
        let rec = table.get(info.ofd).ok_or(Errno::ENOTSOCK)?;
        if rec.ty != SOCK_STREAM {
            return Err(Errno::ENOTTY);
        }
        Ok(0)
    }

    /// Called by the fd table when the last descriptor reference is dropped.
    pub fn close_hook(&self, ofd: u32) {
        let removed = self.table.borrow_mut().remove(ofd);
        if let Some(rec) = removed
            && let Some(sock) = rec.sock
        {
            self.backend.close(sock);
        }
    }

    /// The poll registration for a descriptor.
    pub fn poll_interest(&self, fd: i32) -> Result<PollInterest, Errno> {
        let info = self.fds.resolve(fd)?;
        let table = self.table.borrow();
        let rec = table.get(info.ofd).ok_or(Errno::ENOTSOCK)?;
        ready::interest(rec)
    }

    /// Map a backend wait result to `revents`.
    #[must_use]
    pub fn poll_revents(&self, _fd: i32, events: i16, ev: &WaitItem) -> i16 {
        ready::revents(events, ev)
    }

    /// `if_nametoindex`; unknown names are `ENODEV`.
    pub fn if_nametoindex(&self, name: &[u8]) -> Result<u32, Errno> {
        crate::iface::nametoindex(&self.backend, name)
    }

    /// `if_indextoname`.
    pub fn if_indextoname(&self, idx: u32, out: &mut [u8; 16]) -> Result<usize, Errno> {
        crate::iface::indextoname(&self.backend, idx, out)
    }

    /// `if_nameindex`.
    pub fn if_nameindex(&self) -> Result<Vec<(u32, Vec<u8>)>, Errno> {
        crate::iface::nameindex(&self.backend)
    }

    /// `getifaddrs`.
    pub fn getifaddrs(&self) -> Result<Vec<crate::IfAddr>, Errno> {
        crate::iface::getifaddrs(&self.backend)
    }

    /// Configure the `AF_UNIX` backend after construction.
    pub fn set_unix(&mut self, unix: Option<U>) {
        self.unix = unix;
    }

    fn wait_one(&self, sock: Sock, read: bool, timeout_ms: Option<u64>) -> Result<(), Errno> {
        let mut item = WaitItem {
            sock,
            want_read: read,
            want_write: !read,
            ..WaitItem::default()
        };
        match self
            .backend
            .wait(core::slice::from_mut(&mut item), timeout_ms)
        {
            Ok(0) => Err(Errno::EAGAIN),
            Ok(_) => {
                if item.got_err {
                    Err(Errno::EIO)
                } else {
                    Ok(())
                }
            }
            Err(error) => Err(error),
        }
    }
}

fn validate_proto(ty: i32, proto: i32) -> Result<(), Errno> {
    let accepted = if ty == SOCK_STREAM {
        proto == 0 || proto == 6
    } else {
        proto == 0 || proto == 17
    };
    if accepted {
        Ok(())
    } else {
        Err(Errno::EPROTONOSUPPORT)
    }
}

fn default_proto(ty: i32, proto: i32) -> i32 {
    if proto != 0 {
        proto
    } else if ty == SOCK_STREAM {
        6
    } else {
        17
    }
}

fn check_family(rec: &SocketRec, addr: &SockAddr) -> Result<(), Errno> {
    let expected = i32::from(addr.family());
    if rec.domain == expected {
        Ok(())
    } else {
        Err(Errno::EAFNOSUPPORT)
    }
}

fn clamp_backlog(backlog: i32) -> Result<u16, Errno> {
    let value = if backlog <= 0 { 128 } else { backlog.min(4096) };
    u16::try_from(value).map_err(|_| Errno::EINVAL)
}

fn apply_wire<B: NetBackend + ?Sized>(
    backend: &B,
    sock: Sock,
    wire: &mut opts::WireOpts,
) -> Result<(), Errno> {
    for (option, value) in wire.pending.iter().copied() {
        match backend.call(sock, Call::SetSockOpt { opt: option, value }) {
            Ok(CallReply::Ok) => {}
            Ok(_) => return Err(Errno::EPROTO),
            Err(error) => return Err(error),
        }
    }
    wire.pending.clear();
    Ok(())
}

fn error_from_i32(value: i32) -> Errno {
    if value <= 0 || value > i32::from(u16::MAX) {
        Errno::EIO
    } else {
        Errno(value as u16)
    }
}

fn total_len_iov(iov: &[&mut [u8]]) -> usize {
    iov.iter()
        .fold(0usize, |sum, part| sum.saturating_add(part.len()))
}

/// Copy `data` across mutable iovecs; the caller checked the total length.
fn scatter(data: &[u8], iov: &mut [&mut [u8]]) -> usize {
    let mut position = 0usize;
    for part in iov.iter_mut() {
        if position >= data.len() {
            break;
        }
        let count = part.len().min(data.len().saturating_sub(position));
        let end = position.saturating_add(count);
        if let (Some(target), Some(source)) = (part.get_mut(..count), data.get(position..end)) {
            target.copy_from_slice(source);
        }
        position = end;
    }
    position
}

/// Re-exported so tests and the shell can name the capability struct.
pub type NetCaps = Caps;

#[cfg(test)]
#[path = "socket_ut.rs"]
mod tests;
