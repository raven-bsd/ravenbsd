// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent RACK BBR bandwidth, pacing, timely and probe-RTT model.
//!
//! Behaviour follows the FreeBSD `bbr.c`/`rack.c` functions and the BBR/RACK-TLP drafts named in
//! the ticket. The model is original Rust; no donor expression is copied. It is an oracle for the
//! future full RACK/BBR implementation, not a second copy of that state machine.

/// BBR fixed-point unit (1.0 == 256).
pub const BBR_UNIT: u32 = 256;
/// Startup pacing gain, `BBR_UNIT * 2885 / 1000 + 1`.
pub const HIGH_GAIN_X256: u32 = 739;
/// Drain pacing gain, `BBR_UNIT * 1000 / 2885`.
pub const DRAIN_GAIN_X256: u32 = 88;
/// Default cwnd gain (2.0).
pub const CWND_GAIN_X256: u32 = 512;
/// Probe-BW gain cycle: 1.25, 0.75, then six 1.0 rounds.
pub const PROBE_BW_GAINS_X256: [u32; 8] = [320, 192, 256, 256, 256, 256, 256, 256];
/// Probe-RTT duration, milliseconds.
pub const PROBE_RTT_MS: u32 = 200;
/// Probe-RTT interval, milliseconds.
pub const PROBE_RTT_INTERVAL_MS: u32 = 10_000;
/// Minimum congestion window in MSS units.
pub const MIN_CWND_MSS: u32 = 4;
/// Extra quanta (2 MSS by default).
pub const QUANTA_MSS: u32 = 2;

/// Timing and sizing configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Sender MSS.
    pub mss: u32,
    /// Initial congestion window.
    pub initial_cwnd: u32,
    /// Initial bandwidth floor, bytes per second.
    pub initial_bw_bps: u64,
    /// Delivery-rate samples kept by the max filter.
    pub bw_samples: usize,
    /// Rounds without growth before leaving Startup.
    pub startup_rounds: u32,
    /// Probe-RTT interval, milliseconds.
    pub probe_rtt_interval_ms: u32,
    /// Probe-RTT duration, milliseconds.
    pub probe_rtt_ms: u32,
    /// Congestion-window ceiling.
    pub max_cwnd: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mss: 1_460,
            initial_cwnd: 10 * 1_460,
            initial_bw_bps: 62_500,
            bw_samples: 10,
            startup_rounds: 3,
            probe_rtt_interval_ms: PROBE_RTT_INTERVAL_MS,
            probe_rtt_ms: PROBE_RTT_MS,
            max_cwnd: 256 * 1_460,
        }
    }
}

/// BBR state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Startup: exponential bandwidth growth.
    Startup,
    /// Drain: reduce the queue built in Startup.
    Drain,
    /// Probe-BW: eight-round gain cycle.
    ProbeBw,
    /// Probe-RTT: 200 ms, minimum window.
    ProbeRtt,
}

impl State {
    /// Stable trace label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Drain => "drain",
            Self::ProbeBw => "probe_bw",
            Self::ProbeRtt => "probe_rtt",
        }
    }
}

/// Timely RTT-gradient judgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timely {
    /// Increase the bandwidth multiplier.
    Increase,
    /// Hold.
    Hold,
    /// Decrease the bandwidth multiplier (code 1 or 2 in the donor).
    Decrease,
}

impl Timely {
    /// Stable trace label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Hold => "hold",
            Self::Decrease => "decrease",
        }
    }
}

/// The stable 5-column trace record shared with the K319/K320 model schema.
///
/// K320's fifth column is `reo_wnd`; K326's fifth column carries the BBR metric (bytes/s, cwnd or
/// gain). The column order and the `-` deadline sentinel are identical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    /// Virtual milliseconds.
    pub now_ms: u32,
    /// Event label.
    pub event: &'static str,
    /// Action/label for the event.
    pub action: &'static str,
    /// Deadline, if the action armed one.
    pub deadline: Option<u32>,
    /// Subsystem value (pacing rate, cwnd or gain).
    pub value: u64,
}

impl TraceStep {
    /// Stable CSV line: `now,event,action,deadline,value`.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{},{},{},{},{}",
            self.now_ms,
            self.event,
            self.action,
            self.deadline
                .map_or_else(|| "-".to_owned(), |deadline| deadline.to_string()),
            self.value
        )
    }
}

/// The fixed header for the shared trace format.
#[must_use]
pub const fn trace_header() -> &'static str {
    "now,event,action,deadline,value"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProbeRtt {
    saved_cwnd: u32,
    deadline: u32,
}

/// The independent BBR reference model.
#[derive(Clone, Debug)]
pub struct BbrModel {
    /// Configuration.
    pub config: Config,
    /// Current state.
    pub state: State,
    /// Probe-BW substate, 0..8.
    pub substate: u8,
    /// Delivery-rate samples (bytes per second).
    pub samples: Vec<u64>,
    /// Max-filter bandwidth, bytes per second.
    pub bw_bps: u64,
    /// Full-pipe bandwidth seen in Startup.
    pub full_bw_bps: u64,
    /// Consecutive Startup rounds without 25% growth.
    pub rounds_without_growth: u32,
    /// Minimum RTT, microseconds (0 until measured).
    pub min_rtt_us: u32,
    /// Smoothed RTT, microseconds.
    pub srtt_us: u32,
    /// RTT used for the cwnd target, microseconds.
    pub rtt_prop_us: u32,
    /// Congestion window.
    pub cwnd: u32,
    /// Current pacing gain in BBR_UNIT.
    pub pacing_gain_x256: u32,
    /// Current cwnd gain in BBR_UNIT.
    pub cwnd_gain_x256: u32,
    /// Timely bandwidth multiplier, percent.
    pub timely_gain_pct: u32,
    /// Timely decrease count.
    pub timely_decreases: u32,
    /// Last Probe-RTT entry time, milliseconds.
    pub last_probe_rtt_ms: u32,
    probe: Option<ProbeRtt>,
    /// Stable trace.
    pub trace: Vec<TraceStep>,
}

impl BbrModel {
    /// Create a model in Startup.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: State::Startup,
            substate: 0,
            samples: Vec::new(),
            bw_bps: config.initial_bw_bps.max(1),
            full_bw_bps: 0,
            rounds_without_growth: 0,
            min_rtt_us: 0,
            srtt_us: 0,
            rtt_prop_us: 0,
            cwnd: config.initial_cwnd.max(MIN_CWND_MSS * config.mss),
            pacing_gain_x256: HIGH_GAIN_X256,
            cwnd_gain_x256: CWND_GAIN_X256,
            timely_gain_pct: 100,
            timely_decreases: 0,
            last_probe_rtt_ms: 0,
            probe: None,
            trace: Vec::new(),
        }
    }

    fn record(&mut self, now_ms: u32, event: &'static str, action: &'static str, value: u64) {
        self.trace.push(TraceStep {
            now_ms,
            event,
            action,
            deadline: self.probe.as_ref().map(|probe| probe.deadline),
            value,
        });
    }

    /// Current pacing rate, bytes per second.
    #[must_use]
    pub fn pacing_rate_bps(&self) -> u64 {
        self.bw_bps
            .saturating_mul(u64::from(self.pacing_gain_x256))
            .checked_div(u64::from(BBR_UNIT))
            .unwrap_or(0)
            .max(1)
    }

    /// Pacing delay for `len` bytes, microseconds.
    #[must_use]
    pub fn pacing_delay_us(&self, len: u32) -> u32 {
        self.pacing_delay_gain_us(len, self.pacing_gain_x256)
    }

    /// Pacing delay for an explicit gain, microseconds.
    #[must_use]
    pub fn pacing_delay_gain_us(&self, len: u32, gain_x256: u32) -> u32 {
        if len == 0 {
            return 0;
        }
        let numerator = u64::from(len)
            .saturating_mul(1_000_000)
            .saturating_mul(u64::from(BBR_UNIT));
        let denominator = u64::from(gain_x256.max(1)).saturating_mul(self.bw_bps.max(1));
        u32::try_from(numerator / denominator)
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// Pacing length for a time budget, bytes.
    #[must_use]
    pub fn pacing_len(&self, usecs: u32) -> u32 {
        let numerator = u64::from(usecs)
            .saturating_mul(self.bw_bps)
            .saturating_mul(u64::from(self.pacing_gain_x256));
        let denominator = 1_000_000u64.saturating_mul(u64::from(BBR_UNIT));
        u32::try_from(numerator / denominator)
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// Bytes in flight at the propagation RTT.
    #[must_use]
    pub fn bdp_bytes(&self) -> u64 {
        let rtt = u64::from(self.rtt_prop_us.max(self.srtt_us).max(1));
        self.bw_bps.saturating_mul(rtt) / 1_000_000
    }

    /// Target congestion window for the current state and gain.
    #[must_use]
    pub fn target_cwnd(&self) -> u32 {
        if self.state == State::ProbeRtt {
            return MIN_CWND_MSS.saturating_mul(self.config.mss);
        }
        if self.rtt_prop_us == 0 || self.bw_bps == 0 {
            return self.config.initial_cwnd.max(MIN_CWND_MSS * self.config.mss);
        }
        let bdp = self.bdp_bytes();
        let raw = bdp
            .saturating_mul(u64::from(self.cwnd_gain_x256))
            .checked_div(u64::from(BBR_UNIT))
            .unwrap_or(0);
        let mss = u64::from(self.config.mss.max(1));
        let mut cwnd = raw.saturating_add(mss - 1) / mss * mss;
        cwnd = cwnd.saturating_add(u64::from(QUANTA_MSS) * mss);
        if self.state == State::ProbeBw && self.substate == 0 {
            cwnd = cwnd.saturating_add(2 * mss);
        }
        if (cwnd / mss) % 2 == 1 {
            cwnd = cwnd.saturating_add(mss);
        }
        let minimum = u64::from(MIN_CWND_MSS.saturating_mul(self.config.mss));
        let maximum = u64::from(self.config.max_cwnd.max(MIN_CWND_MSS * self.config.mss));
        u32::try_from(cwnd.clamp(minimum, maximum)).unwrap_or(self.config.max_cwnd)
    }

    /// Process one delivery sample and update the rate/RTT filters and cwnd.
    pub fn on_delivery(&mut self, delivered_bytes: u32, rtt_us: u32, now_ms: u32) {
        let rtt = rtt_us.max(1);
        let sample = u64::from(delivered_bytes)
            .saturating_mul(1_000_000)
            .checked_div(u64::from(rtt))
            .unwrap_or(0);
        if sample > 0 {
            self.samples.push(sample);
            if self.samples.len() > self.config.bw_samples {
                self.samples.remove(0);
            }
            self.bw_bps = self
                .samples
                .iter()
                .copied()
                .max()
                .unwrap_or(self.config.initial_bw_bps)
                .max(self.config.initial_bw_bps);
        }
        if self.min_rtt_us == 0 || rtt < self.min_rtt_us {
            self.min_rtt_us = rtt;
        }
        self.rtt_prop_us = self.min_rtt_us;
        self.srtt_us = if self.srtt_us == 0 {
            rtt
        } else {
            (self.srtt_us.saturating_mul(7).saturating_add(rtt)) / 8
        };
        self.cwnd = self.target_cwnd();
        self.record(
            now_ms,
            "delivery",
            self.state.label(),
            self.pacing_rate_bps(),
        );
    }

    /// Advance one round and update state/gains.
    pub fn on_round(&mut self, flight_bytes: u32, now_ms: u32) {
        match self.state {
            State::Startup => {
                if self.bw_bps >= self.full_bw_bps.saturating_mul(5) / 4 {
                    self.full_bw_bps = self.bw_bps;
                    self.rounds_without_growth = 0;
                } else {
                    self.rounds_without_growth = self.rounds_without_growth.saturating_add(1);
                    if self.rounds_without_growth >= self.config.startup_rounds {
                        self.state = State::Drain;
                        self.pacing_gain_x256 = DRAIN_GAIN_X256;
                        self.cwnd_gain_x256 = CWND_GAIN_X256;
                    }
                }
            }
            State::Drain => {
                if u64::from(flight_bytes) <= self.bdp_bytes() {
                    self.state = State::ProbeBw;
                    self.substate = 0;
                    self.pacing_gain_x256 = PROBE_BW_GAINS_X256[0];
                }
            }
            State::ProbeBw => {
                self.substate = (self.substate + 1) % 8;
                self.pacing_gain_x256 = PROBE_BW_GAINS_X256[usize::from(self.substate)];
            }
            State::ProbeRtt => {}
        }
        self.cwnd = self.target_cwnd();
        self.record(now_ms, "round", self.state.label(), u64::from(self.cwnd));
    }

    /// Timely judgement and multiplier update.
    pub fn timely(&mut self, rtt_us: u32, rtt_diff_us: i32, now_ms: u32) -> Timely {
        let judgement = if self.min_rtt_us == 0 {
            Timely::Hold
        } else if rtt_us >= self.min_rtt_us.saturating_mul(3) {
            Timely::Decrease
        } else if rtt_us <= self.min_rtt_us.saturating_add(self.min_rtt_us / 4) || rtt_diff_us <= 0
        {
            Timely::Increase
        } else {
            Timely::Decrease
        };
        match judgement {
            Timely::Increase => {
                self.timely_gain_pct = self.timely_gain_pct.saturating_add(5).min(200);
                self.timely_decreases = 0;
            }
            Timely::Decrease => {
                self.timely_gain_pct = self.timely_gain_pct.saturating_mul(80) / 100;
                self.timely_gain_pct = self.timely_gain_pct.max(5);
                self.timely_decreases = self.timely_decreases.saturating_add(1);
            }
            Timely::Hold => {}
        }
        self.record(
            now_ms,
            "timely",
            judgement.label(),
            u64::from(self.timely_gain_pct),
        );
        judgement
    }

    /// Poll the Probe-RTT timer.
    pub fn poll(&mut self, now_ms: u32) -> &'static str {
        if self.state == State::ProbeRtt
            && let Some(probe) = &self.probe
            && now_ms >= probe.deadline
        {
            self.cwnd = probe.saved_cwnd.min(self.config.max_cwnd);
            self.probe = None;
            self.state = if self.full_bw_bps == 0 {
                State::Startup
            } else {
                State::ProbeBw
            };
            self.pacing_gain_x256 = if self.state == State::Startup {
                HIGH_GAIN_X256
            } else {
                PROBE_BW_GAINS_X256[usize::from(self.substate)]
            };
            self.cwnd_gain_x256 = CWND_GAIN_X256;
            self.record(now_ms, "poll", "exit_probe_rtt", u64::from(self.cwnd));
            return "exit_probe_rtt";
        }
        if self.state != State::ProbeRtt
            && now_ms.saturating_sub(self.last_probe_rtt_ms) >= self.config.probe_rtt_interval_ms
        {
            let saved_cwnd = self.cwnd;
            let deadline = now_ms.saturating_add(self.config.probe_rtt_ms);
            self.probe = Some(ProbeRtt {
                saved_cwnd,
                deadline,
            });
            self.state = State::ProbeRtt;
            self.cwnd = MIN_CWND_MSS.saturating_mul(self.config.mss);
            self.pacing_gain_x256 = BBR_UNIT;
            self.cwnd_gain_x256 = BBR_UNIT;
            self.last_probe_rtt_ms = now_ms;
            self.record(now_ms, "poll", "enter_probe_rtt", u64::from(self.cwnd));
            return "enter_probe_rtt";
        }
        self.record(now_ms, "poll", "none", u64::from(self.cwnd));
        "none"
    }
}
