// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SACK scoreboard (RFC 6675), receiver block selection (RFC 2018) and DSACK.

use alloc::vec::Vec;

/// The candidate selected by `NextSeg`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextKind {
    /// Retransmit an existing range.
    Retransmit,
    /// Send new data at `HighData`.
    New,
    /// Send a rescue segment at the highest outstanding octet.
    Rescue,
    /// Nothing to send.
    None,
}

/// A `NextSeg` result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NextSeg {
    /// Start offset relative to `snd_una`, or -1 for none.
    pub start: i64,
    /// Length in bytes.
    pub len: u32,
    /// Candidate kind.
    pub kind: NextKind,
    /// RFC 6675 rule number.
    pub rule: u8,
}

impl NextSeg {
    /// The encoded none result.
    pub const NONE: Self = Self {
        start: -1,
        len: 0,
        kind: NextKind::None,
        rule: 5,
    };
}

/// One scoreboard computation result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SackResult {
    /// Ranges after clamping, merging and capping.
    pub sacked: Vec<(u32, u32)>,
    /// Whether a DSACK block was detected.
    pub dsack_detected: bool,
    /// Whether reneging was detected.
    pub reneging: bool,
    /// Ranges dropped by the cap or outside the window.
    pub dropped: usize,
    /// RFC 6675 Pipe estimate.
    pub pipe: u32,
    /// Lost gaps.
    pub lost: Vec<(u32, u32)>,
    /// Next segment choice.
    pub next_seg: NextSeg,
    /// `HighRxt` after the choice.
    pub high_rxt_after: u32,
    /// `RescueRxt` after the choice, or -1.
    pub rescue_rxt_after: i64,
}

/// A bounded SACK scoreboard over byte offsets relative to `snd_una`.
#[derive(Clone, Debug)]
pub struct SackScoreboard {
    pub sacked: Vec<(u32, u32)>,
    pub high_data: u32,
    pub high_rxt: u32,
    pub high_ack: u32,
    pub rescue_rxt: Option<u32>,
    pub mss: u32,
    pub dup_threshold: u32,
    pub range_cap: usize,
    pub dropped: usize,
    dsack_detected: bool,
    reneging: bool,
}

impl SackScoreboard {
    /// Build a scoreboard.
    #[must_use]
    pub fn new(
        high_data: u32,
        high_rxt: u32,
        high_ack: u32,
        mss: u32,
        dup_threshold: u32,
        range_cap: usize,
    ) -> Self {
        Self {
            sacked: Vec::new(),
            high_data,
            high_rxt,
            high_ack,
            rescue_rxt: None,
            mss,
            dup_threshold: dup_threshold.max(1),
            range_cap: range_cap.max(1),
            dropped: 0,
            dsack_detected: false,
            reneging: false,
        }
    }

    /// Record a DSACK block (negative or below `snd_una`).
    pub fn note_dsack(&mut self, _start: i64, _end: i64) {
        self.dsack_detected = true;
    }

    /// Mark reneging and rebuild conservatively.
    pub fn note_reneging(&mut self) {
        self.reneging = true;
        self.sacked.clear();
    }

    /// Insert a SACK block, clipping to `[0, high_data)` and merging overlaps.
    pub fn insert_block(&mut self, start: i64, end: i64) {
        if end <= 0 || start >= i64::from(self.high_data) {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let start = u32::try_from(start.max(0)).unwrap_or(0);
        let end = u32::try_from(end)
            .unwrap_or(self.high_data)
            .min(self.high_data);
        if start >= end {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        if self.sacked.len() >= self.range_cap {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.sacked.push((start, end));
        self.normalize();
    }

    fn normalize(&mut self) {
        self.sacked.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (start, end) in self.sacked.drain(..) {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
                continue;
            }
            merged.push((start, end));
        }
        self.sacked = merged;
    }

    /// SACKed bytes strictly above `point`.
    #[must_use]
    pub fn sacked_bytes_above(&self, point: u32) -> u32 {
        self.sacked
            .iter()
            .map(|(start, end)| end.saturating_sub((*start).max(point)))
            .sum()
    }

    /// Number of SACKed ranges whose start is above `point`.
    #[must_use]
    pub fn ranges_above(&self, point: u32) -> u32 {
        u32::try_from(
            self.sacked
                .iter()
                .filter(|(start, _)| *start > point)
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// RFC 6675 `IsLost(S)`.
    #[must_use]
    pub fn is_lost(&self, point: u32) -> bool {
        self.ranges_above(point) >= self.dup_threshold
            || self.sacked_bytes_above(point)
                > self
                    .dup_threshold
                    .saturating_sub(1)
                    .saturating_mul(self.mss)
    }

    /// The complement of `sacked` over `[0, high_data)`.
    #[must_use]
    pub fn gaps(&self) -> Vec<(u32, u32)> {
        let mut gaps = Vec::new();
        let mut cursor = 0u32;
        for (start, end) in &self.sacked {
            if *start > cursor {
                gaps.push((cursor, *start));
            }
            cursor = cursor.max(*end);
        }
        if cursor < self.high_data {
            gaps.push((cursor, self.high_data));
        }
        gaps
    }

    /// RFC 6675 `SetPipe`.
    #[must_use]
    pub fn pipe(&self) -> u32 {
        let mut pipe = 0u32;
        for (start, end) in self.gaps() {
            if !self.is_lost(start) {
                pipe = pipe.saturating_add(end - start);
            }
            if start < self.high_rxt {
                pipe = pipe.saturating_add(end.min(self.high_rxt) - start);
            }
        }
        pipe
    }

    /// Lost gaps.
    #[must_use]
    pub fn lost_ranges(&self) -> Vec<(u32, u32)> {
        self.gaps()
            .into_iter()
            .filter(|(start, _)| self.is_lost(*start))
            .collect()
    }

    /// RFC 6675 `NextSeg`.
    #[must_use]
    pub fn next_seg(&mut self, unsent_available: bool, receiver_window: u32) -> SackResult {
        let lost = self.lost_ranges();
        let hs = self.sacked.iter().map(|(_, end)| *end).max().unwrap_or(0);
        let mut next = NextSeg::NONE;
        let mut high_rxt_after = self.high_rxt;
        let mut rescue_after = self.rescue_rxt.map(i64::from).unwrap_or(-1);

        // Rule 1: lost range above HighRxt.
        for (start, end) in &lost {
            if *start >= self.high_rxt && *start < hs {
                let length = (*end - *start).min(self.mss);
                next = NextSeg {
                    start: i64::from(*start),
                    len: length,
                    kind: NextKind::Retransmit,
                    rule: 1,
                };
                high_rxt_after = high_rxt_after.max(start.saturating_add(length));
                break;
            }
        }
        // Rule 2: new data.
        if next.kind == NextKind::None && unsent_available {
            let available = receiver_window.saturating_sub(self.high_data);
            let length = available.min(self.mss);
            if length > 0 {
                next = NextSeg {
                    start: i64::from(self.high_data),
                    len: length,
                    kind: NextKind::New,
                    rule: 2,
                };
            }
        }
        // Rule 3: smallest unSACKed above HighRxt even if not lost.
        if next.kind == NextKind::None {
            for (start, end) in self.gaps() {
                if start >= self.high_rxt && start < hs {
                    let length = (end - start).min(self.mss);
                    next = NextSeg {
                        start: i64::from(start),
                        len: length,
                        kind: NextKind::Retransmit,
                        rule: 3,
                    };
                    high_rxt_after = high_rxt_after.max(start.saturating_add(length));
                    break;
                }
            }
        }
        // Rule 4: rescue.
        if next.kind == NextKind::None
            && (self.rescue_rxt.is_none() || i64::from(self.high_ack) > rescue_after)
            && let Some((start, end)) = self.gaps().last().copied()
        {
            let start = start.max(self.high_data.saturating_sub(self.mss));
            let length = (end - start).min(self.mss);
            next = NextSeg {
                start: i64::from(start),
                len: length,
                kind: NextKind::Rescue,
                rule: 4,
            };
            self.rescue_rxt = Some(self.high_data);
            rescue_after = i64::from(self.high_data);
        }

        let lost = self.lost_ranges();
        SackResult {
            sacked: self.sacked.clone(),
            dsack_detected: self.dsack_detected,
            reneging: self.reneging,
            dropped: self.dropped,
            pipe: self.pipe(),
            lost,
            next_seg: next,
            high_rxt_after,
            rescue_rxt_after: rescue_after,
        }
    }
}

/// Receiver-side block selection: most recent holes first, bounded by the option
/// space (`40 - 2 - 12` when timestamps are present, else `40 - 2`).
#[must_use]
pub fn receiver_blocks(
    holes: &[(u32, u32)],
    timestamps: bool,
    dsack: Option<(u32, u32)>,
) -> Vec<(u32, u32)> {
    let max_blocks = if timestamps { 3 } else { 4 };
    let mut blocks: Vec<(u32, u32)> = Vec::new();
    if let Some(block) = dsack {
        blocks.push(block);
    }
    for (start, end) in holes.iter().rev() {
        if blocks.len() >= max_blocks {
            break;
        }
        blocks.push((*start, *end));
    }
    blocks
}

/// Option bytes for SACK blocks, including the kind/length prefix.
#[must_use]
pub fn sack_option_bytes(blocks: &[(u32, u32)]) -> Vec<u8> {
    if blocks.is_empty() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    bytes.push(5);
    bytes.push(u8::try_from(2 + blocks.len().saturating_mul(8)).unwrap_or(40));
    for (start, end) in blocks {
        bytes.extend_from_slice(&start.to_be_bytes());
        bytes.extend_from_slice(&end.to_be_bytes());
    }
    bytes
}
