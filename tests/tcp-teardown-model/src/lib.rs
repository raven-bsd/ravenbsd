// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only reference for TCP teardown, FIN, 2MSL and RST.
//!
//! The state transitions are the RFC 9293 Figure 6 teardown path, cross-checked against
//! FreeBSD `sys/netinet/tcp_fsm.h` and `tcp_input.c` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause, behaviour only). No donor
//! expression is copied. The tests drive the public `raven_tcpip::Stack` API and compare every
//! observed state and timer deadline with this model.

#![forbid(unsafe_code)]

use raven_tcpip::{Instant, TcpState};

/// Default `tcp.time_wait_interval`: FreeBSD `TT_2MSL = 2 * TCPTV_MSL`.
pub const TIME_WAIT_MS: u64 = 60_000;
/// Default `tcp.finwait2_timeout`.
pub const FIN_WAIT2_MS: u64 = 60_000;

/// The reference teardown state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefState {
    /// No connection.
    Closed,
    /// Passive listener.
    Listen,
    /// Active SYN sent.
    SynSent,
    /// SYN received.
    SynReceived,
    /// Data transfer.
    Established,
    /// Peer FIN received.
    CloseWait,
    /// Our FIN sent, waiting for its ACK.
    FinWait1,
    /// Both FINs exchanged, our FIN not acknowledged.
    Closing,
    /// Peer FIN received, our FIN sent.
    LastAck,
    /// Our FIN acknowledged.
    FinWait2,
    /// TIME-WAIT.
    TimeWait,
}

impl From<TcpState> for RefState {
    fn from(state: TcpState) -> Self {
        match state {
            TcpState::Closed => Self::Closed,
            TcpState::Listen => Self::Listen,
            TcpState::SynSent => Self::SynSent,
            TcpState::SynReceived => Self::SynReceived,
            TcpState::Established => Self::Established,
            TcpState::CloseWait => Self::CloseWait,
            TcpState::FinWait1 => Self::FinWait1,
            TcpState::Closing => Self::Closing,
            TcpState::LastAck => Self::LastAck,
            TcpState::FinWait2 => Self::FinWait2,
            TcpState::TimeWait => Self::TimeWait,
        }
    }
}

/// One teardown event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The application closed or shut down its write half.
    LocalClose,
    /// A bare ACK arrived.
    RecvAck,
    /// A FIN arrived without an ACK for our FIN.
    RecvFin,
    /// A FIN arrived together with the ACK for our FIN.
    RecvFinAck,
    /// A RST arrived.
    RecvRst,
    /// The 2MSL or FIN-WAIT-2 timer fired.
    TimerExpired,
}

/// The RFC 9293 teardown transition.
#[must_use]
pub fn transition(state: RefState, event: Event) -> RefState {
    if event == Event::RecvRst {
        return RefState::Closed;
    }
    match (state, event) {
        (RefState::Established, Event::LocalClose) => RefState::FinWait1,
        (RefState::Established, Event::RecvFin) | (RefState::Established, Event::RecvFinAck) => {
            RefState::CloseWait
        }
        (RefState::FinWait1, Event::RecvAck) => RefState::FinWait2,
        (RefState::FinWait1, Event::RecvFin) => RefState::Closing,
        (RefState::FinWait1, Event::RecvFinAck) => RefState::TimeWait,
        (RefState::FinWait2, Event::RecvFin) | (RefState::FinWait2, Event::RecvFinAck) => {
            RefState::TimeWait
        }
        (RefState::Closing, Event::RecvAck) => RefState::TimeWait,
        (RefState::CloseWait, Event::LocalClose) => RefState::LastAck,
        (RefState::LastAck, Event::RecvAck) => RefState::Closed,
        (RefState::LastAck, Event::RecvFin) | (RefState::LastAck, Event::RecvFinAck) => {
            RefState::LastAck
        }
        (RefState::TimeWait, Event::TimerExpired) | (RefState::FinWait2, Event::TimerExpired) => {
            RefState::Closed
        }
        (RefState::Listen, Event::LocalClose) => RefState::Closed,
        (RefState::Closed, _) => RefState::Closed,
        (state, _) => state,
    }
}

/// The timer deadline expected after entering `state` at `now`.
#[must_use]
pub fn deadline(state: RefState, now: Instant) -> Option<Instant> {
    let interval = match state {
        RefState::TimeWait => TIME_WAIT_MS,
        RefState::FinWait2 => FIN_WAIT2_MS,
        _ => return None,
    };
    Some(Instant::from_millis(
        now.as_millis()
            .saturating_add(i64::try_from(interval).unwrap_or(i64::MAX)),
    ))
}
