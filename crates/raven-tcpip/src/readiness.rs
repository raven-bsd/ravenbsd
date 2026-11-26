// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// The `soreadable`/`sowritable` predicates and the `sodopoll` mapping are a
// Rust port of NetBSD `sys/sys/socketvar.h` and `sys/kern/uipc_socket.c`:
//
// Copyright (c) 2008, 2009 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Andrew Doran.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2002, 2007, 2008, 2009, 2023 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Jason R. Thorpe of Wasabi Systems, Inc, and by Andrew Doran.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2004 The FreeBSD Foundation
// Copyright (c) 2004 Robert Watson
// Copyright (c) 1982, 1986, 1988, 1990, 1993
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
//
// NetBSD donor revision: 36eddebcc36cb0775aa2962005b9c05419a6ff01.
// RavenBSD modifications: the donor's `struct socket`/`struct sockbuf` state is
// mapped onto `SocketRecord`, and the NetBSD receive-only error is represented
// by the single Raven `record.error`. The `test_set_readable` seam is Raven-only.

//! Live, non-consuming socket readiness.
//!
//! A readiness doorbell is a hint: the kernel latch says that *something* changed, not which
//! direction is currently ready. `Stack::readiness` asks the same NetBSD socket predicates that
//! [`crate::Stack::poll`] uses to generate edges, without reading, accepting, popping a datagram,
//! clearing an error or allocating a socket.
//!
//! The level source is the literal NetBSD pair `soreadable`/`sowritable`
//! (`sys/sys/socketvar.h`) mapped through `sodopoll` (`sys/kern/uipc_socket.c`): receive bytes
//! against `sb_lowat`, `SS_CANTRCVMORE`, `so_qlen`, `sbspace` against `sb_lowat`,
//! `SS_ISCONNECTED`/`PR_CONNREQUIRED`, `SS_CANTSENDMORE`, and the socket error. `sopoll` is the
//! quick level check; the caller owns registration and re-check, as in the K244 netd seam.
//!
//! Raven divergences, recorded so the port is not mistaken for NetBSD itself:
//! * one `record.error` stands for both `so_error` and `so_rerror`;
//! * a `Kind::Icmp` record is an echo-reply queue with no send path, so it is not writable even
//!   though a NetBSD raw socket without `PR_CONNREQUIRED` would be;
//! * `Event::Closed`/`Readiness::hup` come from the record's close level, where NetBSD raises
//!   `POLLHUP` through `sowakeup`/`soisdisconnected`;
//! * `forced_readable` is a Raven C15 test seam and not a predicate input.

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

/// NetBSD poll event bits (`sys/sys/poll.h`).
pub(crate) const POLLIN: u32 = 0x0001;
/// Urgent/out-of-band read event; Raven has no `SS_POLLRDBAND` equivalent.
pub(crate) const POLLPRI: u32 = 0x0002;
/// Normal read event.
pub(crate) const POLLRDNORM: u32 = 0x0040;
/// Normal write event.
pub(crate) const POLLOUT: u32 = 0x0004;
/// Normal write event; NetBSD aliases `POLLWRNORM` to `POLLOUT`.
pub(crate) const POLLWRNORM: u32 = POLLOUT;
/// Out-of-band read band; Raven has no `SS_POLLRDBAND` state that raises it.
pub(crate) const POLLRDBAND: u32 = 0x0080;

/// NetBSD `sb_lowat` for both socket buffers.
///
/// Raven has one fixed low-water mark; the bounded queues make a value of one byte the
/// equivalent of the NetBSD default.
const SB_LOWAT: usize = 1;

/// NetBSD `SS_ISCONNECTED` for a Raven TCP record.
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

/// NetBSD `sbspace` for one Raven socket buffer.
///
/// The byte bound is `sb_hiwat - sb_cc`; Raven has no separate `sb_mbmax` accounting. A full
/// buffer returns zero, exactly as NetBSD does when `sb_hiwat <= sb_cc`.
fn sbspace(queued: usize, capacity: usize) -> usize {
    capacity.saturating_sub(queued)
}

/// NetBSD `SS_CANTRCVMORE`: the peer finished sending, or the connection is fully closed.
fn cantrcvmore(record: &SocketRecord) -> bool {
    record.closed
        || record.peer_closed
        || matches!(&record.kind, Kind::Tcp(connection) if connection.peer_fin)
}

/// NetBSD `SS_CANTSENDMORE`: a local write shutdown or a completed/disconnected close.
fn cantsendmore(record: &SocketRecord) -> bool {
    record.tx_shutdown || record.closed || record.peer_closed
}

/// `SS_ISCONNECTED || !(pr_flags & PR_CONNREQUIRED)`.
fn connected_or_no_connection(record: &SocketRecord) -> bool {
    match &record.kind {
        // TCP requires a connection; LISTEN is not connected.
        Kind::Tcp(connection) => tcp_connected(connection),
        // UDP does not require a connection (PR_CONNREQUIRED is not set).
        Kind::Udp { .. } => true,
        // Raven's ICMP record is an echo-reply queue, not a raw socket with a send path.
        Kind::Listener { .. } | Kind::Icmp { .. } => false,
    }
}

/// NetBSD `so_rcv.sb_cc >= sb_lowat`, with the Raven datagram queue standing in for `sb_cc`.
fn receive_queued(record: &SocketRecord) -> bool {
    match &record.kind {
        // `sbappendaddr` counts the address header, so a queued zero-length datagram is
        // readable in NetBSD too.
        Kind::Udp { datagrams, .. } => !datagrams.is_empty(),
        _ => record.rx.len() >= SB_LOWAT,
    }
}

/// NetBSD `so_qlen != 0`: the listener's completed-connection queue.
fn listener_qlen(record: &SocketRecord) -> bool {
    matches!(&record.kind, Kind::Listener { pending, .. } if !pending.is_empty())
}

/// NetBSD `soreadable`: can we read something from this socket?
pub(crate) fn soreadable(record: &SocketRecord) -> bool {
    receive_queued(record) || cantrcvmore(record) || listener_qlen(record) || record.error.is_some()
}

/// NetBSD `sowritable`: can we write something to this socket?
pub(crate) fn sowritable(record: &SocketRecord) -> bool {
    (sbspace(record.tx.len(), record.tx_cap) >= SB_LOWAT && connected_or_no_connection(record))
        || cantsendmore(record)
        || record.error.is_some()
}

/// NetBSD `sodopoll`: map the live predicates to the requested poll event bits.
pub(crate) fn sodopoll(record: &SocketRecord, events: u32) -> u32 {
    let mut revents = 0;
    if events & (POLLIN | POLLRDNORM) != 0 && soreadable(record) {
        revents |= events & (POLLIN | POLLRDNORM);
    }
    if events & (POLLOUT | POLLWRNORM) != 0 && sowritable(record) {
        revents |= events & (POLLOUT | POLLWRNORM);
    }
    // NetBSD also reports POLLPRI|POLLRDBAND when SS_POLLRDBAND is set. Raven rejects
    // out-of-band sends and has no equivalent state bit, so no urgent bit is raised.
    revents
}

/// NetBSD `sopoll`: the quick level check before registration and re-check.
///
/// Registration belongs to the event seam (`Stack::poll` records the transition against the
/// previous level, and netd re-queries); no stored-mask verdict is kept here.
pub(crate) fn sopoll(record: &SocketRecord) -> u32 {
    sodopoll(
        record,
        POLLIN | POLLRDNORM | POLLOUT | POLLWRNORM | POLLPRI | POLLRDBAND,
    )
}

/// Compute the level of one record with the NetBSD socket predicates.
pub(crate) fn socket_levels(record: &SocketRecord) -> Levels {
    let revents = sopoll(record);
    // Raven C15 test seam: `test_set_readable` forces a transition without data. It is not a
    // NetBSD predicate and never decides the live K244 query on its own.
    let readable = revents & (POLLIN | POLLRDNORM) != 0 || record.forced_readable;
    let writable = revents & POLLOUT != 0;
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
