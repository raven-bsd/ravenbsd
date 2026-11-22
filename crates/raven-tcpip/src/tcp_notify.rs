// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1982, 1986, 1993
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

//! Sans-IO FreeBSD TCP state-change and notification helpers.
//!
//! Ported from FreeBSD `sys/netinet/tcp_subr.c` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`: `tcp_state_change()`,
//! `tcp_notify()` and `tcp_drop_syn_sent()`. The module is not wired into
//! `Stack`/`tcp_state.rs`/`tcp_data.rs`; callers map the outcomes to readiness.
//!
//! Divergences: FreeBSD `tcp_state_change` also updates per-state counters and a DTrace probe;
//! this sans-IO helper records the transition and leaves telemetry to the caller.
//! `tcp_notify` clears a cached route for established unreachable errors; Raven has no route
//! cache in this module, so that side effect is a no-op.

use crate::{Errno, Event, SocketId, TcpState};

/// The transition produced by FreeBSD `tcp_state_change`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpStateChange {
    /// State before the change.
    pub from: TcpState,
    /// State after the change.
    pub to: TcpState,
}

/// The result of FreeBSD `tcp_notify` / `tcp_drop_syn_sent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpNotifyOutcome {
    /// The notification is ignored or the state was not eligible for the drop helper.
    Ignored,
    /// The error is recorded as a soft error; the connection remains up.
    SoftError(Errno),
    /// The connection is dropped; the caller emits the corresponding readiness error.
    Dropped(Errno),
}

impl TcpNotifyOutcome {
    /// Map a drop to the Raven readiness event. Soft errors and ignored notifications are silent.
    #[must_use]
    pub const fn event(self, socket: SocketId) -> Option<Event> {
        match self {
            Self::Ignored | Self::SoftError(_) => None,
            Self::Dropped(error) => Some(Event::Error(socket, error)),
        }
    }
}

/// The state needed by the FreeBSD notification helpers.
#[derive(Clone, Debug)]
pub struct TcpNotify {
    /// `t_state`.
    pub state: TcpState,
    /// `t_rxtshift`.
    pub rxtshift: u8,
    /// `t_softerror`, or `None` for zero.
    pub softerror: Option<Errno>,
    /// `TF_FASTOPEN`.
    pub fastopen: bool,
}

impl TcpNotify {
    /// Create a notification state with no soft error and no Fast Open path.
    #[must_use]
    pub const fn new(state: TcpState) -> Self {
        Self {
            state,
            rxtshift: 0,
            softerror: None,
            fastopen: false,
        }
    }

    /// Port of `tcp_state_change`: record the transition.
    pub fn state_change(&mut self, new_state: TcpState) -> TcpStateChange {
        let from = self.state;
        self.state = new_state;
        TcpStateChange {
            from,
            to: new_state,
        }
    }

    /// Port of `tcp_notify`.
    pub fn notify(&mut self, error: Errno) -> TcpNotifyOutcome {
        if self.state == TcpState::Established
            && (error == Errno::EHOSTUNREACH
                || error == Errno::ENETUNREACH
                || error == Errno::EHOSTDOWN)
        {
            return TcpNotifyOutcome::Ignored;
        }
        if before_established(self.state) && self.rxtshift > 3 && self.softerror.is_some() {
            self.state = TcpState::Closed;
            return TcpNotifyOutcome::Dropped(error);
        }
        self.softerror = Some(error);
        TcpNotifyOutcome::SoftError(error)
    }

    /// Port of `tcp_drop_syn_sent`.
    pub fn drop_syn_sent(&mut self, error: Errno) -> TcpNotifyOutcome {
        if self.state != TcpState::SynSent {
            return TcpNotifyOutcome::Ignored;
        }
        self.fastopen = false;
        self.state = TcpState::Closed;
        TcpNotifyOutcome::Dropped(error)
    }
}

/// FreeBSD `tp->t_state < TCPS_ESTABLISHED`.
const fn before_established(state: TcpState) -> bool {
    matches!(
        state,
        TcpState::Closed | TcpState::Listen | TcpState::SynSent | TcpState::SynReceived
    )
}

#[cfg(test)]
#[path = "tcp_notify_ut.rs"]
mod tests;
