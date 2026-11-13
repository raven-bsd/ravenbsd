// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Readiness mapping for `poll`.
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLOUT, POLLRDHUP};

use crate::record::{SocketRec, SocketState};
use crate::{Sock, WaitItem};

/// What `poll` should register for one socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollInterest {
    /// The kernel handle.
    pub sock: Sock,
    /// Register for readability.
    pub read: bool,
    /// Register for writability.
    pub write: bool,
}

/// Build the poll registration for a record.
pub fn interest(rec: &SocketRec) -> Result<PollInterest, Errno> {
    let sock = rec.sock.ok_or(Errno::EBADF)?;
    let read = !rec.read_shut() && rec.state != SocketState::Closed;
    let write = !rec.write_shut()
        && matches!(
            rec.state,
            SocketState::Open
                | SocketState::Bound
                | SocketState::Connected
                | SocketState::Connecting
                | SocketState::Listening
        );
    Ok(PollInterest { sock, read, write })
}

/// Map one wait result to `revents`.
#[must_use]
pub fn revents(events: i16, ev: &WaitItem) -> i16 {
    let requested = events as u16;
    let mut out = 0u16;
    if ev.got_read && requested & (POLLIN as u16) != 0 {
        out |= POLLIN as u16;
    }
    if ev.got_write && requested & (POLLOUT as u16) != 0 {
        out |= POLLOUT as u16;
    }
    if ev.got_err {
        out |= POLLERR as u16;
    }
    if ev.got_hup {
        out |= POLLHUP as u16;
        if requested & (POLLRDHUP as u16) != 0 {
            out |= POLLRDHUP as u16;
        }
    }
    out as i16
}

#[cfg(test)]
#[path = "ready_ut.rs"]
mod tests;
