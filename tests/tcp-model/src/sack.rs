// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SACK byte-set reference.
//!
//! `SackedSet` is a naive byte-set reference for RFC 6675 block and ACK arithmetic. The current
//! `raven-tcpip::SackScoreboard` is compared against it; no donor scoreboard code is copied.

use std::collections::BTreeSet;

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

    /// Structural invariants: sorted, disjoint runs and gaps inside `high_data`.
    #[must_use]
    pub fn invariants_hold(&self) -> bool {
        self.bytes.iter().all(|byte| *byte < self.high_data)
            && self.runs().windows(2).all(|pair| pair[0].1 <= pair[1].0)
            && self.gaps().windows(2).all(|pair| pair[0].1 <= pair[1].0)
    }
}
