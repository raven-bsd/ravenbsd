// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent RFC 7323 PAWS and sequence-space reference model.

/// 24 days in milliseconds (`TCP_PAWS_IDLE` in the FreeBSD/NetBSD donor).
pub const TCP_PAWS_IDLE_MS: u64 = 24 * 24 * 60 * 60 * 1000;

/// Wrapping sequence comparison (`SEQ_LT`).
#[must_use]
pub const fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// Wrapping sequence comparison (`SEQ_LEQ`).
#[must_use]
pub const fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

/// Wrapping sequence comparison (`SEQ_GEQ`).
#[must_use]
pub const fn seq_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

/// One incoming ACK/data segment.
#[derive(Clone, Copy, Debug)]
pub struct Segment<'a> {
    /// Segment sequence.
    pub seq: u32,
    /// Timestamp value, when the TS option is present.
    pub tsval: Option<u32>,
    /// Payload bytes; the sequence-space length is `payload.len()`.
    pub payload: &'a [u8],
}

impl<'a> Segment<'a> {
    /// A segment with no timestamp.
    #[must_use]
    pub const fn new(seq: u32, payload: &'a [u8]) -> Self {
        Self {
            seq,
            tsval: None,
            payload,
        }
    }

    /// Add a timestamp value.
    #[must_use]
    pub const fn with_timestamp(mut self, tsval: u32) -> Self {
        self.tsval = Some(tsval);
        self
    }
}

/// The outcome of one reference input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Bytes accepted into the receive stream.
    pub accepted: usize,
    /// A pure ACK was sent.
    pub ack: bool,
    /// PAWS dropped the segment.
    pub paws_drop: bool,
    /// The old `ts_recent` was aged out (24-day exception).
    pub aged_out: bool,
    /// `ts_recent` was updated from this segment.
    pub ts_recent_updated: bool,
}

/// A reference TCP connection for PAWS and sequence-wrap comparison.
#[derive(Clone, Debug)]
pub struct Connection {
    /// Next expected sequence.
    pub rcv_nxt: u32,
    /// Receive window.
    pub rcv_wnd: u32,
    /// Last ACK sent (the RFC 7323 `Last.ACK.sent`).
    pub last_ack_sent: u32,
    /// Most recent accepted timestamp.
    pub ts_recent: u32,
    /// Age anchor of `ts_recent`.
    pub ts_recent_age_ms: u64,
    /// Whether timestamps were negotiated.
    pub timestamps: bool,
    /// PAWS drops observed.
    pub paws_drops: usize,
    /// A sequence wrap occurred.
    pub saw_wrap: bool,
    accepted: Vec<u8>,
}

impl Connection {
    /// An established connection at `rcv_nxt`.
    #[must_use]
    pub fn established(rcv_nxt: u32, ts_recent: u32, now_ms: u64) -> Self {
        Self {
            rcv_nxt,
            rcv_wnd: 65_535,
            last_ack_sent: rcv_nxt,
            ts_recent,
            ts_recent_age_ms: now_ms,
            timestamps: true,
            paws_drops: 0,
            saw_wrap: false,
            accepted: Vec::new(),
        }
    }

    /// Feed one segment through the reference model.
    pub fn receive(&mut self, now_ms: u64, segment: &Segment<'_>) -> Outcome {
        let mut outcome = Outcome::default();
        let length = u32::try_from(segment.payload.len()).unwrap_or(u32::MAX);

        // RFC 7323 section 5.4 / FreeBSD tcp_input.c: RFC1323 PAWS. A segment whose
        // timestamp is behind ts_recent is dropped unless ts_recent is older than 24 days;
        // in that case ts_recent is invalidated and the segment is allowed through.
        if self.timestamps
            && let Some(tsval) = segment.tsval
            && self.ts_recent != 0
            && seq_lt(tsval, self.ts_recent)
        {
            if now_ms.saturating_sub(self.ts_recent_age_ms) > TCP_PAWS_IDLE_MS {
                self.ts_recent = 0;
                outcome.aged_out = true;
            } else {
                self.paws_drops = self.paws_drops.saturating_add(1);
                outcome.paws_drop = true;
                outcome.ack = true;
                return outcome;
            }
        }

        // RFC 7323 section 5.3 / FreeBSD tcp_input.c: record the timestamp when the
        // last ACK sent falls inside this segment's sequence space. This intentionally
        // uses Last.ACK.sent, not "any newer timestamp".
        if self.timestamps
            && let Some(tsval) = segment.tsval
        {
            let end = segment.seq.wrapping_add(length);
            if seq_le(segment.seq, self.last_ack_sent) && seq_le(self.last_ack_sent, end) {
                self.ts_recent = tsval;
                self.ts_recent_age_ms = now_ms;
                outcome.ts_recent_updated = true;
            }
        }

        if length > 0 {
            let right = self.rcv_nxt.wrapping_add(self.rcv_wnd);
            if seq_ge(segment.seq, right) {
                outcome.ack = true;
                return outcome;
            }
            let skip = if seq_lt(segment.seq, self.rcv_nxt) {
                self.rcv_nxt.wrapping_sub(segment.seq)
            } else {
                0
            };
            if skip >= length {
                outcome.ack = true;
                return outcome;
            }
            let take = length - skip;
            let start = usize::try_from(skip).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(take).unwrap_or(0));
            if let Some(bytes) = segment.payload.get(start..end) {
                self.accepted.extend_from_slice(bytes);
            }
            self.rcv_nxt = self.rcv_nxt.wrapping_add(take);
            self.last_ack_sent = self.rcv_nxt;
            outcome.accepted = usize::try_from(take).unwrap_or(usize::MAX);
            if segment.seq.wrapping_add(length) < segment.seq {
                self.saw_wrap = true;
            }
        }
        outcome
    }

    /// Bytes accepted into the stream.
    #[must_use]
    pub fn stream(&self) -> &[u8] {
        &self.accepted
    }

    /// FNV-1a hash of the accepted stream.
    #[must_use]
    pub fn stream_hash(&self) -> u64 {
        fnv1a(&self.accepted)
    }
}

/// FNV-1a 64-bit hash.
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
