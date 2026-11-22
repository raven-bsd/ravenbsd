// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Send-window model and a naive offset reference.
//!
//! Behaviour reference: FreeBSD `sys/netinet/tcp_input.c` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-3-Clause), window update block
//! (`snd_una`/`snd_nxt`/`snd_wnd`/`snd_wl1`/`snd_wl2`), and RFC 9293 Section 3.8.4.
//! The zero-window probe sends one byte when the window is closed. The naive reference uses
//! monotonic byte offsets instead of serial-number comparisons; no donor code is copied here.

use crate::seq::{seq_distance, seq_ge, seq_gt, seq_le, seq_lt, seq_max};

/// A send window over wrapping 32-bit sequence numbers.
#[derive(Clone, Debug)]
pub struct SendWindow {
    /// Initial send sequence.
    pub iss: u32,
    /// Oldest unacknowledged sequence.
    pub snd_una: u32,
    /// Next sequence to send.
    pub snd_nxt: u32,
    /// Highest sequence ever sent.
    pub snd_max: u32,
    /// Current peer window.
    pub snd_wnd: u32,
    /// Largest peer window observed.
    pub max_snd_wnd: u32,
    /// Sequence of the last accepted window update.
    pub snd_wl1: u32,
    /// Acknowledgement of the last accepted window update.
    pub snd_wl2: u32,
    /// Negotiated window scale.
    pub snd_wscale: u8,
    /// Sender MSS.
    pub mss: u32,
    /// Congestion window.
    pub cwnd: u32,
}

impl SendWindow {
    /// Start with a zero peer window (the handshake has not supplied one yet).
    #[must_use]
    pub const fn new(iss: u32, mss: u32, cwnd: u32) -> Self {
        Self {
            iss,
            snd_una: iss,
            snd_nxt: iss,
            snd_max: iss,
            snd_wnd: 0,
            max_snd_wnd: 0,
            snd_wl1: 0,
            snd_wl2: 0,
            snd_wscale: 0,
            mss,
            cwnd,
        }
    }

    /// Bytes sent but not cumulatively acknowledged.
    #[must_use]
    pub fn inflight(&self) -> u32 {
        seq_distance(self.snd_una, self.snd_nxt)
    }

    /// Bytes the sender may put on the wire now.
    #[must_use]
    pub fn usable(&self, wnd: u32) -> u32 {
        self.cwnd
            .min(self.snd_wnd.min(wnd))
            .saturating_sub(self.inflight())
    }

    /// Send up to `len` bytes within the `wnd` snapshot.
    pub fn send(&mut self, len: u32, wnd: u32) -> u32 {
        let sent = len.min(self.usable(wnd));
        self.snd_nxt = self.snd_nxt.wrapping_add(sent);
        self.snd_max = seq_max(self.snd_max, self.snd_nxt);
        sent
    }

    /// Acknowledge up to `ack`; returns the newly acknowledged byte count.
    pub fn ack(&mut self, ack: u32) -> u32 {
        if seq_gt(ack, self.snd_una) && seq_le(ack, self.snd_max) {
            let acked = seq_distance(self.snd_una, ack);
            self.snd_una = ack;
            acked
        } else {
            0
        }
    }

    /// Apply a window update with the RFC 9293 / FreeBSD `snd_wl1`/`snd_wl2` ordering rule.
    /// Returns true when the update was accepted.
    pub fn update_window(&mut self, seq: u32, ack: u32, wnd: u16) -> bool {
        let scaled = u32::from(wnd) << self.snd_wscale.min(14);
        let newer = seq_lt(self.snd_wl1, seq)
            || (seq == self.snd_wl1
                && (seq_lt(self.snd_wl2, ack) || (ack == self.snd_wl2 && scaled > self.snd_wnd)));
        if !newer {
            return false;
        }
        self.snd_wnd = scaled;
        self.max_snd_wnd = self.max_snd_wnd.max(scaled);
        self.snd_wl1 = seq;
        self.snd_wl2 = ack;
        true
    }

    /// Zero-window probe: with a closed window and queued data, send one byte.
    pub fn persist_probe(&mut self, queued: u32) -> bool {
        if self.snd_wnd != 0 || queued == 0 || self.inflight() >= self.cwnd {
            return false;
        }
        self.snd_nxt = self.snd_nxt.wrapping_add(1);
        self.snd_max = seq_max(self.snd_max, self.snd_nxt);
        true
    }

    /// Whether `ack` is a valid cumulative acknowledgement.
    #[must_use]
    pub fn valid_ack(&self, ack: u32) -> bool {
        seq_ge(ack, self.snd_una) && seq_le(ack, self.snd_max)
    }

    /// Whether an update with this sequence, acknowledgement and window is newer.
    #[must_use]
    pub fn window_is_newer(&self, seq: u32, ack: u32, wnd: u16) -> bool {
        let scaled = u32::from(wnd) << self.snd_wscale.min(14);
        seq_lt(self.snd_wl1, seq)
            || (seq == self.snd_wl1
                && (seq_lt(self.snd_wl2, ack) || (ack == self.snd_wl2 && scaled > self.snd_wnd)))
    }
}

/// A naive send window over monotonic byte offsets.
#[derive(Clone, Debug)]
pub struct NaiveWindow {
    /// Oldest unacknowledged offset.
    pub snd_una: u32,
    /// Next send offset.
    pub snd_nxt: u32,
    /// Highest sent offset.
    pub snd_max: u32,
    /// Peer window.
    pub snd_wnd: u32,
    /// Largest peer window.
    pub max_snd_wnd: u32,
    /// Last accepted window sequence offset.
    pub snd_wl1: u32,
    /// Last accepted window acknowledgement offset.
    pub snd_wl2: u32,
    /// Window scale.
    pub snd_wscale: u8,
    /// MSS.
    pub mss: u32,
    /// Congestion window.
    pub cwnd: u32,
}

impl NaiveWindow {
    /// Create a zero-window naive sender.
    #[must_use]
    pub const fn new(mss: u32, cwnd: u32) -> Self {
        Self {
            snd_una: 0,
            snd_nxt: 0,
            snd_max: 0,
            snd_wnd: 0,
            max_snd_wnd: 0,
            snd_wl1: 0,
            snd_wl2: 0,
            snd_wscale: 0,
            mss,
            cwnd,
        }
    }

    /// In-flight bytes.
    #[must_use]
    pub fn inflight(&self) -> u32 {
        self.snd_nxt.saturating_sub(self.snd_una)
    }

    /// Usable bytes.
    #[must_use]
    pub fn usable(&self, wnd: u32) -> u32 {
        self.cwnd
            .min(self.snd_wnd.min(wnd))
            .saturating_sub(self.inflight())
    }

    /// Send up to `len` bytes.
    pub fn send(&mut self, len: u32, wnd: u32) -> u32 {
        let sent = len.min(self.usable(wnd));
        self.snd_nxt = self.snd_nxt.saturating_add(sent);
        self.snd_max = self.snd_max.max(self.snd_nxt);
        sent
    }

    /// Acknowledge up to `ack` offset.
    pub fn ack(&mut self, ack: u32) -> u32 {
        if ack > self.snd_una && ack <= self.snd_max {
            let acked = ack - self.snd_una;
            self.snd_una = ack;
            acked
        } else {
            0
        }
    }

    /// Accept a newer window update (plain offset ordering).
    pub fn update_window(&mut self, seq: u32, ack: u32, wnd: u16) -> bool {
        let scaled = u32::from(wnd) << self.snd_wscale.min(14);
        let newer = seq > self.snd_wl1
            || (seq == self.snd_wl1
                && (ack > self.snd_wl2 || (ack == self.snd_wl2 && scaled > self.snd_wnd)));
        if !newer {
            return false;
        }
        self.snd_wnd = scaled;
        self.max_snd_wnd = self.max_snd_wnd.max(scaled);
        self.snd_wl1 = seq;
        self.snd_wl2 = ack;
        true
    }

    /// Persist probe over offsets.
    pub fn persist_probe(&mut self, queued: u32) -> bool {
        if self.snd_wnd != 0 || queued == 0 || self.inflight() >= self.cwnd {
            return false;
        }
        self.snd_nxt = self.snd_nxt.saturating_add(1);
        self.snd_max = self.snd_max.max(self.snd_nxt);
        true
    }
}
