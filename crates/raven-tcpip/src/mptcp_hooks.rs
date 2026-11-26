// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Object-safe MPTCP subflow hooks over the existing TCP stack.
//!
//! MPTCP (RFC 8684) carries its data over ordinary TCP subflows. The `raven-mptcp` core needs a
//! small surface to create a subflow, steer bytes into it, aggregate live readiness and mark a
//! backup path. This module adds that surface without putting any MPTCP wire logic into the TCP
//! stack: every operation delegates to the plain-TCP connect/send/recv/close path in
//! [`crate::Stack`].
//!
//! Donors read for behaviour only: NetBSD `sys/netinet/tcp_usrreq.c` (`PRU_CONNECT`,
//! `PRU_DISCONNECT`, `PRU_SHUTDOWN`, `PRU_SEND`), NetBSD `sys/netinet/tcp_subr.c` (`tcp_close`,
//! `tcp_drop`), the NetBSD `soreadable`/`sowritable` readiness mapping already ported in
//! [`crate::readiness`], and RFC 8684 sections 3-4. No donor code is copied.
//!
//! Readiness is deliberately **live**: [`SubflowOps::readiness`] asks the same socket predicates
//! as [`crate::Stack::readiness`] on every call and never returns a remembered mask. The
//! `test_set_subflow_readiness_stale` seam exists only for the negative test and is compiled for
//! tests/`testutil`; it is not part of the production path.

use alloc::collections::BTreeMap;
#[cfg(any(test, feature = "testutil"))]
use core::cell::RefCell;

use raven_abi::errno::Errno;

use crate::Ipv4Addr;
use crate::device::Device;
use crate::event::SocketId;
use crate::ipv6_stack::IpAddr;
use crate::readiness::Readiness;
use crate::stack::Stack;

/// A subflow handle owned by the MPTCP core, stable for the life of the subflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubflowId(pub u32);

/// One live subflow record.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SubflowEntry {
    /// The plain-TCP socket carrying the subflow.
    pub(crate) socket: SocketId,
    /// The local address the core requested, for diagnostics and future multipath binding.
    pub(crate) local_if: Option<Ipv4Addr>,
    /// Whether the path is a backup path (`MP_PRIO`/backup flag).
    pub(crate) backup: bool,
    /// The core closed this subflow; readiness reports hangup/error from here on.
    pub(crate) closed: bool,
}

/// The per-stack subflow table. Lives in [`crate::socket::SocketTable`] so the hook trait can be
/// implemented without adding a second stack object.
pub(crate) struct SubflowTable {
    entries: BTreeMap<u32, SubflowEntry>,
    next_id: u32,
    #[cfg(any(test, feature = "testutil"))]
    stale_readiness: bool,
    #[cfg(any(test, feature = "testutil"))]
    readiness_cache: RefCell<BTreeMap<u32, Readiness>>,
}

impl SubflowTable {
    /// An empty table.
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_id: 1,
            #[cfg(any(test, feature = "testutil"))]
            stale_readiness: false,
            #[cfg(any(test, feature = "testutil"))]
            readiness_cache: RefCell::new(BTreeMap::new()),
        }
    }

    /// Register a socket as a subflow and return its handle.
    pub(crate) fn insert(
        &mut self,
        socket: SocketId,
        local_if: Option<Ipv4Addr>,
    ) -> Result<SubflowId, Errno> {
        let mut id = self.next_id.max(1);
        for _ in 0..=self.entries.len() {
            if !self.entries.contains_key(&id) {
                self.next_id = id.wrapping_add(1).max(1);
                self.entries.insert(
                    id,
                    SubflowEntry {
                        socket,
                        local_if,
                        backup: false,
                        closed: false,
                    },
                );
                return Ok(SubflowId(id));
            }
            id = id.wrapping_add(1).max(1);
        }
        Err(Errno::ENOSPC)
    }

    pub(crate) fn entry(&self, subflow: SubflowId) -> Result<&SubflowEntry, Errno> {
        self.entries.get(&subflow.0).ok_or(Errno::EBADF)
    }

    pub(crate) fn entry_mut(&mut self, subflow: SubflowId) -> Result<&mut SubflowEntry, Errno> {
        self.entries.get_mut(&subflow.0).ok_or(Errno::EBADF)
    }

    /// Mark a subflow closed. The handle is kept so readiness can report the terminal state.
    pub(crate) fn mark_closed(&mut self, subflow: SubflowId) {
        if let Some(entry) = self.entries.get_mut(&subflow.0) {
            entry.closed = true;
        }
    }

    #[cfg(any(test, feature = "testutil"))]
    pub(crate) fn set_stale_readiness(&mut self, stale: bool) {
        self.stale_readiness = stale;
        self.readiness_cache.borrow_mut().clear();
    }

    #[cfg(any(test, feature = "testutil"))]
    pub(crate) fn stale_readiness(&self) -> bool {
        self.stale_readiness
    }

    #[cfg(any(test, feature = "testutil"))]
    pub(crate) fn cached_readiness(&self, subflow: SubflowId) -> Option<Readiness> {
        self.readiness_cache.borrow().get(&subflow.0).copied()
    }

    #[cfg(any(test, feature = "testutil"))]
    pub(crate) fn cache_readiness(&self, subflow: SubflowId, readiness: Readiness) {
        self.readiness_cache
            .borrow_mut()
            .insert(subflow.0, readiness);
    }
}

impl Default for SubflowTable {
    fn default() -> Self {
        Self::new()
    }
}

/// The terminal readiness for a closed or unknown subflow.
const CLOSED: Readiness = Readiness {
    readable: false,
    writable: false,
    hup: true,
    error: true,
};

/// The MPTCP subflow surface implemented by the TCP stack.
///
/// The trait is object-safe: the MPTCP core can hold `&mut dyn SubflowOps` and never reaches into
/// the socket table. Every method delegates to the plain-TCP path; no MPTCP wire logic lives here.
pub trait SubflowOps {
    /// Create a subflow to `remote` (address and port) over `local_if`.
    ///
    /// The current stack is single-homed: `local_if` may be `None` (use the configured address)
    /// or the configured IPv4 address; any other address is refused with `EADDRNOTAVAIL`.
    fn open_subflow(
        &mut self,
        remote: (IpAddr, u16),
        local_if: Option<Ipv4Addr>,
    ) -> Result<SubflowId, Errno>;

    /// Queue bytes on one subflow; `EAGAIN` when that subflow's send queue is full.
    fn send_on(&mut self, subflow: SubflowId, data: &[u8]) -> Result<usize, Errno>;

    /// Read bytes from one subflow; `Ok(0)` is an orderly EOF.
    fn recv_from(&mut self, subflow: SubflowId, buffer: &mut [u8]) -> Result<usize, Errno>;

    /// Live readiness for one subflow. This calls the socket predicates on every call; there is no
    /// cached mask.
    fn readiness(&self, subflow: SubflowId) -> Readiness;

    /// Close one subflow. The handle remains valid for a terminal readiness query.
    fn close_subflow(&mut self, subflow: SubflowId);

    /// Mark or clear a subflow's backup flag.
    fn set_backup(&mut self, subflow: SubflowId, backup: bool);

    /// The current backup flag for a subflow; `false` for an unknown handle.
    fn is_backup(&self, subflow: SubflowId) -> bool;
}

impl<D: Device> SubflowOps for Stack<D> {
    fn open_subflow(
        &mut self,
        remote: (IpAddr, u16),
        local_if: Option<Ipv4Addr>,
    ) -> Result<SubflowId, Errno> {
        if let Some(requested) = local_if {
            let configured = self.ipv4.ok_or(Errno::EADDRNOTAVAIL)?.addr;
            if !requested.is_unspecified() && requested != configured {
                return Err(Errno::EADDRNOTAVAIL);
            }
        }
        let socket = self.tcp_connect(remote, None)?;
        self.sockets.subflows.insert(socket, local_if)
    }

    fn send_on(&mut self, subflow: SubflowId, data: &[u8]) -> Result<usize, Errno> {
        let socket = {
            let entry = self.sockets.subflows.entry(subflow)?;
            if entry.closed {
                return Err(Errno::EBADF);
            }
            entry.socket
        };
        self.send(socket, data)
    }

    fn recv_from(&mut self, subflow: SubflowId, buffer: &mut [u8]) -> Result<usize, Errno> {
        let socket = {
            let entry = self.sockets.subflows.entry(subflow)?;
            if entry.closed {
                return Err(Errno::EBADF);
            }
            entry.socket
        };
        self.recv(socket, buffer)
    }

    fn readiness(&self, subflow: SubflowId) -> Readiness {
        let Ok(entry) = self.sockets.subflows.entry(subflow) else {
            return CLOSED;
        };
        if entry.closed {
            return CLOSED;
        }
        #[cfg(any(test, feature = "testutil"))]
        if self.sockets.subflows.stale_readiness() {
            if let Some(cached) = self.sockets.subflows.cached_readiness(subflow) {
                return cached;
            }
            let live = Stack::readiness(self, entry.socket).unwrap_or(CLOSED);
            self.sockets.subflows.cache_readiness(subflow, live);
            return live;
        }
        Stack::readiness(self, entry.socket).unwrap_or(CLOSED)
    }

    fn close_subflow(&mut self, subflow: SubflowId) {
        let socket = match self.sockets.subflows.entry(subflow) {
            Ok(entry) if !entry.closed => entry.socket,
            _ => {
                self.sockets.subflows.mark_closed(subflow);
                return;
            }
        };
        let _ = self.close(socket);
        self.sockets.subflows.mark_closed(subflow);
    }

    fn set_backup(&mut self, subflow: SubflowId, backup: bool) {
        if let Ok(entry) = self.sockets.subflows.entry_mut(subflow) {
            entry.backup = backup;
        }
    }

    fn is_backup(&self, subflow: SubflowId) -> bool {
        self.sockets
            .subflows
            .entry(subflow)
            .is_ok_and(|entry| entry.backup)
    }
}

impl<D: Device> Stack<D> {
    /// The local address the core requested for a subflow, if any.
    #[must_use]
    pub fn subflow_local_if(&self, subflow: SubflowId) -> Option<Ipv4Addr> {
        self.sockets
            .subflows
            .entry(subflow)
            .ok()
            .and_then(|entry| entry.local_if)
    }

    /// Test-only readiness-cache seam. With `stale` set, the first readiness for each subflow is
    /// remembered and returned again, so the live-readiness test must fail. Compiled only for
    /// tests/`testutil`; never reachable in the production path.
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_set_subflow_readiness_stale(&mut self, stale: bool) {
        self.sockets.subflows.set_stale_readiness(stale);
    }
}
