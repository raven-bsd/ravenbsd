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

//! TCP state machine model.
//!
//! State names and output flags are a model-only port of FreeBSD
//! `sys/netinet/tcp_fsm.h` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
//! (BSD-3-Clause). The event transitions are clean-room from RFC 9293 Figure 6 and the
//! per-state processing rules. The model match and the table reference are written
//! independently; the property test compares them over randomized event sequences.

/// The eleven RFC 9293 states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// Closed.
    Closed,
    /// Listening.
    Listen,
    /// SYN sent.
    SynSent,
    /// SYN received.
    SynReceived,
    /// Established.
    Established,
    /// Peer FIN received.
    CloseWait,
    /// Our FIN sent.
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

/// One input event or local action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    /// Passive open.
    PassiveOpen,
    /// Active open.
    ActiveOpen,
    /// A SYN segment.
    ReceiveSyn,
    /// A SYN+ACK segment.
    ReceiveSynAck,
    /// A bare ACK segment.
    ReceiveAck,
    /// An ACK+FIN segment.
    ReceiveAckFin,
    /// A FIN segment.
    ReceiveFin,
    /// An RST segment.
    ReceiveRst,
    /// The application closes.
    UserClose,
    /// A retransmission or TIME-WAIT timeout.
    Timeout,
}

/// The resulting state and flags to emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    /// Next state.
    pub next: State,
    /// Emit SYN.
    pub syn: bool,
    /// Emit ACK.
    pub ack: bool,
    /// Emit FIN.
    pub fin: bool,
    /// Emit RST.
    pub rst: bool,
}

impl Transition {
    const fn new(next: State, syn: bool, ack: bool, fin: bool, rst: bool) -> Self {
        Self {
            next,
            syn,
            ack,
            fin,
            rst,
        }
    }

    const fn stay(state: State) -> Self {
        Self::new(state, false, false, false, false)
    }
}

const TH_FIN: u8 = 0x01;
const TH_SYN: u8 = 0x02;
const TH_RST: u8 = 0x04;
const TH_ACK: u8 = 0x10;

/// The FreeBSD `tcp_outflags` table, keyed by state.
#[must_use]
pub const fn output_flags(state: State) -> u8 {
    match state {
        State::Closed => TH_RST | TH_ACK,
        State::Listen => 0,
        State::SynSent => TH_SYN,
        State::SynReceived => TH_SYN | TH_ACK,
        State::Established => TH_ACK,
        State::CloseWait => TH_ACK,
        State::FinWait1 => TH_FIN | TH_ACK,
        State::Closing => TH_FIN | TH_ACK,
        State::LastAck => TH_FIN | TH_ACK,
        State::FinWait2 => TH_ACK,
        State::TimeWait => TH_ACK,
    }
}

/// The model transition, expressed as a match.
#[must_use]
pub fn transition(state: State, event: Event) -> Transition {
    match (state, event) {
        (State::Closed, Event::PassiveOpen) => {
            Transition::new(State::Listen, false, false, false, false)
        }
        (State::Closed, Event::ActiveOpen) => {
            Transition::new(State::SynSent, true, false, false, false)
        }
        (State::Closed, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::Listen, Event::ReceiveSyn) => {
            Transition::new(State::SynReceived, true, true, false, false)
        }
        (State::Listen, Event::ReceiveRst) => Transition::stay(State::Listen),
        (State::Listen, Event::UserClose) => Transition::stay(State::Closed),
        (State::Listen, Event::Timeout) => Transition::stay(State::Listen),
        (State::SynSent, Event::ReceiveSynAck) => {
            Transition::new(State::Established, false, true, false, false)
        }
        (State::SynSent, Event::ReceiveSyn) => {
            Transition::new(State::SynReceived, true, true, false, false)
        }
        (State::SynSent, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::SynSent, Event::UserClose) => Transition::stay(State::Closed),
        (State::SynSent, Event::Timeout) => Transition::stay(State::Closed),
        (State::SynReceived, Event::ReceiveAck) => {
            Transition::new(State::Established, false, true, false, false)
        }
        (State::SynReceived, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::SynReceived, Event::UserClose) => {
            Transition::new(State::FinWait1, false, true, true, false)
        }
        (State::SynReceived, Event::Timeout) => Transition::stay(State::Closed),
        (State::Established, Event::ReceiveFin) => {
            Transition::new(State::CloseWait, false, true, false, false)
        }
        (State::Established, Event::UserClose) => {
            Transition::new(State::FinWait1, false, true, true, false)
        }
        (State::Established, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::Established, Event::Timeout) => Transition::stay(State::Closed),
        (State::CloseWait, Event::UserClose) => {
            Transition::new(State::LastAck, false, true, true, false)
        }
        (State::CloseWait, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::CloseWait, Event::Timeout) => Transition::stay(State::Closed),
        (State::FinWait1, Event::ReceiveAck) => {
            Transition::new(State::FinWait2, false, true, false, false)
        }
        (State::FinWait1, Event::ReceiveFin) => {
            Transition::new(State::Closing, false, true, false, false)
        }
        (State::FinWait1, Event::ReceiveAckFin) => {
            Transition::new(State::TimeWait, false, true, false, false)
        }
        (State::FinWait1, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::FinWait1, Event::Timeout) => Transition::stay(State::Closed),
        (State::FinWait2, Event::ReceiveFin) => {
            Transition::new(State::TimeWait, false, true, false, false)
        }
        (State::FinWait2, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::FinWait2, Event::Timeout) => Transition::stay(State::Closed),
        (State::Closing, Event::ReceiveAck) => {
            Transition::new(State::TimeWait, false, true, false, false)
        }
        (State::Closing, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::Closing, Event::Timeout) => Transition::stay(State::Closed),
        (State::LastAck, Event::ReceiveAck) => Transition::stay(State::Closed),
        (State::LastAck, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::LastAck, Event::Timeout) => Transition::stay(State::Closed),
        (State::TimeWait, Event::ReceiveFin) => Transition::stay(State::TimeWait),
        (State::TimeWait, Event::ReceiveRst) => Transition::stay(State::Closed),
        (State::TimeWait, Event::Timeout) => Transition::stay(State::Closed),
        _ => Transition::stay(state),
    }
}

const REFERENCE: &[(State, Event, Transition)] = &[
    (
        State::Closed,
        Event::PassiveOpen,
        Transition::new(State::Listen, false, false, false, false),
    ),
    (
        State::Closed,
        Event::ActiveOpen,
        Transition::new(State::SynSent, true, false, false, false),
    ),
    (
        State::Listen,
        Event::ReceiveSyn,
        Transition::new(State::SynReceived, true, true, false, false),
    ),
    (
        State::Listen,
        Event::UserClose,
        Transition::stay(State::Closed),
    ),
    (
        State::SynSent,
        Event::ReceiveSynAck,
        Transition::new(State::Established, false, true, false, false),
    ),
    (
        State::SynSent,
        Event::ReceiveSyn,
        Transition::new(State::SynReceived, true, true, false, false),
    ),
    (
        State::SynSent,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::SynSent,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::SynSent,
        Event::UserClose,
        Transition::stay(State::Closed),
    ),
    (
        State::SynReceived,
        Event::ReceiveAck,
        Transition::new(State::Established, false, true, false, false),
    ),
    (
        State::SynReceived,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::SynReceived,
        Event::UserClose,
        Transition::new(State::FinWait1, false, true, true, false),
    ),
    (
        State::SynReceived,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::Established,
        Event::ReceiveFin,
        Transition::new(State::CloseWait, false, true, false, false),
    ),
    (
        State::Established,
        Event::UserClose,
        Transition::new(State::FinWait1, false, true, true, false),
    ),
    (
        State::Established,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::Established,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::CloseWait,
        Event::UserClose,
        Transition::new(State::LastAck, false, true, true, false),
    ),
    (
        State::CloseWait,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::CloseWait,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::FinWait1,
        Event::ReceiveAck,
        Transition::new(State::FinWait2, false, true, false, false),
    ),
    (
        State::FinWait1,
        Event::ReceiveFin,
        Transition::new(State::Closing, false, true, false, false),
    ),
    (
        State::FinWait1,
        Event::ReceiveAckFin,
        Transition::new(State::TimeWait, false, true, false, false),
    ),
    (
        State::FinWait1,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::FinWait1,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::FinWait2,
        Event::ReceiveFin,
        Transition::new(State::TimeWait, false, true, false, false),
    ),
    (
        State::FinWait2,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::FinWait2,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::Closing,
        Event::ReceiveAck,
        Transition::new(State::TimeWait, false, true, false, false),
    ),
    (
        State::Closing,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::Closing,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::LastAck,
        Event::ReceiveAck,
        Transition::stay(State::Closed),
    ),
    (
        State::LastAck,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::LastAck,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
    (
        State::TimeWait,
        Event::ReceiveFin,
        Transition::stay(State::TimeWait),
    ),
    (
        State::TimeWait,
        Event::ReceiveRst,
        Transition::stay(State::Closed),
    ),
    (
        State::TimeWait,
        Event::Timeout,
        Transition::stay(State::Closed),
    ),
];

/// The independent table reference.
#[must_use]
pub fn reference(state: State, event: Event) -> Transition {
    REFERENCE
        .iter()
        .find(|(from, on, _)| *from == state && *on == event)
        .map_or_else(|| Transition::stay(state), |(_, _, next)| *next)
}

/// Human-readable state name.
#[must_use]
pub const fn state_name(state: State) -> &'static str {
    match state {
        State::Closed => "CLOSED",
        State::Listen => "LISTEN",
        State::SynSent => "SYN_SENT",
        State::SynReceived => "SYN_RCVD",
        State::Established => "ESTABLISHED",
        State::CloseWait => "CLOSE_WAIT",
        State::FinWait1 => "FIN_WAIT_1",
        State::Closing => "CLOSING",
        State::LastAck => "LAST_ACK",
        State::FinWait2 => "FIN_WAIT_2",
        State::TimeWait => "TIME_WAIT",
    }
}
