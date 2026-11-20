// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SACK scoreboard models.
//!
//! `SackedSet` is a naive byte-set reference for RFC 6675 block/ACK arithmetic. `NetBsdScoreboard`
//! is the model-only hole-list port of NetBSD `sys/netinet/tcp_sack.c,v 1.36` (`tcp_sack_option`,
//! `tcp_del_sackholes`, `tcp_sack_output`). The current `raven-tcpip::SackScoreboard` is compared
//! against the byte-set reference in the tests.

use std::collections::BTreeSet;

use crate::seq::{seq_ge, seq_gt, seq_le, seq_lt, seq_max, seq_min};

/// A naive SACK reference over individual byte offsets.
#[derive(Clone, Debug, Default)]
pub struct SackedSet {
    /// Highest valid offset.
    pub high_data: u32,
    /// SACKed byte offsets.
    pub bytes: BTreeSet<u32>,
}

impl SackedSet {
    /// Create an empty reference for `0..high_data`.
    #[must_use]
    pub fn new(high_data: u32) -> Self {
        Self {
            high_data,
            bytes: BTreeSet::new(),
        }
    }

    /// Insert and clip one SACK block.
    pub fn insert(&mut self, start: i64, end: i64) {
        let start = u32::try_from(start.max(0)).unwrap_or(0).min(self.high_data);
        let end = u32::try_from(end.max(0))
            .unwrap_or(self.high_data)
            .min(self.high_data);
        if start >= end {
            return;
        }
        for byte in start..end {
            self.bytes.insert(byte);
        }
    }

    /// Drop SACKed bytes below a cumulative ACK.
    pub fn ack(&mut self, ack: u32) {
        self.bytes.retain(|byte| *byte >= ack);
    }

    /// Merged runs of SACKed bytes.
    #[must_use]
    pub fn runs(&self) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        let mut start = None;
        let mut previous = 0u32;
        for byte in &self.bytes {
            match start {
                None => {
                    start = Some(*byte);
                    previous = *byte;
                }
                Some(_) if *byte == previous.saturating_add(1) => previous = *byte,
                Some(first) => {
                    runs.push((first, previous.saturating_add(1)));
                    start = Some(*byte);
                    previous = *byte;
                }
            }
        }
        if let Some(first) = start {
            runs.push((first, previous.saturating_add(1)));
        }
        runs
    }

    /// Gaps over `0..high_data`.
    #[must_use]
    pub fn gaps(&self) -> Vec<(u32, u32)> {
        let mut gaps = Vec::new();
        let mut cursor = 0u32;
        for (start, end) in self.runs() {
            if start > cursor {
                gaps.push((cursor, start));
            }
            cursor = cursor.max(end);
        }
        if cursor < self.high_data {
            gaps.push((cursor, self.high_data));
        }
        gaps
    }

    /// SACKed bytes strictly above `point`.
    #[must_use]
    pub fn sacked_bytes_above(&self, point: u32) -> u32 {
        u32::try_from(self.bytes.iter().filter(|byte| **byte > point).count()).unwrap_or(u32::MAX)
    }

    /// Number of SACKed ranges whose start is above `point`.
    #[must_use]
    pub fn ranges_above(&self, point: u32) -> u32 {
        u32::try_from(
            self.runs()
                .iter()
                .filter(|(start, _)| *start > point)
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// RFC 6675 `IsLost(S)`.
    #[must_use]
    pub fn is_lost(&self, point: u32, mss: u32, dup_threshold: u32) -> bool {
        self.ranges_above(point) >= dup_threshold
            || self.sacked_bytes_above(point) > dup_threshold.saturating_sub(1).saturating_mul(mss)
    }

    /// RFC 6675 `SetPipe`.
    #[must_use]
    pub fn pipe(&self, high_rxt: u32, mss: u32, dup_threshold: u32) -> u32 {
        let mut pipe = 0u32;
        for (start, end) in self.gaps() {
            if !self.is_lost(start, mss, dup_threshold) {
                pipe = pipe.saturating_add(end - start);
            }
            if start < high_rxt {
                pipe = pipe.saturating_add(end.min(high_rxt) - start);
            }
        }
        pipe
    }

    /// RFC 6675 lost gaps.
    #[must_use]
    pub fn lost_ranges(&self, mss: u32, dup_threshold: u32) -> Vec<(u32, u32)> {
        self.gaps()
            .into_iter()
            .filter(|(start, _)| self.is_lost(*start, mss, dup_threshold))
            .collect()
    }
}

/// One NetBSD SACK hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hole {
    /// First byte of the hole.
    pub start: u32,
    /// One past the last byte.
    pub end: u32,
    /// Retransmit cursor.
    pub rxmit: u32,
}

/// The NetBSD `snd_holes` scoreboard.
#[derive(Clone, Debug)]
pub struct NetBsdScoreboard {
    /// Oldest unacknowledged sequence.
    pub snd_una: u32,
    /// Highest sent sequence.
    pub snd_max: u32,
    /// Highest SACKed sequence.
    pub snd_fack: u32,
    /// Right edge of the last received SACK block.
    pub rcv_lastsack: u32,
    /// Sorted holes.
    pub holes: Vec<Hole>,
}

impl NetBsdScoreboard {
    /// Create an empty scoreboard.
    #[must_use]
    pub fn new(snd_una: u32, snd_max: u32) -> Self {
        Self {
            snd_una,
            snd_max,
            snd_fack: snd_una,
            rcv_lastsack: snd_una,
            holes: Vec::new(),
        }
    }

    /// Apply one SACK block with cumulative ACK `ack`.
    pub fn sack_block(&mut self, left: u32, right: u32, ack: u32) {
        let acked = seq_max(ack, self.snd_una);
        if seq_le(right, acked) || seq_gt(right, self.snd_max) || seq_ge(left, right) {
            return;
        }
        if seq_ge(right, self.snd_fack) {
            self.snd_fack = right;
        }
        if self.holes.is_empty() {
            if seq_lt(acked, left) {
                self.holes.push(Hole {
                    start: acked,
                    end: left,
                    rxmit: acked,
                });
            }
            self.rcv_lastsack = right;
            return;
        }
        let mut index = 0usize;
        while index < self.holes.len() {
            let mut hole = self.holes[index];
            if seq_le(right, hole.start) {
                break;
            }
            if seq_ge(left, hole.end) {
                index += 1;
                continue;
            }
            if seq_le(left, hole.start) {
                if seq_ge(right, hole.end) {
                    self.holes.remove(index);
                    break;
                }
                hole.start = right;
                hole.rxmit = seq_max(hole.rxmit, hole.start);
                self.holes[index] = hole;
                break;
            }
            if seq_ge(right, hole.end) {
                hole.end = left;
                hole.rxmit = seq_min(hole.rxmit, hole.end);
                self.holes[index] = hole;
                break;
            }
            if seq_lt(hole.start, left) && seq_gt(hole.end, right) {
                let split = Hole {
                    start: right,
                    end: hole.end,
                    rxmit: seq_max(hole.rxmit, right),
                };
                hole.end = left;
                hole.rxmit = seq_min(hole.rxmit, hole.end);
                self.holes[index] = hole;
                self.holes.insert(index + 1, split);
                break;
            }
            index += 1;
        }
        if seq_lt(self.rcv_lastsack, left) {
            self.holes.push(Hole {
                start: self.rcv_lastsack,
                end: left,
                rxmit: self.rcv_lastsack,
            });
        }
        if seq_lt(self.rcv_lastsack, right) {
            self.rcv_lastsack = right;
        }
    }

    /// Remove holes covered by a cumulative ACK.
    pub fn del_sackholes(&mut self, ack: u32) {
        let last = seq_max(ack, self.snd_una);
        let index = 0usize;
        while index < self.holes.len() {
            let hole = self.holes[index];
            if seq_le(hole.end, last) {
                self.holes.remove(index);
            } else if seq_lt(hole.start, last) {
                self.holes[index].start = last;
                if seq_lt(self.holes[index].rxmit, last) {
                    self.holes[index].rxmit = last;
                }
                break;
            } else {
                break;
            }
        }
        self.snd_una = last;
    }

    /// First hole with retransmit work, if any.
    #[must_use]
    pub fn sack_output(&self) -> Option<Hole> {
        self.holes
            .iter()
            .copied()
            .find(|hole| seq_lt(hole.rxmit, hole.end) && seq_ge(hole.rxmit, self.snd_una))
    }

    /// Check the scoreboard's structural invariants.
    #[must_use]
    pub fn invariants_hold(&self) -> bool {
        self.holes.iter().all(|hole| {
            seq_lt(hole.start, hole.end)
                && seq_ge(hole.rxmit, hole.start)
                && seq_le(hole.rxmit, hole.end)
        }) && self
            .holes
            .windows(2)
            .all(|pair| seq_le(pair[0].end, pair[1].start))
    }
}
