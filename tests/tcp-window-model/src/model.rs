// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room reference model for the TCP receive window, persist probe, SWS and
//! window-update rules.
//!
//! Behaviour follows RFC 9293 and the FreeBSD `tcp_output`/`tcp_timer_persist` semantics
//! already ported by K265/K266; no donor expression is copied. The model works in byte
//! offsets and a 64-bit FNV-1a stream hash, so the real-stack tests can compare every
//! delivered byte and the receive-window state at quiescent points.

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Streaming FNV-1a hasher used by the model and the real-stack tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamHash(u64);

impl StreamHash {
    /// Start a new hash.
    #[must_use]
    pub const fn new() -> Self {
        Self(FNV_OFFSET)
    }

    /// Add bytes in delivery order.
    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// Final 64-bit value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl Default for StreamHash {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience hash for one byte slice.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = StreamHash::new();
    hash.update(bytes);
    hash.value()
}

/// The reference receive window and sender state.
#[derive(Clone, Debug)]
pub struct WindowModel {
    /// Receive buffer capacity in bytes.
    pub rx_cap: usize,
    /// Bytes currently queued for the application.
    pub queued: usize,
    /// Sender MSS.
    pub mss: u16,
    /// Receive-window scale.
    pub rcv_scale: u8,
    /// Peer-advertised send window.
    pub snd_wnd: u32,
    /// Oldest unacknowledged send sequence offset.
    pub snd_una: u32,
    /// Next send sequence offset.
    pub snd_nxt: u32,
    /// Highest send sequence offset.
    pub snd_max: u32,
    /// Window update sequence offset.
    pub snd_wl1: u32,
    /// Window update acknowledgement offset.
    pub snd_wl2: u32,
    /// Largest advertised send window.
    pub max_snd_wnd: u16,
    /// Persist probes sent.
    pub probes: usize,
    /// Persist backoff shift.
    pub backoff_shift: u8,
    /// Bytes accepted for delivery.
    pub delivered: usize,
    /// Hash of delivered bytes.
    pub hash: StreamHash,
}

impl WindowModel {
    /// A receiver with `rx_cap` bytes, an MSS and a window scale.
    #[must_use]
    pub fn new(rx_cap: usize, mss: u16, rcv_scale: u8) -> Self {
        Self {
            rx_cap,
            queued: 0,
            mss,
            rcv_scale: rcv_scale.min(14),
            snd_wnd: 0,
            snd_una: 0,
            snd_nxt: 0,
            snd_max: 0,
            snd_wl1: 0,
            snd_wl2: 0,
            max_snd_wnd: 0,
            probes: 0,
            backoff_shift: 0,
            delivered: 0,
            hash: StreamHash::new(),
        }
    }

    /// Advertise the receive window with the product's MSS/half-buffer floor.
    #[must_use]
    pub fn advertised_window(&self) -> u16 {
        let free = self.rx_cap.saturating_sub(self.queued);
        let floor = usize::from(self.mss).max(self.rx_cap / 2).max(1);
        let value = if free < floor && free < self.rx_cap {
            0
        } else {
            free >> self.rcv_scale
        };
        u16::try_from(value.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)
    }

    /// Accept in-order bytes into the receive queue; returns the accepted count.
    pub fn accept(&mut self, bytes: &[u8]) -> usize {
        let room = self.rx_cap.saturating_sub(self.queued);
        let take = bytes.len().min(room);
        self.queued += take;
        self.delivered += take;
        self.hash.update(&bytes[..take]);
        take
    }

    /// Drain `count` bytes to the application; returns the drained count.
    pub fn drain(&mut self, count: usize) -> usize {
        let take = count.min(self.queued);
        self.queued -= take;
        take
    }

    /// The current free receive space.
    #[must_use]
    pub fn free(&self) -> usize {
        self.rx_cap.saturating_sub(self.queued)
    }

    /// Record a locally sent segment.
    pub fn send(&mut self, len: usize) -> usize {
        let sent = len.min(usize::try_from(self.snd_wnd).unwrap_or(usize::MAX));
        self.snd_nxt = self.snd_nxt.wrapping_add(u32::try_from(sent).unwrap_or(0));
        self.snd_max = self.snd_max.max(self.snd_nxt);
        sent
    }

    /// Apply a cumulative ACK and a window update.
    ///
    /// Returns the newly acknowledged byte count.
    pub fn ack(&mut self, seq: u32, ack: u32, window: u16) -> usize {
        let mut acked = 0;
        if ack > self.snd_una && ack <= self.snd_max {
            acked = usize::try_from(ack - self.snd_una).unwrap_or(usize::MAX);
            self.snd_una = ack;
        }
        self.update_window(seq, ack, window);
        acked
    }

    /// RFC 9293 / FreeBSD `snd_wl1`/`snd_wl2` window-update ordering.
    pub fn update_window(&mut self, seq: u32, ack: u32, window: u16) -> bool {
        let scaled = u32::from(window) << self.rcv_scale;
        let newer = seq > self.snd_wl1
            || (seq == self.snd_wl1
                && (ack > self.snd_wl2 || (ack == self.snd_wl2 && scaled > self.snd_wnd)));
        if !newer {
            return false;
        }
        self.snd_wnd = scaled;
        self.max_snd_wnd = self.max_snd_wnd.max(window);
        self.snd_wl1 = seq;
        self.snd_wl2 = ack;
        true
    }

    /// Whether a segment is a duplicate ACK.
    #[must_use]
    pub fn is_duplicate_ack(&self, ack: u32, payload_len: usize) -> bool {
        ack == self.snd_una && payload_len == 0
    }

    /// A persist probe is due when the window is closed and unsent data is queued.
    #[must_use]
    pub fn probe_due(&self, queued_bytes: usize) -> bool {
        self.snd_wnd == 0 && queued_bytes > 0
    }

    /// Send one forced persist byte (advances `snd_max`, not `snd_nxt`).
    pub fn probe(&mut self, queued_bytes: usize) -> bool {
        if !self.probe_due(queued_bytes) {
            return false;
        }
        self.probes += 1;
        if self.backoff_shift < 12 {
            self.backoff_shift += 1;
        }
        self.snd_max = self.snd_max.wrapping_add(1);
        true
    }

    /// FreeBSD `tcp_output` SWS/Nagle decision (product `freebsd_output_plan`).
    #[must_use]
    pub fn should_send(
        &self,
        len: i64,
        off: i64,
        sbavail: i64,
        nodelay: bool,
        force: bool,
    ) -> bool {
        let idle = off == 0;
        len > 0
            && (len >= i64::from(self.mss)
                || ((idle || nodelay) && len + off >= sbavail)
                || force
                || (self.max_snd_wnd > 0 && len >= i64::from(self.max_snd_wnd / 2)))
    }
}
