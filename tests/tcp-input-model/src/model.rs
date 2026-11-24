// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room model of the TCP input path exercised by K282.
//!
//! State names and expected outcomes follow RFC 9293 and the RFC 5961 RST/SYN rules.
//! The FreeBSD `tcp_do_segment` port at `133344405f` is the behavioural donor; no donor
//! expression is copied. The model is deliberately smaller than the product: it covers the
//! SYN, SYN_RECEIVED, synchronized-state dispatch, RST, PAWS, window-edge and wrap cases that
//! `tests/tcp-input-model` checks against the real stack.

use raven_abi::errno::Errno;

/// The synchronized and half-open states covered by the input model.
#[allow(dead_code)] // The model keeps the full RFC state set for follow-up properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Passive open waiting for a SYN.
    Listen,
    /// SYN received, SYN|ACK sent.
    SynReceived,
    /// Data transfer.
    Established,
    /// Our FIN sent.
    FinWait1,
    /// Our FIN acknowledged.
    FinWait2,
    /// Peer FIN received.
    CloseWait,
    /// Both FINs sent, ours not acknowledged.
    Closing,
    /// Peer FIN received, our FIN sent.
    LastAck,
    /// No connection.
    Closed,
}

/// What the input path does with one segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Consumed without a reply.
    None,
    /// Send a bare ACK.
    SendAck,
    /// Send SYN|ACK.
    SendSynAck,
    /// Send a bare RST.
    SendRst,
    /// Send the `tcp_dropwithreset` RST|ACK shape.
    DropWithReset,
    /// Send a challenge ACK.
    Challenge,
    /// Close, optionally recording the socket error.
    Close(Option<Errno>),
}

/// TCP header flags used by the model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    /// SYN.
    pub syn: bool,
    /// ACK.
    pub ack: bool,
    /// RST.
    pub rst: bool,
    /// FIN.
    pub fin: bool,
}

impl Flags {
    /// An ACK segment.
    #[must_use]
    pub const fn ack() -> Self {
        Self {
            syn: false,
            ack: true,
            rst: false,
            fin: false,
        }
    }

    /// A SYN segment without ACK.
    #[must_use]
    pub const fn syn() -> Self {
        Self {
            syn: true,
            ack: false,
            rst: false,
            fin: false,
        }
    }

    /// A RST segment.
    #[must_use]
    pub const fn rst() -> Self {
        Self {
            syn: false,
            ack: false,
            rst: true,
            fin: false,
        }
    }
}

/// One inbound segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Sequence number.
    pub seq: u32,
    /// Acknowledgement number.
    pub ack: u32,
    /// Advertised window.
    pub window: u16,
    /// Flags.
    pub flags: Flags,
    /// Payload length in the sequence space.
    pub len: u32,
    /// Timestamp value, when present.
    pub tsval: Option<u32>,
}

impl Segment {
    /// A segment with no payload or timestamp.
    #[must_use]
    pub const fn new(seq: u32, ack: u32, window: u16, flags: Flags) -> Self {
        Self {
            seq,
            ack,
            window,
            flags,
            len: 0,
            tsval: None,
        }
    }

    /// Add a payload length.
    #[must_use]
    pub const fn with_len(mut self, len: u32) -> Self {
        self.len = len;
        self
    }

    /// Add a timestamp value.
    #[must_use]
    pub const fn with_timestamp(mut self, tsval: u32) -> Self {
        self.tsval = Some(tsval);
        self
    }
}

/// RST disposition from RFC 5961 section 3.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetDecision {
    /// `SEQ == last_ack_sent` or `SEQ == RCV.NXT`: close.
    Accept,
    /// In window but not at either edge: challenge ACK.
    Challenge,
    /// Outside the window: ignore.
    Ignore,
}

/// Sequence comparison with RFC 1982 wrapping.
#[must_use]
pub fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// Sequence comparison with RFC 1982 wrapping.
#[must_use]
pub fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// Sequence comparison with RFC 1982 wrapping.
#[must_use]
pub fn seq_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// Sequence comparison with RFC 1982 wrapping.
#[must_use]
pub fn seq_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

/// A model connection.
#[derive(Clone, Debug)]
pub struct Connection {
    /// Current state.
    pub state: State,
    /// Initial send sequence.
    pub iss: u32,
    /// Initial receive sequence.
    pub irs: u32,
    /// Oldest unacknowledged send sequence.
    pub snd_una: u32,
    /// Next send sequence.
    pub snd_nxt: u32,
    /// Highest send sequence.
    pub snd_max: u32,
    /// Send window.
    pub snd_wnd: u32,
    /// Send window scale.
    pub snd_wscale: u8,
    /// Window update sequence.
    pub snd_wl1: u32,
    /// Window update acknowledgement.
    pub snd_wl2: u32,
    /// Next receive sequence.
    pub rcv_nxt: u32,
    /// Receive window.
    pub rcv_wnd: u16,
    /// Sequence of the last ACK sent.
    pub last_ack_sent: u32,
    /// Most recent accepted timestamp.
    pub ts_recent: u32,
    /// Age anchor of `ts_recent`, in milliseconds.
    pub ts_recent_age_ms: u64,
    /// Largest send window seen.
    pub max_snd_wnd: u16,
    /// Bytes accepted into the receive queue.
    pub data_bytes: usize,
    /// PAWS drops.
    pub paws_drops: usize,
    /// RSTs challenged.
    pub rst_challenges: usize,
    /// RSTs ignored as out of window.
    pub rst_ignored: usize,
}

impl Connection {
    /// A passive connection in SYN_RECEIVED after `irs`.
    #[must_use]
    pub fn syn_received(iss: u32, irs: u32) -> Self {
        Self {
            state: State::SynReceived,
            iss,
            irs,
            snd_una: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_max: iss.wrapping_add(1),
            snd_wnd: 0,
            snd_wscale: 0,
            snd_wl1: irs,
            snd_wl2: iss,
            rcv_nxt: irs.wrapping_add(1),
            rcv_wnd: 65_535,
            last_ack_sent: irs.wrapping_add(1),
            ts_recent: 0,
            ts_recent_age_ms: 0,
            max_snd_wnd: 65_535,
            data_bytes: 0,
            paws_drops: 0,
            rst_challenges: 0,
            rst_ignored: 0,
        }
    }

    /// An established connection.
    #[must_use]
    pub fn established(iss: u32, irs: u32) -> Self {
        let mut connection = Self::syn_received(iss, irs);
        connection.state = State::Established;
        connection.snd_una = iss.wrapping_add(1);
        connection.snd_nxt = iss.wrapping_add(1);
        connection.snd_max = iss.wrapping_add(1);
        connection.snd_wnd = 65_535;
        connection.snd_wl1 = irs.wrapping_add(1);
        connection.snd_wl2 = iss.wrapping_add(1);
        connection
    }

    /// RFC 5961 RST disposition for the current window.
    #[must_use]
    pub fn reset_disposition(&self, seq: u32) -> ResetDecision {
        let in_window = if self.rcv_wnd == 0 {
            self.last_ack_sent == seq || self.rcv_nxt == seq
        } else {
            seq_ge(seq, self.last_ack_sent)
                && seq_lt(seq, self.rcv_nxt.wrapping_add(u32::from(self.rcv_wnd)))
        };
        if !in_window {
            return ResetDecision::Ignore;
        }
        if self.last_ack_sent == seq || self.rcv_nxt == seq {
            ResetDecision::Accept
        } else {
            ResetDecision::Challenge
        }
    }

    fn ack_ok(&self, ack: u32) -> bool {
        !seq_gt(ack, self.snd_max)
            && seq_ge(ack, self.snd_una.wrapping_sub(u32::from(self.max_snd_wnd)))
    }

    fn update_snd_wnd(&mut self, seq: u32, ack: u32, window: u16) {
        let scaled = u32::from(window) << self.snd_wscale;
        if seq_lt(self.snd_wl1, seq)
            || (self.snd_wl1 == seq
                && (seq_lt(self.snd_wl2, ack) || (self.snd_wl2 == ack && scaled > self.snd_wnd)))
        {
            self.snd_wnd = scaled;
            self.snd_wl1 = seq;
            self.snd_wl2 = ack;
            self.max_snd_wnd = self
                .max_snd_wnd
                .max(u16::try_from(scaled).unwrap_or(u16::MAX));
        }
    }

    /// Process one inbound segment and return the required reply action.
    pub fn input(&mut self, segment: &Segment, now_ms: u64) -> Action {
        match self.state {
            State::Listen => self.listen(segment),
            State::SynReceived => self.syn_received_input(segment),
            State::Closed => Action::None,
            _ => self.synchronized(segment, now_ms),
        }
    }

    fn listen(&mut self, segment: &Segment) -> Action {
        if segment.flags.rst {
            return Action::None;
        }
        if segment.flags.ack {
            return Action::SendRst;
        }
        if !segment.flags.syn {
            return Action::None;
        }
        self.irs = segment.seq;
        self.rcv_nxt = segment.seq.wrapping_add(1);
        self.snd_una = self.iss;
        self.snd_nxt = self.iss.wrapping_add(1);
        self.snd_max = self.snd_nxt;
        self.last_ack_sent = self.rcv_nxt;
        self.snd_wl1 = segment.seq;
        self.snd_wl2 = self.iss;
        self.state = State::SynReceived;
        Action::SendSynAck
    }

    fn syn_received_input(&mut self, segment: &Segment) -> Action {
        if segment.flags.rst {
            return match self.reset_disposition(segment.seq) {
                ResetDecision::Accept => {
                    self.state = State::Closed;
                    Action::Close(Some(Errno::ECONNREFUSED))
                }
                ResetDecision::Challenge => {
                    self.rst_challenges += 1;
                    Action::Challenge
                }
                ResetDecision::Ignore => {
                    self.rst_ignored += 1;
                    Action::None
                }
            };
        }
        if segment.flags.syn && !segment.flags.ack {
            if segment.seq == self.irs {
                self.last_ack_sent = self.rcv_nxt;
                return Action::SendSynAck;
            }
            if seq_lt(segment.seq, self.irs) {
                return Action::DropWithReset;
            }
            return Action::SendAck;
        }
        if segment.flags.ack {
            if seq_gt(segment.ack, self.snd_una) && seq_le(segment.ack, self.snd_max) {
                self.snd_una = segment.ack;
                let scaled = u32::from(segment.window) << self.snd_wscale;
                self.snd_wnd = scaled;
                self.max_snd_wnd = self.max_snd_wnd.max(segment.window);
                self.snd_wl1 = segment.seq.wrapping_sub(1);
                self.snd_wl2 = segment.ack;
                self.last_ack_sent = self.rcv_nxt;
                self.state = State::Established;
                if segment.len > 0 {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(segment.len);
                    self.data_bytes = self.data_bytes.saturating_add(segment.len as usize);
                }
                if segment.flags.fin && segment.seq.wrapping_add(segment.len) == self.rcv_nxt {
                    self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
                    self.state = State::CloseWait;
                }
                return Action::SendAck;
            }
            return Action::SendRst;
        }
        if segment.flags.fin && segment.seq == self.rcv_nxt {
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
            self.state = State::CloseWait;
            return Action::SendAck;
        }
        Action::None
    }

    fn synchronized(&mut self, segment: &Segment, now_ms: u64) -> Action {
        if segment.flags.rst {
            return match self.reset_disposition(segment.seq) {
                ResetDecision::Accept => {
                    self.state = State::Closed;
                    Action::Close(Some(Errno::ECONNRESET))
                }
                ResetDecision::Challenge => {
                    self.rst_challenges += 1;
                    Action::Challenge
                }
                ResetDecision::Ignore => {
                    self.rst_ignored += 1;
                    Action::None
                }
            };
        }
        if segment.flags.syn {
            return Action::Challenge;
        }
        if self.ts_recent != 0
            && let Some(tsval) = segment.tsval
            && seq_lt(tsval, self.ts_recent)
            && now_ms.saturating_sub(self.ts_recent_age_ms) < 24 * 60 * 60 * 1000
        {
            self.paws_drops += 1;
            return Action::SendAck;
        }
        if segment.flags.ack
            && matches!(
                self.state,
                State::FinWait2 | State::CloseWait | State::Closing | State::LastAck
            )
            && !self.ack_ok(segment.ack)
        {
            return Action::SendAck;
        }
        if segment.flags.ack {
            if seq_gt(segment.ack, self.snd_una) && seq_le(segment.ack, self.snd_max) {
                self.snd_una = segment.ack;
            }
            self.update_snd_wnd(segment.seq, segment.ack, segment.window);
        }
        if segment.len > 0 {
            if seq_ge(
                segment.seq,
                self.rcv_nxt.wrapping_add(u32::from(self.rcv_wnd)),
            ) {
                return Action::SendAck;
            }
            let skip = if seq_lt(segment.seq, self.rcv_nxt) {
                self.rcv_nxt.wrapping_sub(segment.seq)
            } else {
                0
            };
            if skip >= segment.len {
                return Action::SendAck;
            }
            let take = segment.len - skip;
            self.rcv_nxt = self.rcv_nxt.wrapping_add(take);
            self.data_bytes = self.data_bytes.saturating_add(take as usize);
        }
        if segment.flags.fin && segment.seq == self.rcv_nxt {
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
            self.state = match self.state {
                State::Established => State::CloseWait,
                State::FinWait1 => State::Closing,
                State::FinWait2 => State::Closed,
                State::CloseWait | State::Closing | State::LastAck => self.state,
                _ => self.state,
            };
            return Action::SendAck;
        }
        Action::None
    }
}
