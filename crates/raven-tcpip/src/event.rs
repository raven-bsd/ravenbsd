// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Readiness events and poll results (contract C15).

use raven_abi::errno::Errno;

/// A socket identifier.
///
/// Ids are allocated monotonically and never reused until the `u32` counter wraps. Callers may
/// therefore keep an id in an event queue without confusing it with a later socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SocketId(pub u32);

/// A readiness transition observed by [`crate::Stack::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The socket has data that can be read.
    Readable(SocketId),
    /// The socket has room for more data.
    Writable(SocketId),
    /// The peer closed the connection or the local close completed.
    Closed(SocketId),
    /// The connection failed; the socket is closed.
    Error(SocketId, Errno),
    /// A listener completed a handshake and has a connection ready for `tcp_accept`.
    Accepted {
        /// The listening socket.
        listener: SocketId,
        /// The new connection socket.
        conn: SocketId,
    },
}

/// Counters for one [`crate::Stack::poll`] call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PollResult {
    /// Frames received from the device during this poll.
    pub rx_frames: usize,
    /// Frames handed to the device during this poll.
    pub tx_frames: usize,
    /// Frames or socket writes dropped because a configured limit was reached.
    pub dropped: usize,
}
