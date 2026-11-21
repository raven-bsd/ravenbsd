// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK recovery/pipe reference model (K324).
//!
//! This crate extends the merged K320 `rack-tlp-model` `RackModel` with the RACK recovery
//! entry/exit, lost-range/RFC 6675 pipe and retransmit-selection layer. It produces a stable
//! K319-format 10k trace. No product source is edited and no donor expression is copied.

#![forbid(unsafe_code)]

use rack_tlp_model::model::{Config, RackModel};
use rack_tlp_model::rng::Rng;

/// Sender MSS used by the traces.
pub const MSS: u32 = 1000;
/// Segments in the fixed send map.
pub const SEGMENTS: usize = 8;
/// First byte sequence.
pub const FIRST_SEQ: u32 = 1000;
/// Number of golden traces.
pub const TRACE_COUNT: u64 = 10_000;

/// One ACK/SACK/DSACK event in the K319 trace shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Event time, milliseconds.
    pub at: u64,
    /// Cumulative ACK.
    pub ack: u32,
    /// SACK blocks as `(start, end)`.
    pub sacks: Vec<(u32, u32)>,
    /// A duplicate SACK is present.
    pub dsack: bool,
    /// Optional RTT sample, milliseconds.
    pub rtt: Option<u32>,
}

/// One deterministic scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// Stable seed.
    pub seed: u64,
    /// Events in order.
    pub events: Vec<Event>,
}

/// Model output a full RACK implementation must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Final lost ranges, sorted and merged.
    pub lost: Vec<(u32, u32)>,
    /// RFC 6675-style pipe estimate.
    pub pipe: u32,
    /// Retransmit selections in event order.
    pub retransmits: Vec<(u32, u32)>,
    /// Recovery entries.
    pub recovery_entries: u32,
    /// Recovery exits.
    pub recovery_exits: u32,
}

/// Recovery/pipe layer over the shared K320 `RackModel`.
pub struct RecoveryModel {
    /// Shared RACK-TLP/DSACK/reordering core.
    pub inner: RackModel,
    /// Current lost ranges.
    pub lost: Vec<(u32, u32)>,
    /// All retransmit selections.
    pub retransmits: Vec<(u32, u32)>,
    /// Recovery entry count.
    pub recovery_entries: u32,
    /// Recovery exit count.
    pub recovery_exits: u32,
    /// Ranges already selected for retransmission.
    retransmitted: Vec<(u32, u32)>,
}

impl RecoveryModel {
    /// Build the model on the K320 core.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let mut inner = RackModel::new(config, 10, 4);
        inner.snd_una = FIRST_SEQ;
        inner.snd_max = FIRST_SEQ;
        Self {
            inner,
            lost: Vec::new(),
            retransmits: Vec::new(),
            recovery_entries: 0,
            recovery_exits: 0,
            retransmitted: Vec::new(),
        }
    }

    /// Record a segment transmission.
    pub fn send(&mut self, start: u32, end: u32, now_ms: u32) {
        self.inner.on_sent(start, end, now_ms);
    }

    /// Process a cumulative ACK and count recovery exit.
    pub fn on_ack(&mut self, ack: u32, now_ms: u32) {
        let was_recovering = self.inner.recovery.is_some();
        self.inner.on_ack(ack, now_ms);
        if was_recovering && self.inner.recovery.is_none() {
            self.recovery_exits = self.recovery_exits.saturating_add(1);
        }
        self.recompute(now_ms);
    }

    /// Process a SACK range; ranges at or below `snd_una` are ignored.
    pub fn on_sack(&mut self, start: u32, end: u32, now_ms: u32) {
        let start = start.max(self.inner.snd_una);
        if end <= start {
            return;
        }
        self.inner.on_sack(start, end);
        self.recompute(now_ms);
    }

    /// Process a duplicate SACK.
    pub fn on_dsack(&mut self, start: u32, end: u32, now_ms: u32) {
        self.inner.on_dsack(start, end);
        self.recompute(now_ms);
    }

    /// Recompute lost ranges and enter recovery on the first loss.
    pub fn recompute(&mut self, now_ms: u32) {
        let mut lost: Vec<(u32, u32)> = self
            .inner
            .segments
            .iter()
            .filter(|segment| self.inner.is_lost(segment.start, now_ms))
            .map(|segment| (segment.start, segment.end))
            .collect();
        lost.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (start, end) in lost {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
                continue;
            }
            merged.push((start, end));
        }
        self.lost = merged;
        if !self.lost.is_empty() && self.inner.recovery.is_none() {
            let recover = self.inner.snd_max;
            self.inner.enter_recovery(recover);
            self.recovery_entries = self.recovery_entries.saturating_add(1);
        }
    }

    /// Select the lowest lost, not-yet-retransmitted segment.
    pub fn select(&mut self, now_ms: u32) -> Option<(u32, u32)> {
        self.inner.recovery?;
        let mut candidate = None;
        'ranges: for &(start, end) in &self.lost {
            let mut cursor = start;
            while cursor < end {
                let segment = (cursor, (cursor + MSS).min(end));
                if !self.retransmitted.contains(&segment) {
                    candidate = Some(segment);
                    break 'ranges;
                }
                cursor = segment.1;
            }
        }
        let candidate = candidate?;
        self.retransmitted.push(candidate);
        self.retransmits.push(candidate);
        if let Some(segment) = self
            .inner
            .segments
            .iter_mut()
            .find(|segment| segment.start == candidate.0)
        {
            segment.retransmits = segment.retransmits.saturating_add(1);
        }
        self.inner
            .note_retransmit(candidate.1.saturating_sub(candidate.0));
        self.inner.now_ms = now_ms;
        Some(candidate)
    }

    /// RFC 6675-style pipe: `outstanding - sacked - lost + retransmitted`.
    #[must_use]
    pub fn pipe(&self) -> u32 {
        let outstanding = self.inner.snd_max.saturating_sub(self.inner.snd_una);
        let sacked = self.inner.sacked_bytes().min(outstanding);
        let lost: u32 = self
            .lost
            .iter()
            .map(|(start, end)| end.saturating_sub(*start))
            .sum::<u32>()
            .min(outstanding);
        let retransmitted: u32 = self
            .retransmits
            .iter()
            .map(|(start, end)| {
                let low = (*start).max(self.inner.snd_una);
                let high = (*end).min(self.inner.snd_max);
                high.saturating_sub(low)
            })
            .sum();
        outstanding
            .saturating_sub(sacked)
            .saturating_sub(lost)
            .saturating_add(retransmitted)
    }

    /// Final model output.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        Outcome {
            lost: self.lost.clone(),
            pipe: self.pipe(),
            retransmits: self.retransmits.clone(),
            recovery_entries: self.recovery_entries,
            recovery_exits: self.recovery_exits,
        }
    }
}

impl Scenario {
    /// Build a deterministic scenario for `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(1));
        let count = 8 + rng.below(9) as usize;
        let mut at = 100u64;
        let mut events = Vec::with_capacity(count + 1);
        for _ in 0..count {
            at = at.saturating_add(u64::from(rng.range(10, 50)));
            let ack = FIRST_SEQ + rng.below(SEGMENTS as u32 + 1) * MSS;
            let mut sacks = Vec::new();
            for _ in 0..rng.below(3) {
                let start = FIRST_SEQ + rng.below(SEGMENTS as u32) * MSS;
                sacks.push((start, start + MSS));
            }
            events.push(Event {
                at,
                ack,
                sacks,
                dsack: rng.below(5) == 0,
                rtt: (rng.below(3) == 0).then(|| rng.range(50, 200)),
            });
        }
        if seed.is_multiple_of(5) {
            events.push(Event {
                at: at.saturating_add(50),
                ack: FIRST_SEQ + SEGMENTS as u32 * MSS,
                sacks: Vec::new(),
                dsack: false,
                rtt: None,
            });
        }
        Self { seed, events }
    }

    /// Run the recovery/pipe model over the scenario.
    #[must_use]
    pub fn run(&self) -> Outcome {
        let mut model = RecoveryModel::new(Config::default());
        for index in 0..SEGMENTS {
            let start = FIRST_SEQ + index as u32 * MSS;
            model.send(start, start + MSS, index as u32 * 5);
        }
        for event in &self.events {
            let now = u32::try_from(event.at).unwrap_or(u32::MAX);
            model.on_ack(event.ack, now);
            for &(start, end) in &event.sacks {
                model.on_sack(start, end, now);
            }
            if event.dsack
                && let Some(&(start, end)) = event.sacks.first()
            {
                model.on_dsack(start, end, now);
            }
            model.recompute(now);
            let _ = model.select(now);
        }
        model.outcome()
    }
}

/// FNV-1a 64-bit digest of the canonical event list.
#[must_use]
pub fn event_digest(events: &[Event]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for event in events {
        let sacks: Vec<String> = event
            .sacks
            .iter()
            .map(|(start, end)| format!("{start}-{end}"))
            .collect();
        let text = format!(
            "{}:{}:{}:{}:{}|",
            event.at,
            event.ack,
            sacks.join(","),
            u8::from(event.dsack),
            event.rtt.unwrap_or(0)
        );
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn ranges(values: &[(u32, u32)]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|(start, end)| format!("{start}-{end}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The stable trace header.
#[must_use]
pub fn header() -> String {
    "# RACK recovery/pipe reference traces (K324)\n\
     # seed\tevents\tlost\tpipe\tretransmits\trecovery_entries/exits\tevent_digest\n"
        .to_string()
}

/// Format one trace row.
#[must_use]
pub fn format_row(seed: u64, event_count: usize, outcome: &Outcome, digest: u64) -> String {
    format!(
        "{seed}\t{event_count}\t{}\t{}\t{}\t{}/{}\t{digest:016x}",
        ranges(&outcome.lost),
        outcome.pipe,
        ranges(&outcome.retransmits),
        outcome.recovery_entries,
        outcome.recovery_exits
    )
}

/// Generate all golden rows.
#[must_use]
pub fn generate(count: u64) -> Vec<String> {
    (1..=count)
        .map(|seed| {
            let scenario = Scenario::new(seed);
            let outcome = scenario.run();
            let digest = event_digest(&scenario.events);
            format_row(seed, scenario.events.len(), &outcome, digest)
        })
        .collect()
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
