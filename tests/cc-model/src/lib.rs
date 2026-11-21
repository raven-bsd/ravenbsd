// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only independent model for NewReno, CUBIC and RACK congestion control.
//!
//! This crate is an oracle for K269, not product code. It is written from the FreeBSD
//! `cc_newreno.c`, `cc_cubic.c` and `rack.c` behaviour and RFC 9438/8985; no donor expression is
//! copied. Every scenario is deterministic from its seed, so the merged implementation and the
//! K269 branch can be driven with the same ACK/loss/RTT events and the outcomes compared.

#![forbid(unsafe_code)]

/// Sender MSS used by the model.
pub const MSS: u32 = 1_000;
/// First byte sequence.
pub const FIRST_SEQ: u32 = 1_000;
/// Segments initially in flight.
pub const SEGMENTS: usize = 8;
/// Number of golden traces.
pub const TRACE_COUNT: u64 = 10_000;

/// Congestion-control algorithm under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// NewReno ABC.
    NewReno,
    /// CUBIC curve and fast convergence.
    Cubic,
    /// RACK recovery/pipe.
    Rack,
}

/// Scenario parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Algorithm.
    pub kind: Kind,
    /// Initial SRTT in milliseconds.
    pub srtt_ms: u32,
    /// Initial congestion window in bytes.
    pub cwnd: u32,
    /// Initial slow-start threshold in bytes.
    pub ssthresh: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kind: Kind::NewReno,
            srtt_ms: 100,
            cwnd: 10 * MSS,
            ssthresh: 1_000_000,
        }
    }
}

/// One ACK/loss/RTT event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Event time in milliseconds.
    pub at: u64,
    /// Cumulative ACK.
    pub ack: u32,
    /// A loss is reported at this event.
    pub loss: bool,
    /// Optional RTT sample in milliseconds.
    pub rtt: Option<u32>,
}

/// One deterministic trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// Stable seed.
    pub seed: u64,
    /// Parameters.
    pub config: Config,
    /// Events.
    pub events: Vec<Event>,
}

/// Model output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Final congestion window.
    pub cwnd: u32,
    /// Final slow-start threshold.
    pub ssthresh: u32,
    /// Recovery active at the end.
    pub in_recovery: bool,
    /// Lost sequence starts.
    pub lost: Vec<u32>,
    /// Retransmit selections.
    pub retransmits: Vec<(u32, u32)>,
    /// Final pipe estimate.
    pub pipe: u32,
    /// CUBIC `W_max` (zero for other algorithms).
    pub w_max: u32,
}

/// Small deterministic LCG.
#[derive(Clone, Debug)]
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn range(&mut self, low: u64, high: u64) -> u64 {
        low + self.next() % (high - low + 1)
    }

    fn bool(&mut self, one_in: u64) -> bool {
        self.range(1, one_in) == one_in
    }
}

impl Scenario {
    /// Generate a deterministic scenario from a seed.
    #[must_use]
    pub fn generate(seed: u64) -> Self {
        let mut rng = Lcg::new(seed);
        let kind = match seed % 3 {
            0 => Kind::NewReno,
            1 => Kind::Cubic,
            _ => Kind::Rack,
        };
        let config = Config {
            kind,
            srtt_ms: rng.range(20, 200) as u32,
            cwnd: 10 * MSS,
            ssthresh: rng.range(8, 80) as u32 * MSS,
        };
        let events = rng.range(8, 16) as usize;
        let mut trace = Vec::with_capacity(events);
        let mut at = 0u64;
        let mut ack = FIRST_SEQ;
        for index in 0..events {
            at += rng.range(10, 120);
            if rng.bool(2) {
                ack = ack.saturating_add(rng.range(0, 2) as u32 * MSS);
            }
            let loss = index > 0 && index % 5 == 0;
            if seed.is_multiple_of(7) && index + 1 == events {
                ack = FIRST_SEQ + SEGMENTS as u32 * MSS;
            }
            let rtt = if rng.bool(3) {
                Some(rng.range(20, 250) as u32)
            } else {
                None
            };
            trace.push(Event { at, ack, loss, rtt });
        }
        Self {
            seed,
            config,
            events: trace,
        }
    }
}

fn seq_max(a: u32, b: u32) -> u32 {
    if a.wrapping_sub(b) < 0x8000_0000 {
        a
    } else {
        b
    }
}

/// Independent CUBIC curve in segments (RFC 9438, not the K269 fixed-point expression).
fn cubic_target(w_max: u32, cwnd_epoch: u32, elapsed_ms: u64, srtt_ms: u32) -> u32 {
    let wmax = f64::from(w_max.max(MSS)) / f64::from(MSS);
    let epoch = f64::from(cwnd_epoch.max(MSS)) / f64::from(MSS);
    let diff = (wmax - epoch).max(0.0);
    let k = (diff / 0.4).max(0.0).powf(1.0 / 3.0);
    let t = elapsed_ms as f64 / 1000.0 + srtt_ms as f64 / 1000.0;
    let curve = 0.4 * (t - k).powi(3) + wmax;
    let reno = 0.7 * wmax + t * f64::from(MSS) / f64::from(MSS.max(1));
    let target = curve.max(reno).max(2.0);
    target.mul_add(f64::from(MSS), 0.0).min(f64::from(u32::MAX)) as u32
}

/// Run one scenario through the independent model.
#[must_use]
pub fn run(scenario: &Scenario) -> Outcome {
    let config = scenario.config;
    let mut cwnd = config.cwnd.max(2 * MSS);
    let mut ssthresh = config.ssthresh.max(2 * MSS);
    let mut srtt = config.srtt_ms.max(1);
    let mut snd_una = FIRST_SEQ;
    let mut snd_max = FIRST_SEQ + SEGMENTS as u32 * MSS;
    let mut in_recovery = false;
    let mut recover = 0u32;
    let mut lost = Vec::new();
    let mut retransmits = Vec::new();
    let mut w_max = 0u32;
    let mut cwnd_epoch = cwnd;

    for event in &scenario.events {
        if let Some(sample) = event.rtt {
            srtt = (((u64::from(srtt) * 7) + u64::from(sample)) / 8).max(1) as u32;
        }
        let acked = event.ack.saturating_sub(snd_una).min(snd_max - snd_una);
        if event.ack > snd_una {
            snd_una = event.ack.min(snd_max);
        }
        if event.loss {
            lost.push(snd_una);
            if config.kind == Kind::Cubic {
                if w_max != 0 && cwnd < w_max {
                    w_max = (u64::from(cwnd) * 217 / 256) as u32;
                } else {
                    w_max = cwnd;
                }
                ssthresh = ((u64::from(cwnd) * 179 / 256) as u32).max(2 * MSS);
                cwnd = ssthresh;
                cwnd_epoch = cwnd;
            } else if config.kind == Kind::NewReno {
                ssthresh = (cwnd / 2).max(2 * MSS);
                cwnd = ssthresh;
            }
            if !in_recovery {
                in_recovery = true;
                recover = snd_max;
            }
            if let Some(range) = lost.last() {
                retransmits.push((*range, range.saturating_add(MSS)));
            }
        } else if !in_recovery && acked > 0 {
            if cwnd <= ssthresh {
                cwnd = cwnd
                    .saturating_add(acked.min(2 * MSS))
                    .min(config.ssthresh.max(2 * MSS) * 2);
            } else if config.kind == Kind::Cubic {
                let target = cubic_target(w_max, cwnd_epoch, event.at, srtt);
                let incr = (u64::from(target.saturating_sub(cwnd)) * u64::from(MSS)
                    / u64::from(cwnd.max(1))) as u32;
                cwnd = cwnd.saturating_add(incr);
            } else {
                let incr = (u64::from(MSS) * u64::from(MSS) / u64::from(cwnd.max(1))).max(1);
                cwnd = cwnd.saturating_add(incr as u32);
            }
        }
        if in_recovery && snd_una >= recover {
            in_recovery = false;
            cwnd = cwnd.max(ssthresh);
        }
        snd_max = seq_max(snd_max, snd_una);
    }

    let outstanding = snd_max.saturating_sub(snd_una);
    let lost_bytes = lost.len() as u32 * MSS;
    let pipe = outstanding.saturating_sub(lost_bytes).max(MSS);
    Outcome {
        cwnd,
        ssthresh,
        in_recovery,
        lost,
        retransmits,
        pipe,
        w_max,
    }
}

/// Stable one-line trace digest for the golden file.
#[must_use]
pub fn trace_row(scenario: &Scenario, outcome: &Outcome) -> String {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    for event in &scenario.events {
        for byte in event.ack.to_le_bytes() {
            digest ^= u64::from(byte);
            digest = digest.wrapping_mul(0x100_0000_01b3);
        }
        digest ^= event.at;
    }
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:016x}",
        scenario.seed,
        scenario.events.len(),
        outcome.cwnd,
        outcome.ssthresh,
        u8::from(outcome.in_recovery),
        outcome.lost.len(),
        outcome.retransmits.len(),
        outcome.pipe,
        digest
    )
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
