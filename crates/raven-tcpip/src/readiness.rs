// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD at donor revision 36eddebcc36cb0775aa2962005b9c05419a6ff01:
//   sys/sys/socketvar.h:450-472      soreadable, sowritable
//   sys/kern/uipc_socket.c:2427-2474  sodopoll, sopoll
//   sys/sys/poll.h:48-53             POLLIN/POLLPRI/POLLOUT/POLLRDNORM/POLLRDBAND
//   sys/sys/socketvar.h:193-201      SS_ISCONNECTED/SS_CANTSENDMORE/SS_CANTRCVMORE/
//                                     SS_POLLRDBAND
// BSD-3-Clause. The NetBSD predicates are ported literally; the Raven socket
// record and the K244 netd seam replace `struct socket`/selectors.

//! Live, non-consuming socket readiness.
//!
//! A readiness doorbell is a hint: the kernel latch says that *something* changed, not which
//! direction is currently ready. `Stack::readiness` asks the same socket state that
//! [`crate::Stack::poll`] uses to generate edges, without reading, accepting, popping a datagram,
//! clearing an error or allocating a socket.
//!
//! The `soreadable`/`sowritable` predicates and the `sodopoll`/`sopoll` event
//! mapping are a literal Rust port of NetBSD `sys/sys/socketvar.h` and
//! `sys/kern/uipc_socket.c` (BSD-3-Clause, NetBSD revision in the file header).
//! The selectors are represented by the K244 netd seam: `sopoll` reports the
//! live mask and whether the caller would have recorded a read/write wakeup.
//! No stored edge mask decides the live query.

use raven_abi::errno::Errno;

use crate::SocketId;
use crate::device::Device;
use crate::event::{POLLIN, POLLOUT, POLLPRI, POLLRDBAND, POLLRDNORM, POLLWRNORM};
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

/// NetBSD `sb_lowat` for the read and write socket buffers (default 1 byte).
const SB_LOWAT: usize = 1;

/// NetBSD `SS_ISCONNECTED` mapped from the Raven TCP state.
///
/// A TCP connection is connected after the handshake; SYN-SENT/SYN-RECEIVED and
/// CLOSED are not. LISTEN has no connection.
fn tcp_connected(connection: &crate::tcp_state::TcpConnection) -> bool {
    matches!(
        connection.state,
        crate::tcp_state::TcpState::Established
            | crate::tcp_state::TcpState::FinWait1
            | crate::tcp_state::TcpState::FinWait2
            | crate::tcp_state::TcpState::CloseWait
            | crate::tcp_state::TcpState::Closing
            | crate::tcp_state::TcpState::LastAck
            | crate::tcp_state::TcpState::TimeWait
    )
}

/// NetBSD `SS_CANTRCVMORE`: the peer finished sending or the socket is closed.
fn cannot_receive_more(record: &SocketRecord) -> bool {
    record.closed
        || record.peer_closed
        || matches!(&record.kind, Kind::Tcp(connection) if connection.peer_fin)
}

/// NetBSD `SS_ISCONNECTED || !(pr_flags & PR_CONNREQUIRED)`.
fn connected_or_no_connection(record: &SocketRecord) -> bool {
    match &record.kind {
        Kind::Tcp(connection) => tcp_connected(connection),
        Kind::Udp { .. } => true,
        Kind::Listener { .. } | Kind::Icmp { .. } => false,
    }
}

/// NetBSD `SS_CANTSENDMORE`: a local write shutdown or a completed close.
fn cannot_send_more(record: &SocketRecord) -> bool {
    record.tx_shutdown || record.closed || record.peer_closed
}

/// NetBSD `soreadable`: can a read return data, EOF or a pending error?
fn soreadable(record: &SocketRecord) -> bool {
    let listener_backlog =
        matches!(&record.kind, Kind::Listener { pending, .. } if !pending.is_empty());
    let datagram_queue =
        matches!(&record.kind, Kind::Udp { datagrams, .. } if !datagrams.is_empty());
    record.rx.len() >= SB_LOWAT
        || record.forced_readable
        || datagram_queue
        || cannot_receive_more(record)
        || listener_backlog
        || record.error.is_some()
}

/// NetBSD `sowritable`: can a write make progress (or fail on a shutdown/error)?
fn sowritable(record: &SocketRecord) -> bool {
    (record.tx.len() < record.tx_cap && connected_or_no_connection(record))
        || cannot_send_more(record)
        || record.error.is_some()
}

/// NetBSD `sodopoll`: map the live predicates to poll event bits.
pub(crate) fn sodopoll(record: &SocketRecord, events: u16) -> u16 {
    let mut revents = 0u16;
    if events & (POLLIN | POLLRDNORM) != 0 && soreadable(record) {
        revents |= events & (POLLIN | POLLRDNORM);
    }
    if events & (POLLOUT | POLLWRNORM) != 0 && sowritable(record) {
        revents |= events & (POLLOUT | POLLWRNORM);
    }
    // NetBSD has no priority-band queue in this Raven socket record, so
    // SS_POLLRDBAND is never set; the event bits are accepted but not returned.
    revents
}

/// Compute the levels from the NetBSD predicates; the edge path and the live
/// query both use these functions.
pub(crate) fn socket_levels(record: &SocketRecord) -> Levels {
    Levels {
        readable: soreadable(record),
        writable: sowritable(record),
        closed: record.closed || record.peer_closed,
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

    /// NetBSD `sodopoll`: return the live event bits for the requested events.
    pub fn sodopoll(&self, socket: SocketId, events: u16) -> Result<u16, Errno> {
        let record = self.sockets.record(socket)?;
        Ok(sodopoll(record, events))
    }

    /// NetBSD `sopoll`: live mask plus the wakeup registration the caller would
    /// make when the mask is zero.
    ///
    /// The K244 netd seam owns the actual selector registration; this method
    /// reports the two booleans so the seam can arm only the needed direction.
    pub fn sopoll(&self, socket: SocketId, events: u16) -> Result<(u16, bool, bool), Errno> {
        let record = self.sockets.record(socket)?;
        let revents = sodopoll(record, events);
        if revents != 0 {
            return Ok((revents, false, false));
        }
        let register_read = events & (POLLIN | POLLPRI | POLLRDNORM | POLLRDBAND) != 0;
        let register_write = events & (POLLOUT | POLLWRNORM) != 0;
        Ok((revents, register_read, register_write))
    }
}

#[cfg(test)]
#[path = "readiness_ut.rs"]
mod tests;
