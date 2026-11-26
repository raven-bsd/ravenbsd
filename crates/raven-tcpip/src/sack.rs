// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from DragonFlyBSD sys/netinet/tcp_sack.c @
// 94448f622a06cde90ba42d688e9cab2448ad9ab3 (3-clause BSD; Jeffrey M. Hsu and
// The DragonFly Project). Rust reimplementation; no C expression copied.

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

/// A saved scoreboard for reneging rollback (`tcp_sack_save_scoreboard`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SackSnapshot {
    sacked: Vec<(u32, u32)>,
    lostseq: u32,
    reneging: bool,
}

/// DragonFly `tcp_sack_ndsack_blocks`: classify the leading DSACK block(s).
#[must_use]
pub fn ndsack_blocks(blocks: &[(i64, i64)], snd_una: i64) -> u8 {
    if blocks.is_empty() {
        return 0;
    }
    if blocks[0].0 < snd_una {
        return 1;
    }
    if blocks.len() > 1 && blocks[0].0 >= blocks[1].0 && blocks[0].1 <= blocks[1].1 {
        return 2;
    }
    0
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
    /// First sequence number considered lost (DragonFly `scb.lostseq`), relative to `snd_una`.
    pub lostseq: u32,
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
            lostseq: 0,
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

    /// Whether the last scoreboard update detected reneging.
    #[must_use]
    pub fn is_reneging(&self) -> bool {
        self.reneging
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
        self.update_lostseq(0);
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

    /// Compute the DragonFly `lostseq` from the SACK blocks.
    fn computed_lostseq(&self) -> u32 {
        let threshold = self.dup_threshold.max(1);
        let bytes_threshold = threshold.saturating_sub(1).saturating_mul(self.mss);
        let mut blocks = 0u32;
        let mut bytes = 0u32;
        for (start, end) in self.sacked.iter().rev() {
            blocks = blocks.saturating_add(1);
            bytes = bytes.saturating_add(end.saturating_sub(*start));
            if blocks == threshold || bytes >= bytes_threshold {
                return *start;
            }
        }
        0
    }

    /// DragonFly `tcp_sack_update_lostseq`: advance the first-lost mark from the SACK blocks.
    pub fn update_lostseq(&mut self, snd_una: u32) {
        let lostseq = self.computed_lostseq();
        self.lostseq = if lostseq == 0 { snd_una } else { lostseq };
    }

    /// RFC 6675 `IsLost(S)` / DragonFly `tcp_sack_islost`.
    #[must_use]
    pub fn is_lost(&self, point: u32) -> bool {
        self.ranges_above(point) >= self.dup_threshold
            || self.sacked_bytes_above(point)
                > self
                    .dup_threshold
                    .saturating_sub(1)
                    .saturating_mul(self.mss)
    }

    /// DragonFly `tcp_sack_has_sacked`.
    #[must_use]
    pub fn has_sacked(&self, amount: u32) -> bool {
        let mut bytes = 0u32;
        for (start, end) in &self.sacked {
            bytes = bytes.saturating_add(end.saturating_sub(*start));
            if bytes >= amount {
                return true;
            }
        }
        false
    }

    /// DragonFly `tcp_sack_bytes_below`.
    #[must_use]
    pub fn bytes_below(&self, sequence: u32) -> u32 {
        let mut bytes = 0u32;
        for (start, end) in &self.sacked {
            if sequence > *start {
                bytes = bytes.saturating_add(sequence.min(*end).saturating_sub(*start));
            }
        }
        bytes
    }

    /// DragonFly `tcp_sack_ack_blocks`: drop blocks the cumulative ACK covers and detect
    /// reneging when the ACK lands inside a still-reported block.
    pub fn ack_blocks(&mut self, ack: u32) {
        while let Some((_, end)) = self.sacked.first().copied() {
            if end <= ack {
                self.sacked.remove(0);
            } else {
                break;
            }
        }
        if let Some((start, _)) = self.sacked.first().copied()
            && ack >= start
        {
            self.note_reneging();
        }
        self.update_lostseq(0);
    }

    /// DragonFly `tcp_sack_add_blocks`: validate and insert incoming SACK blocks.
    ///
    /// The first block is skipped when it is a DSACK (below `snd_una` or enclosed by the next
    /// block); blocks above `snd_max` stop the update.
    pub fn add_blocks(&mut self, ack: u32, snd_max: u32, blocks: &[(i64, i64)]) {
        let start = if ndsack_blocks(blocks, 0) > 0 { 1 } else { 0 };
        for (left, right) in blocks.iter().skip(start) {
            if *left <= i64::from(ack) {
                continue;
            }
            if *right > i64::from(snd_max) {
                break;
            }
            self.insert_block(*left, *right);
        }
    }

    /// DragonFly `tcp_sack_skip_sacked`: advance past a SACKed block containing `seq`.
    #[must_use]
    pub fn skip_sacked(&self, seq: u32) -> u32 {
        for (start, end) in &self.sacked {
            if seq >= *start && seq < *end {
                return *end;
            }
        }
        seq
    }

    /// DragonFly `tcp_sack_first_unsacked_len`.
    #[must_use]
    pub fn first_unsacked_len(&self) -> u32 {
        self.sacked.first().map_or(self.mss, |(start, _)| *start)
    }

    /// DragonFly `tcp_sack_compute_pipe` (the RFC 6675 `pipe()` above keeps the vector contract).
    #[must_use]
    pub fn compute_pipe(&self) -> u32 {
        let lostseq = self.computed_lostseq();
        let mut nlost = self.high_data.saturating_sub(lostseq);
        let mut nretransmitted = self.high_rxt.saturating_sub(self.high_ack);
        for (start, end) in &self.sacked {
            if *start < self.high_rxt {
                nretransmitted =
                    nretransmitted.saturating_sub((*end).min(self.high_rxt).saturating_sub(*start));
            }
            if *start >= lostseq {
                nlost = nlost.saturating_sub(end.saturating_sub(*start));
            }
        }
        nlost.saturating_add(nretransmitted)
    }

    /// Save the scoreboard for DragonFly `tcp_sack_save_scoreboard`.
    #[must_use]
    pub fn snapshot(&self) -> SackSnapshot {
        SackSnapshot {
            sacked: self.sacked.clone(),
            lostseq: self.lostseq,
            reneging: self.reneging,
        }
    }

    /// Restore a saved scoreboard and re-apply the cumulative ACK (`tcp_sack_revert_scoreboard`).
    pub fn revert(&mut self, snapshot: SackSnapshot, snd_una: u32) {
        self.sacked = snapshot.sacked;
        self.lostseq = snapshot.lostseq;
        self.reneging = snapshot.reneging;
        self.ack_blocks(snd_una);
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

    /// RFC 6675 `SetPipe` / DragonFly `tcp_sack_compute_pipe`.
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
            let length = end.saturating_sub(start).min(self.mss);
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

/// Receiver SACK report state (`tcp_sack_fill_report`, `tcp_sack_ack_history`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SackReport {
    /// Reported out-of-order blocks, newest first.
    pub history: Vec<(u32, u32)>,
    /// The block for this ACK, when one is owed.
    pub report: Option<(u32, u32)>,
    /// The enclosing block for a duplicate SACK.
    pub encloses: Option<(u32, u32)>,
    /// The report carries a DSACK.
    pub dsack: bool,
    /// Per-connection history cap.
    pub max_history: usize,
}

// Full DragonFly receiver-report surface; the live receiver call site is outside this ticket's
// Owns (`tcp_state.rs`), and every method is covered by the unit tests.
#[cfg_attr(not(test), allow(dead_code))]
impl SackReport {
    /// Create a report with a history cap.
    #[must_use]
    pub fn new(max_history: usize) -> Self {
        Self {
            history: Vec::new(),
            report: None,
            encloses: None,
            dsack: false,
            max_history: max_history.max(1),
        }
    }

    /// Record the block that triggered this ACK.
    pub fn note_block(&mut self, start: u32, end: u32, dsack: bool) {
        self.report = Some((start, end));
        self.dsack = dsack;
    }

    /// Whether a SACK option must be emitted.
    #[must_use]
    pub fn report_needed(&self) -> bool {
        self.report.is_some() || self.dsack || self.encloses.is_some()
    }

    /// Clear the per-ACK report; keep the history.
    pub fn cleanup(&mut self) {
        self.report = None;
        self.encloses = None;
        self.dsack = false;
    }

    /// `tcp_sack_ack_history`: drop or trim history below `rcv_nxt`.
    pub fn ack_history(&mut self, rcv_nxt: u32) {
        let mut kept = Vec::with_capacity(self.history.len());
        for (start, end) in self.history.drain(..) {
            if end <= rcv_nxt {
                continue;
            }
            kept.push((start.max(rcv_nxt), end));
        }
        self.history = kept;
    }

    /// `tcp_sack_update_reported_history`: merge a new block into the reported history.
    pub fn update_reported_history(&mut self, start: u32, end: u32) {
        let mut kept = Vec::with_capacity(self.history.len());
        let mut start = start;
        let mut end = end;
        for (old_start, old_end) in self.history.drain(..) {
            if end < old_start || start > old_end {
                kept.push((old_start, old_end));
                continue;
            }
            start = start.min(old_start);
            end = end.max(old_end);
        }
        kept.insert(0, (start, end));
        kept.truncate(self.max_history);
        self.history = kept;
    }

    /// `tcp_sack_fill_report`: build the ordered blocks for the option payload.
    pub fn fill_report(&mut self, rcv_nxt: u32, max_blocks: usize) -> Vec<(u32, u32)> {
        self.ack_history(rcv_nxt);
        let mut blocks = Vec::new();
        if let Some(block) = self.report {
            blocks.push(block);
        }
        if let Some(block) = self.encloses {
            blocks.push(block);
        }
        for block in &self.history {
            if blocks.len() >= max_blocks {
                break;
            }
            if blocks.first() != Some(block) {
                blocks.push(*block);
            }
        }
        blocks.truncate(max_blocks);
        self.cleanup();
        blocks
    }
}

/// Parse a SACK option (`kind=5`) into its blocks.
// Full DragonFly option surface; the live option parser is in `tcp_state.rs` (outside Owns).
#[cfg_attr(not(test), allow(dead_code))]
#[must_use]
pub fn parse_sack_option(bytes: &[u8]) -> Option<Vec<(u32, u32)>> {
    if bytes.first().copied()? != 5 {
        return None;
    }
    let length = usize::from(*bytes.get(1)?);
    if length < 10 || length % 8 != 2 || length > bytes.len() {
        return None;
    }
    let count = (length - 2) / 8;
    let mut blocks = Vec::with_capacity(count);
    for index in 0..count {
        let base = 2usize.checked_add(index.checked_mul(8)?)?;
        let start = u32::from_be_bytes(bytes.get(base..base + 4)?.try_into().ok()?);
        let end = u32::from_be_bytes(bytes.get(base + 4..base + 8)?.try_into().ok()?);
        blocks.push((start, end));
    }
    Some(blocks)
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
