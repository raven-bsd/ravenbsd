// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// The `soreadable`/`sowriteable` predicate mapping is derived from FreeBSD
// `sys/sys/socketvar.h`:
//
// Copyright (c) 1982, 1986, 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! Live, non-consuming socket readiness.
//!
//! A readiness doorbell is a hint: the kernel latch says that *something* changed, not which
//! direction is currently ready. `Stack::readiness` asks the same socket state that
//! [`crate::Stack::poll`] uses to generate edges, without reading, accepting, popping a datagram,
//! clearing an error or allocating a socket.
//!
//! The `soreadable`/`sowriteable` predicate mapping below is a Rust port of FreeBSD
//! `sys/sys/socketvar.h` (SPDX-License-Identifier: BSD-3-Clause; Copyright (c) 1982, 1986, 1990,
//! 1993 The Regents of the University of California). RavenBSD modifications remain under the
//! file's BSD-3-Clause notice. The FreeBSD `sopoll` registration/recheck is not in the sparse
//! FreeBSD checkout; that part is clean-room from the existing K244 netd seam and the Illumos
//! `so_poll` behaviour. No NetBSD code is used.

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

/// FreeBSD `sb_lowat` for the read and write socket buffers.
///
/// Raven has one fixed low-water mark; the bounded queues make a value of one byte the
/// equivalent of FreeBSD's default `sb_lowat`.
const SB_LOWAT: usize = 1;

/// FreeBSD `SS_ISCONNECTED` for a Raven TCP record.
///
/// A TCP connection is connected after the handshake; SYN-SENT/SYN-RECEIVED and CLOSED are not.
/// LISTEN has no connection. The state set is the RFC 9293 established set already used by the
/// Raven TCP state machine, not a new state machine.
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

/// FreeBSD `SBS_CANTRCVMORE`: the peer finished sending, or the connection is fully closed.
fn cannot_receive_more(record: &SocketRecord) -> bool {
    record.closed
        || record.peer_closed
        || matches!(&record.kind, Kind::Tcp(connection) if connection.peer_fin)
}

/// FreeBSD `SS_ISCONNECTED || !(pr_flags & PR_CONNREQUIRED)`.
fn connected_or_no_connection(record: &SocketRecord) -> bool {
    match &record.kind {
        // TCP requires a connection; LISTEN is not connected.
        Kind::Tcp(connection) => tcp_connected(connection),
        // UDP does not require a connection (PR_CONNREQUIRED is not set).
        Kind::Udp { .. } => true,
        // The Raven ICMP record is an echo-reply queue, not a FreeBSD raw socket; it has no
        // write path and therefore keeps the K244 non-writable contract.
        Kind::Listener { .. } | Kind::Icmp { .. } => false,
    }
}

/// FreeBSD `SBS_CANTSENDMORE`: a local write shutdown or a completed/disconnected close.
fn cannot_send_more(record: &SocketRecord) -> bool {
    record.tx_shutdown || record.closed || record.peer_closed
}

/// Compute the level of one record with the FreeBSD socket predicates.
pub(crate) fn socket_levels(record: &SocketRecord) -> Levels {
    let listener_backlog =
        matches!(&record.kind, Kind::Listener { pending, .. } if !pending.is_empty());
    let datagram_queue =
        matches!(&record.kind, Kind::Udp { datagrams, .. } if !datagrams.is_empty());
    let readable = record.forced_readable
        || record.rx.len() >= SB_LOWAT
        || datagram_queue
        || cannot_receive_more(record)
        || listener_backlog
        || record.error.is_some();
    let writable = (record.tx.len() < record.tx_cap && connected_or_no_connection(record))
        || cannot_send_more(record)
        || record.error.is_some();
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
