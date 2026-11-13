// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The user-space socket record and its bounded table.
use crate::ErrnoExt;

use alloc::collections::BTreeMap;

use raven_libc_abi::errno::Errno;
use raven_proto_net::Endpoint;

use crate::Sock;
use crate::opts::{StoredOpts, WireOpts};

/// Maximum live socket records in one process.
pub const MAX_SOCKETS: usize = 4096;

/// The state of one socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    /// No kernel handle yet (pre-PG-40 lazy open).
    Pending,
    /// A handle exists, not bound, connected or listening.
    Open,
    /// A local endpoint is recorded.
    Bound,
    /// A TCP listener.
    Listening,
    /// A non-blocking connect is in progress.
    Connecting,
    /// Connected TCP or UDP.
    Connected,
    /// One or both directions were shut down.
    Shutdown {
        /// Read direction closed.
        read: bool,
        /// Write direction closed.
        write: bool,
    },
    /// The handle is gone.
    Closed,
}

/// One socket record.
#[derive(Clone, Debug)]
pub struct SocketRec {
    /// `AF_INET`, `AF_INET6` or `AF_UNIX`.
    pub domain: i32,
    /// `SOCK_STREAM` or `SOCK_DGRAM`.
    pub ty: i32,
    /// The protocol number.
    pub proto: i32,
    /// Current state.
    pub state: SocketState,
    /// Local endpoint, when known.
    pub local: Option<Endpoint>,
    /// Peer endpoint, when connected.
    pub peer: Option<Endpoint>,
    /// A port-0 bind in lazy mode.
    pub bound_port0: bool,
    /// Locally stored options.
    pub opts: StoredOpts,
    /// Wire options waiting for a handle.
    pub wire: WireOpts,
    /// Receive timeout in milliseconds; `None` is infinite.
    pub rcvtimeo_ms: Option<u64>,
    /// Send timeout in milliseconds; `None` is infinite.
    pub sndtimeo_ms: Option<u64>,
    /// Pending `SO_ERROR`.
    pub so_error: i32,
    /// A listener that accepted at least one connection.
    pub accepting: bool,
    /// The libc fd.
    pub fd: i32,
    /// The kernel handle, when open.
    pub sock: Option<Sock>,
    /// Whether the descriptor is non-blocking.
    pub nonblock: bool,
}

impl SocketRec {
    /// Build a record in `state`.
    #[must_use]
    pub fn new(domain: i32, ty: i32, proto: i32, state: SocketState, fd: i32) -> Self {
        Self {
            domain,
            ty,
            proto,
            state,
            local: None,
            peer: None,
            bound_port0: false,
            opts: StoredOpts::default(),
            wire: WireOpts::default(),
            rcvtimeo_ms: None,
            sndtimeo_ms: None,
            so_error: 0,
            accepting: false,
            fd,
            sock: None,
            nonblock: false,
        }
    }

    /// Whether the read direction is shut down.
    #[must_use]
    pub const fn read_shut(&self) -> bool {
        matches!(self.state, SocketState::Shutdown { read: true, .. })
    }

    /// Whether the write direction is shut down.
    #[must_use]
    pub const fn write_shut(&self) -> bool {
        matches!(self.state, SocketState::Shutdown { write: true, .. })
    }
}

/// The bounded `ofd -> record` table.
#[derive(Default)]
pub struct SocketTable {
    records: BTreeMap<u32, SocketRec>,
}

impl SocketTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: BTreeMap::new(),
        }
    }

    /// Number of live records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Insert a record under its OFD token.
    pub fn insert(&mut self, ofd: u32, rec: SocketRec) -> Result<(), Errno> {
        if self.records.len() >= MAX_SOCKETS {
            return Err(Errno::EMFILE);
        }
        self.records.insert(ofd, rec);
        Ok(())
    }

    /// Look up a record.
    #[must_use]
    pub fn get(&self, ofd: u32) -> Option<&SocketRec> {
        self.records.get(&ofd)
    }

    /// Mutably look up a record.
    pub fn get_mut(&mut self, ofd: u32) -> Option<&mut SocketRec> {
        self.records.get_mut(&ofd)
    }

    /// Remove a record.
    pub fn remove(&mut self, ofd: u32) -> Option<SocketRec> {
        self.records.remove(&ofd)
    }
}

#[cfg(test)]
#[path = "record_ut.rs"]
mod tests;
