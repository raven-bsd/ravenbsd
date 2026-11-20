// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Live, non-consuming socket readiness.
//!
//! A readiness doorbell is a hint: the kernel latch says that *something* changed, not which
//! direction is currently ready. `Stack::readiness` asks the same socket state that
//! [`crate::Stack::poll`] uses to generate edges, without reading, accepting, popping a datagram,
//! clearing an error or allocating a socket.
//!
//! The predicates mirror NetBSD `sys/sys/socketvar.h:soreadable`/`sowritable`
//! (`sb_cc >= sb_lowat`, `SS_CANTRCVMORE`, listener `so_qlen`, `sbspace`, socket errors) and the
//! level check in `sys/kern/uipc_socket.c:sodopoll`/`sopoll`.

use raven_abi::errno::Errno;

use crate::SocketId;
use crate::device::Device;
use crate::socket::{Kind, SocketRecord};
use crate::stack::Stack;

/// One live readiness snapshot for a socket.
///
/// This is a measurement, not a stored notification: two queries in a row return the same bits
/// and do not consume data or errors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readiness {
    /// Data may be read, or the peer has finished sending (EOF is readable).
    pub readable: bool,
    /// A write of at least one byte would be accepted, not rejected by shutdown or a full buffer.
    pub writable: bool,
    /// The connection is fully closed (not merely a peer half-close).
    pub hup: bool,
    /// A pending socket error is waiting; the query does not consume it.
    pub error: bool,
}

impl Readiness {
    /// No direction ready and no error.
    pub const NONE: Self = Self {
        readable: false,
        writable: false,
        hup: false,
        error: false,
    };
}

/// The per-record levels shared by `refresh_events` and the readiness query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Levels {
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) closed: bool,
}

/// Compute the level of one record with the predicates the edge generator uses.
pub(crate) fn socket_levels(record: &SocketRecord) -> Levels {
    let listener = matches!(record.kind, Kind::Listener { .. });
    let peer_fin = match &record.kind {
        Kind::Tcp(connection) => connection.peer_fin || record.peer_closed,
        _ => record.peer_closed,
    };
    let readable = record.forced_readable
        || !record.rx.is_empty()
        || peer_fin
        || record.closed
        || matches!(&record.kind, Kind::Udp { datagrams, .. } if !datagrams.is_empty())
        || matches!(&record.kind, Kind::Listener { pending, .. } if !pending.is_empty());
    let writable = !record.closed
        && !record.tx_shutdown
        && record.tx.len() < record.tx_cap
        && !listener
        && !matches!(record.kind, Kind::Icmp { .. });
    let closed = record.closed || record.peer_closed;
    Levels {
        readable,
        writable,
        closed,
    }
}

impl<D: Device> Stack<D> {
    /// Return the current readiness of `socket` without consuming anything.
    ///
    /// `EBADF` means the id is not a live socket; `ENOTSOCK` is never returned here because the
    /// id names a socket record. The query does not read bytes, accept a connection, pop a UDP
    /// datagram or take the pending error.
    pub fn readiness(&self, socket: SocketId) -> Result<Readiness, Errno> {
        let record = self.sockets.record(socket)?;
        let levels = socket_levels(record);
        Ok(Readiness {
            readable: levels.readable,
            writable: levels.writable,
            hup: levels.closed,
            error: record.error.is_some(),
        })
    }
}

#[cfg(test)]
#[path = "readiness_ut.rs"]
mod tests;
