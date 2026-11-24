// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent host-only reference model for RACK loss detection, reordering
//! window, SACK/DSACK handling, recovery entry/exit and retransmit selection.
//!
//! This is an oracle for the future RACK port, not product code. It is written
//! from the FreeBSD `rack.c` behaviour and RFC 8985/6675; no donor source is
//! linked or copied. Every generated scenario is deterministic from its seed,
//! so a future implementation can be driven with the same ACK/SACK/DSACK
//! events and compared against the recorded lost ranges, pipe and retransmits.

#![forbid(unsafe_code)]

/// Sender MSS used by the model, in bytes.
pub const MSS: u32 = 1000;
/// Number of segments initially in flight.
pub const SEGMENTS: usize = 8;
/// Sequence number of the first byte.
pub const FIRST_SEQ: u32 = 1000;
/// Number of randomized traces in the golden set.
pub const TRACE_COUNT: u64 = 10_000;

/// Parameters that shape one deterministic scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Initial smoothed RTT in microseconds.
    pub srtt: u32,
    /// Extra packet-delay term in the reordering window.
    pub pkt_delay: u32,
    /// Right shift used when stale reordering is seen.
    pub reorder_shift: u32,
    /// Ceiling for the reordering window.
    pub rto_max: u32,
    /// Duplicate-ACK count that is treated as a recovery trigger.
    pub dupack_threshold: u32,
    /// Whether DSACK may grow the reordering window.
    pub use_dsack: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            srtt: 100,
            pkt_delay: 20,
            reorder_shift: 2,
            rto_max: 1_000_000,
            dupack_threshold: 3,
            use_dsack: true,
        }
    }
}

/// One incoming ACK/SACK/DSACK event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Receive time in microseconds.
    pub at: u64,
    /// Cumulative ACK.
    pub ack: u32,
    /// SACK blocks as `(start, end)`.
    pub sacks: Vec<(u32, u32)>,
    /// The peer explicitly reported a duplicate SACK.
    pub dsack: bool,
    /// Optional RTT sample in microseconds.
    pub rtt: Option<u32>,
}

/// One deterministic input trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// Stable seed.
    pub seed: u64,
    /// Scenario parameters.
    pub config: Config,
    /// ACK/SACK/DSACK events.
    pub events: Vec<Event>,
}

#[derive(Clone, Debug)]
struct Segment {
    start: u32,
    end: u32,
    sent_at: u64,
    sacked_at: Option<u64>,
    acked: bool,
    lost: bool,
    retransmitted: bool,
}

impl Segment {
    fn len(&self) -> u32 {
        self.end - self.start
    }
}

/// Model output that a future Rack implementation must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Final lost ranges, sorted and merged.
    pub lost: Vec<(u32, u32)>,
    /// Final RFC 6675-style pipe estimate.
    pub pipe: u32,
    /// Retransmit selections in event order.
    pub retransmits: Vec<(u32, u32)>,
    /// Number of recovery entries.
    pub recovery_entries: u32,
    /// Number of recovery exits.
    pub recovery_exits: u32,
}

/// Small deterministic LCG for the trace generator.
#[derive(Clone, Debug)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn range(&mut self, low: u64, high: u64) -> u64 {
        debug_assert!(low <= high);
        low + self.next() % (high - low + 1)
    }

    fn bool(&mut self, one_in: u64) -> bool {
        self.range(1, one_in) == one_in
    }
}

impl Scenario {
    /// Build a deterministic scenario from a seed.
    #[must_use]
    pub fn generate(seed: u64) -> Self {
        let mut rng = Lcg::new(seed);
        let config = Config {
            srtt: rng.range(50, 250) as u32,
            pkt_delay: rng.range(5, 30) as u32,
            reorder_shift: rng.range(1, 4) as u32,
            rto_max: 1_000_000,
            dupack_threshold: 3,
            use_dsack: rng.bool(4),
        };
        let events = rng.range(7, 14) as usize;
        let mut trace = Vec::with_capacity(events);
        let mut at = 0u64;
        for index in 0..events {
            at += rng.range(20, 700);
            let max_ack = FIRST_SEQ + 3 * MSS;
            let steps = rng.range(0, 3);
            let ack = (FIRST_SEQ + steps as u32 * MSS).min(max_ack);
            let mut sacks = Vec::new();
            let sack_count = rng.range(0, 3);
            for _ in 0..sack_count {
                let seg = rng.range(0, (SEGMENTS - 1) as u64) as u32;
                let start = FIRST_SEQ + seg * MSS;
                if start >= ack {
                    sacks.push((start, start + MSS));
                }
            }
            sacks.sort_unstable();
            sacks.dedup();
            // Every seventh event carries a duplicate SACK below the cumulative ACK.
            let dsack = index % 7 == 6;
            if dsack && ack >= FIRST_SEQ + 2 * MSS {
                sacks.push((ack - 2 * MSS, ack - MSS));
            }
            let rtt = if rng.bool(3) {
                Some(rng.range(40, 300) as u32)
            } else {
                None
            };
            trace.push(Event {
                at,
                ack,
                sacks,
                dsack,
                rtt,
            });
        }
        // Every fifth scenario ends with a full ACK so recovery exit is covered.
        if seed.is_multiple_of(5)
            && let Some(last) = trace.last_mut()
        {
            last.ack = FIRST_SEQ + SEGMENTS as u32 * MSS;
        }
        Self {
            seed,
            config,
            events: trace,
        }
    }
}

/// RACK reordering window in microseconds.
///
/// Base `srtt + pkt_delay`; while reordering is recent add `srtt >> shift`
/// (or `srtt / 4` when the shift is zero); with DSACK enabled each duplicate
/// SACK adds `srtt / 4`. The result is capped at `2 * srtt` and at `rto_max`.
#[must_use]
pub fn reo_wnd(config: Config, srtt: u32, reordering_seen: bool, dsack_count: u32) -> u32 {
    let srtt = srtt.max(1);
    let mut window = srtt.saturating_add(config.pkt_delay);
    if reordering_seen {
        let shift = if config.reorder_shift == 0 {
            2
        } else {
            config.reorder_shift
        };
        window = window.saturating_add(srtt >> shift);
        if config.use_dsack && dsack_count > 0 {
            window = window.saturating_add(dsack_count.saturating_mul(srtt >> 2));
        }
    }
    window.min(srtt.saturating_mul(2)).min(config.rto_max)
}

fn merge_ranges(ranges: &mut [(u32, u32)]) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for &(start, end) in ranges.iter() {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

/// Run one scenario through the reference model.
#[must_use]
pub fn run(scenario: &Scenario) -> Outcome {
    let config = scenario.config;
    let mut segments: Vec<Segment> = (0..SEGMENTS)
        .map(|index| {
            let start = FIRST_SEQ + index as u32 * MSS;
            Segment {
                start,
                end: start + MSS,
                sent_at: index as u64 * 10,
                sacked_at: None,
                acked: false,
                lost: false,
                retransmitted: false,
            }
        })
        .collect();
    let mut snd_una = FIRST_SEQ;
    let snd_nxt = FIRST_SEQ + SEGMENTS as u32 * MSS;
    let mut srtt = config.srtt.max(1);
    let mut reordering_seen = false;
    let mut dsack_count = 0u32;
    let mut in_recovery = false;
    let mut recover = 0u32;
    let mut retransmits = Vec::new();
    let mut outcome = Outcome::default();

    for event in &scenario.events {
        if let Some(rtt) = event.rtt {
            srtt = ((srtt as u64 * 7 + rtt as u64) / 8).max(1) as u32;
        }
        for &(start, end) in &event.sacks {
            if end <= event.ack {
                dsack_count = dsack_count.saturating_add(1);
                reordering_seen = true;
            } else if start < snd_nxt && end > snd_una {
                for segment in &mut segments {
                    if segment.start >= start && segment.end <= end && !segment.acked {
                        segment.sacked_at = Some(event.at);
                        segment.lost = false;
                    }
                }
            }
        }
        if event.dsack {
            dsack_count = dsack_count.saturating_add(1);
            reordering_seen = true;
        }

        if event.ack > snd_una {
            snd_una = event.ack.min(snd_nxt);
            for segment in &mut segments {
                if segment.end <= snd_una {
                    segment.acked = true;
                    segment.lost = false;
                }
            }
        }

        // RACK loss: an un-SACKed segment sent before a later SACKed segment,
        // and now older than the reordering window, is declared lost.
        let window = reo_wnd(config, srtt, reordering_seen, dsack_count);
        let mut newly_lost = Vec::new();
        for index in 0..segments.len() {
            let segment = &segments[index];
            if segment.acked || segment.sacked_at.is_some() || segment.lost {
                continue;
            }
            if event.at < segment.sent_at.saturating_add(u64::from(window)) {
                continue;
            }
            let (start, end, sent_at) = (segment.start, segment.end, segment.sent_at);
            let later_sacked = segments.iter().any(|other| {
                other.sacked_at.is_some() && other.start > start && other.sent_at >= sent_at
            });
            if later_sacked {
                segments[index].lost = true;
                newly_lost.push((start, end));
            }
        }

        if !in_recovery && (!newly_lost.is_empty() || dsack_count >= config.dupack_threshold) {
            in_recovery = true;
            recover = snd_nxt;
            outcome.recovery_entries += 1;
        }

        if let Some(index) = segments.iter().position(|segment| {
            segment.lost && !segment.retransmitted && !segment.acked && segment.sacked_at.is_none()
        }) {
            segments[index].retransmitted = true;
            segments[index].sent_at = event.at;
            retransmits.push((segments[index].start, segments[index].end));
        }

        if in_recovery && snd_una >= recover {
            in_recovery = false;
            outcome.recovery_exits += 1;
        }
    }

    let mut lost = Vec::new();
    let mut sacked_bytes = 0u32;
    let mut lost_bytes = 0u32;
    let mut retransmitted_pending = 0u32;
    for segment in &segments {
        if segment.acked {
            continue;
        }
        if segment.sacked_at.is_some() {
            sacked_bytes = sacked_bytes.saturating_add(segment.len());
        }
        if segment.lost {
            lost_bytes = lost_bytes.saturating_add(segment.len());
            lost.push((segment.start, segment.end));
            if segment.retransmitted {
                retransmitted_pending = retransmitted_pending.saturating_add(segment.len());
            }
        }
    }
    let outstanding = snd_nxt.saturating_sub(snd_una);
    outcome.pipe = outstanding
        .saturating_sub(sacked_bytes)
        .saturating_sub(lost_bytes)
        .saturating_add(retransmitted_pending);
    outcome.lost = merge_ranges(&mut lost);
    outcome.retransmits = retransmits;
    outcome
}

/// Canonical text for one event list, used by the trace digest.
#[must_use]
pub fn canonical_events(scenario: &Scenario) -> String {
    let mut text = String::new();
    for event in &scenario.events {
        text.push_str(&format!("{}:{}:", event.at, event.ack));
        for (index, (start, end)) in event.sacks.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(&format!("{start}-{end}"));
        }
        text.push(':');
        text.push(if event.dsack { '1' } else { '0' });
        text.push(':');
        match event.rtt {
            Some(rtt) => text.push_str(&rtt.to_string()),
            None => text.push('-'),
        }
        text.push(';');
    }
    text
}

/// FNV-1a 64-bit digest of the canonical event list.
#[must_use]
pub fn event_digest(scenario: &Scenario) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in canonical_events(scenario).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn format_ranges(ranges: &[(u32, u32)]) -> String {
    if ranges.is_empty() {
        return "-".to_owned();
    }
    ranges
        .iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// One trace-file row: seed, event count, lost, pipe, retransmits, recovery and digest.
#[must_use]
pub fn trace_row(scenario: &Scenario, outcome: &Outcome) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}/{}\t{:016x}",
        scenario.seed,
        scenario.events.len(),
        format_ranges(&outcome.lost),
        outcome.pipe,
        format_ranges(&outcome.retransmits),
        outcome.recovery_entries,
        outcome.recovery_exits,
        event_digest(scenario),
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
