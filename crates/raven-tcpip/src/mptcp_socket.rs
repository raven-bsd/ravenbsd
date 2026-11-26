// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MPTCP2b: the Linux-shaped MPTCP socket surface over the host stack.
//!
//! `MptcpSocket::open` accepts the Linux `IPPROTO_MPTCP` protocol over `AF_INET`/`AF_INET6` and
//! returns a meta-socket that behaves like a normal stream for send/recv/readiness/close. The
//! path manager adds/removes subflows, marks backup paths and reports per-subflow state; failover
//! is the default, aggregation is opt-in. The real-wire MP_CAPABLE negotiation and libc/netd
//! syscall plumbing are MPTCP2c; this slice is host-testable API and lifecycle.
//!
//! A caller that already knows the peer lacks MP_CAPABLE passes `peer_capable = false`; the
//! socket then uses one plain TCP connection and the MPTCP options/state are never created.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_abi::errno::Errno;
use raven_mptcp::{Error as MptcpError, MetaSocket};

use crate::device::Device;
use crate::event::SocketId;
use crate::ipv6_stack::IpAddr;
use crate::mptcp_bridge::StackSubflow;
use crate::mptcp_hooks::{SubflowId, SubflowOps};
use crate::readiness::Readiness;
use crate::stack::Stack;
use crate::{Ipv4Addr, Ipv4Config};

/// Linux `IPPROTO_MPTCP` (RFC 8684 / IANA).
pub const IPPROTO_MPTCP: i32 = 262;
/// Linux `AF_INET`.
pub const AF_INET: i32 = 2;
/// Linux `AF_INET6`.
pub const AF_INET6: i32 = 10;

/// Default TCP MSS used by a new meta-socket.
pub const DEFAULT_MSS: usize = 1460;
/// Default connection send-buffer cap.
pub const DEFAULT_SEND_CAP: usize = 1 << 20;
/// Default connection receive-memory cap.
pub const DEFAULT_RCV_CAP: usize = 1 << 20;
/// Default connection receive window.
pub const DEFAULT_WINDOW: u64 = 262_144;

/// Per-subflow state, shaped like the Linux path-manager GET_ADDR/SUBFLOWS response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubflowInfo {
    /// Stable subflow id.
    pub subflow_id: u32,
    /// Local address selected for the path.
    pub local: IpAddr,
    /// Remote address.
    pub remote: IpAddr,
    /// Remote port.
    pub remote_port: u16,
    /// The path is a backup: it carries no data while a regular path is alive.
    pub backup: bool,
    /// Unacknowledged bytes last sent on this subflow.
    pub bytes_in_flight: usize,
}

#[derive(Clone, Copy, Debug)]
struct PathInfo {
    local: IpAddr,
    remote: (IpAddr, u16),
    backup: bool,
}

/// An MPTCP connection (meta-socket) over one or more host TCP subflows.
pub struct MptcpSocket<D: Device + 'static> {
    stack: Rc<RefCell<Stack<D>>>,
    meta: MetaSocket,
    paths: BTreeMap<u32, PathInfo>,
    fallback: Option<SocketId>,
    closed: bool,
}

impl<D: Device + 'static> MptcpSocket<D> {
    /// Open an MPTCP socket to `remote`.
    ///
    /// `family` must agree with `remote` (`AF_INET`/`AF_INET6`), `protocol` must be
    /// [`IPPROTO_MPTCP`]. `local_if` selects the IPv4 source for the primary path (or `None` for
    /// the configured address). If `peer_capable` is false the socket falls back to one plain
    /// TCP connection, exactly as a non-MPTCP peer requires.
    pub fn open(
        stack: Rc<RefCell<Stack<D>>>,
        family: i32,
        protocol: i32,
        remote: (IpAddr, u16),
        local_if: Option<Ipv4Addr>,
        peer_capable: bool,
    ) -> Result<Self, Errno> {
        if protocol != IPPROTO_MPTCP {
            return Err(Errno::EINVAL);
        }
        match (family, remote.0) {
            (AF_INET, IpAddr::V4(_)) | (AF_INET6, IpAddr::V6(_)) => {}
            (AF_INET | AF_INET6, _) => return Err(Errno::EAFNOSUPPORT),
            _ => return Err(Errno::EAFNOSUPPORT),
        }
        if !peer_capable {
            let socket = stack.borrow_mut().tcp_connect(remote, None)?;
            return Ok(Self {
                stack,
                meta: MetaSocket::new(
                    DEFAULT_MSS,
                    DEFAULT_SEND_CAP,
                    DEFAULT_RCV_CAP,
                    DEFAULT_WINDOW,
                ),
                paths: BTreeMap::new(),
                fallback: Some(socket),
                closed: false,
            });
        }

        let subflow = stack.borrow_mut().open_subflow(remote, local_if)?;
        let local = local_address(&stack.borrow(), local_if);
        let mut meta = MetaSocket::new(
            DEFAULT_MSS,
            DEFAULT_SEND_CAP,
            DEFAULT_RCV_CAP,
            DEFAULT_WINDOW,
        );
        meta.add_subflow(
            subflow.0,
            false,
            Box::new(StackSubflow::new(Rc::clone(&stack), subflow)),
        )
        .map_err(map_meta)?;
        let mut paths = BTreeMap::new();
        paths.insert(
            subflow.0,
            PathInfo {
                local,
                remote,
                backup: false,
            },
        );
        Ok(Self {
            stack,
            meta,
            paths,
            fallback: None,
            closed: false,
        })
    }

    /// Whether the connection fell back to plain TCP.
    #[must_use]
    pub fn is_fallback(&self) -> bool {
        self.fallback.is_some()
    }

    /// Whether the connection is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Send bytes on the connection.
    pub fn send(&mut self, data: &[u8]) -> Result<usize, Errno> {
        if self.closed {
            return Err(Errno::EBADF);
        }
        match self.fallback {
            Some(socket) => self.stack.borrow_mut().send(socket, data),
            None => self.meta.send(data).map_err(map_meta),
        }
    }

    /// Read in-order connection bytes.
    ///
    /// This slice reads the active real subflow; reassembly of DSS mappings from the wire is
    /// MPTCP2c. The MetaSocket still owns the send-side sequencing and failover.
    pub fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        if self.closed {
            return Err(Errno::EBADF);
        }
        match self.fallback {
            Some(socket) => self.stack.borrow_mut().recv(socket, buffer),
            None => {
                self.meta.refresh();
                let id = self.meta.active().ok_or(Errno::ENOTCONN)?;
                self.stack.borrow_mut().recv_from(SubflowId(id), buffer)
            }
        }
    }

    /// Live readiness: current connection bytes and path health.
    #[must_use]
    pub fn readiness(&self) -> Readiness {
        match self.fallback {
            Some(socket) => Stack::readiness(&*self.stack.borrow(), socket).unwrap_or(Readiness {
                readable: false,
                writable: false,
                hup: true,
                error: true,
            }),
            None => {
                let Some(active) = self.meta.active() else {
                    return Readiness {
                        readable: false,
                        writable: false,
                        hup: true,
                        error: false,
                    };
                };
                SubflowOps::readiness(&*self.stack.borrow(), SubflowId(active))
            }
        }
    }

    /// Add a path (Linux `ADD_ADDR` equivalent) and return its subflow id.
    pub fn add_path(
        &mut self,
        remote: (IpAddr, u16),
        local_if: Option<Ipv4Addr>,
        backup: bool,
    ) -> Result<u32, Errno> {
        if self.fallback.is_some() {
            return Err(Errno::EOPNOTSUPP);
        }
        if self.closed {
            return Err(Errno::EBADF);
        }
        let subflow = self.stack.borrow_mut().open_subflow(remote, local_if)?;
        let local = local_address(&self.stack.borrow(), local_if);
        self.meta
            .add_subflow(
                subflow.0,
                backup,
                Box::new(StackSubflow::new(Rc::clone(&self.stack), subflow)),
            )
            .map_err(map_meta)?;
        self.paths.insert(
            subflow.0,
            PathInfo {
                local,
                remote,
                backup,
            },
        );
        Ok(subflow.0)
    }

    /// Remove a path (Linux `REMOVE_ADDR` equivalent).
    pub fn remove_path(&mut self, subflow_id: u32) -> Result<(), Errno> {
        if self.paths.remove(&subflow_id).is_none() {
            return Err(Errno::ENOENT);
        }
        self.meta.remove_subflow(subflow_id).map_err(map_meta)
    }

    /// Mark a path as backup or regular (Linux `SET_FLAGS` backup/unbackup).
    pub fn set_backup(&mut self, subflow_id: u32, backup: bool) -> Result<(), Errno> {
        let path = self.paths.get_mut(&subflow_id).ok_or(Errno::ENOENT)?;
        path.backup = backup;
        self.meta.set_backup(subflow_id, backup).map_err(map_meta)
    }

    /// Enable or disable aggregation. Off by default: failover with idle backups.
    pub fn set_aggregation(&mut self, enabled: bool) {
        self.meta.set_aggregation(enabled);
    }

    /// Whether aggregation is enabled.
    #[must_use]
    pub fn aggregation(&self) -> bool {
        self.meta.aggregation()
    }

    /// Per-subflow state in subflow-id order.
    #[must_use]
    pub fn subflows(&self) -> Vec<SubflowInfo> {
        self.paths
            .iter()
            .map(|(id, path)| SubflowInfo {
                subflow_id: *id,
                local: path.local,
                remote: path.remote.0,
                remote_port: path.remote.1,
                backup: path.backup,
                bytes_in_flight: self.meta.bytes_in_flight(*id),
            })
            .collect()
    }

    /// Close the connection and every subflow.
    pub fn close(&mut self) -> Result<(), Errno> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        match self.fallback {
            Some(socket) => self.stack.borrow_mut().close(socket),
            None => {
                self.meta.close();
                self.meta.close_paths();
                Ok(())
            }
        }
    }
}

fn local_address<D: Device>(stack: &Stack<D>, local_if: Option<Ipv4Addr>) -> IpAddr {
    local_if
        .map(IpAddr::V4)
        .or_else(|| stack.ipv4.map(|config: Ipv4Config| IpAddr::V4(config.addr)))
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

fn map_meta(error: MptcpError) -> Errno {
    match error {
        MptcpError::WouldBlock => Errno::EAGAIN,
        MptcpError::Closed => Errno::EPIPE,
        _ => Errno::EINVAL,
    }
}
