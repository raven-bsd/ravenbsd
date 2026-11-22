// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exact vectors, state transitions and 10k randomized BBR traces.

use crate::model::{BbrModel, Config, PROBE_BW_GAINS_X256, State, Timely, trace_header};
use crate::rng::Rng;

fn config() -> Config {
    Config {
        mss: 1_000,
        initial_cwnd: 10_000,
        initial_bw_bps: 62_500,
        bw_samples: 10,
        startup_rounds: 3,
        probe_rtt_interval_ms: 10_000,
        probe_rtt_ms: 200,
        max_cwnd: 4_000_000,
    }
}

#[test]
fn pacing_and_target_cwnd_vectors_match_integer_bbr_math() {
    let mut model = BbrModel::new(config());
    model.on_delivery(1_000_000, 100_000, 1_000);
    assert_eq!(model.bw_bps, 10_000_000);
    assert_eq!(model.min_rtt_us, 100_000);
    assert_eq!(model.state, State::Startup);
    assert_eq!(model.pacing_gain_x256, 739);
    assert_eq!(model.pacing_rate_bps(), 28_867_187);
    assert_eq!(model.pacing_delay_us(1_000), 34);
    assert_eq!(model.pacing_len(1_000), 28_867);

    model.state = State::ProbeBw;
    model.substate = 0;
    model.pacing_gain_x256 = PROBE_BW_GAINS_X256[0];
    assert_eq!(model.pacing_delay_us(1_000), 80);
    assert_eq!(model.pacing_len(1_000), 12_500);

    model.substate = 2;
    model.pacing_gain_x256 = PROBE_BW_GAINS_X256[2];
    assert_eq!(model.pacing_delay_us(1_000), 100);
    assert_eq!(model.pacing_len(1_000), 10_000);
    assert_eq!(model.target_cwnd(), 2_002_000);
}

#[test]
fn startup_drains_into_probe_bw_and_cycles_gains() {
    let mut model = BbrModel::new(config());
    for (index, bytes) in [10_000u32, 20_000, 30_000, 40_000].into_iter().enumerate() {
        model.on_delivery(bytes, 100_000, index as u32 * 100);
        model.on_round(bytes, index as u32 * 100);
        assert_eq!(model.state, State::Startup);
    }
    assert_eq!(model.full_bw_bps, 400_000);
    for round in 0..2 {
        model.on_round(40_000, 1_000 + round);
        assert_eq!(model.state, State::Startup);
    }
    model.on_round(40_000, 1_100);
    assert_eq!(model.state, State::Drain);
    assert_eq!(model.pacing_gain_x256, 88);
    model.on_round(0, 1_200);
    assert_eq!(model.state, State::ProbeBw);
    assert_eq!(model.pacing_gain_x256, 320);
    model.on_round(0, 1_300);
    assert_eq!(model.pacing_gain_x256, 192);
    model.on_round(0, 1_400);
    assert_eq!(model.pacing_gain_x256, 256);
}

#[test]
fn timely_thresholds_and_decrease_gain_match_the_donor() {
    let mut model = BbrModel::new(config());
    assert_eq!(model.timely(100_000, 0, 0), Timely::Hold);
    model.min_rtt_us = 100_000;
    assert_eq!(model.timely(300_000, 0, 1), Timely::Decrease);
    assert_eq!(model.timely_gain_pct, 80);
    assert_eq!(model.timely(125_000, 0, 2), Timely::Increase);
    assert_eq!(model.timely_gain_pct, 85);
    assert_eq!(model.timely(200_000, -1, 3), Timely::Increase);
    assert_eq!(model.timely_gain_pct, 90);
    assert_eq!(model.timely(200_000, 1, 4), Timely::Decrease);
    assert_eq!(model.timely_gain_pct, 72);
    assert_eq!(model.timely_decreases, 1);
}

#[test]
fn probe_rtt_enters_after_ten_seconds_caps_cwnd_and_restores() {
    let mut model = BbrModel::new(config());
    model.state = State::ProbeBw;
    model.full_bw_bps = 1_000_000;
    model.cwnd = 100_000;
    model.last_probe_rtt_ms = 0;
    assert_eq!(model.poll(9_999), "none");
    assert_eq!(model.poll(10_000), "enter_probe_rtt");
    assert_eq!(model.state, State::ProbeRtt);
    assert_eq!(model.cwnd, 4_000);
    assert_eq!(model.poll(10_100), "none");
    assert_eq!(model.poll(10_200), "exit_probe_rtt");
    assert_eq!(model.state, State::ProbeBw);
    assert_eq!(model.cwnd, 100_000);
    assert_eq!(model.trace.last().expect("trace").deadline, None);
    assert_eq!(model.last_probe_rtt_ms, 10_000);
}

#[test]
fn trace_format_is_the_shared_five_column_schema() {
    assert_eq!(trace_header(), "now,event,action,deadline,value");
    let mut model = BbrModel::new(config());
    model.on_delivery(1_000, 100_000, 10);
    model.on_round(1_000, 20);
    model.timely(100_000, 0, 30);
    model.poll(10_000);
    assert_eq!(model.trace.len(), 4);
    for step in &model.trace {
        let line = step.line();
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 5, "line {line}");
        assert!(!fields[0].is_empty());
        assert!(!fields[1].is_empty());
        assert!(!fields[2].is_empty());
        assert!(fields[3] == "-" || fields[3].parse::<u32>().is_ok());
        assert!(fields[4].parse::<u64>().is_ok());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    trace: Vec<String>,
    min_rtt: u32,
    cwnd: u32,
    state: State,
}

fn run(seed: u64) -> Run {
    let mut rng = Rng::new(seed);
    let mss = rng.range(500, 1_500);
    let mut config = config();
    config.mss = mss;
    config.initial_cwnd = mss * rng.range(4, 10);
    config.max_cwnd = mss * rng.range(16, 256);
    config.initial_bw_bps = u64::from(rng.range(10_000, 1_000_000));
    config.probe_rtt_interval_ms = rng.range(5_000, 20_000);
    let mut model = BbrModel::new(config);
    let mut now = 0u32;
    let mut min_rtt = 0u32;
    for _ in 0..1_000 {
        now = now.saturating_add(rng.range(1, 100));
        match rng.below(8) {
            0..=3 => {
                let bytes = rng.range(1, 64_000);
                let rtt = rng.range(1_000, 1_000_000);
                model.on_delivery(bytes, rtt, now);
                if min_rtt != 0 {
                    assert!(model.min_rtt_us <= min_rtt, "min RTT grows");
                }
                if model.min_rtt_us != 0 {
                    min_rtt = model.min_rtt_us;
                }
            }
            4 => {
                let flight = rng.range(0, config.max_cwnd * 2);
                model.on_round(flight, now);
            }
            5 => {
                let rtt = rng.range(1_000, 1_000_000);
                let diff = i32::try_from(rng.range(0, 20_000)).unwrap_or(0) - 10_000;
                model.timely(rtt, diff, now);
            }
            6 => {
                model.poll(now);
            }
            _ => {
                let bytes = rng.range(1, 32_000);
                let rtt = rng.range(1_000, 500_000);
                model.on_delivery(bytes, rtt, now);
                model.on_round(bytes, now);
            }
        }
        assert!(model.pacing_rate_bps() > 0);
        assert!(model.pacing_delay_us(1_000) > 0);
        assert!(model.pacing_len(1_000) > 0);
        assert!(model.cwnd >= 4 * mss);
        assert!(model.cwnd <= config.max_cwnd);
        assert!(model.substate < 8);
        assert!(model.samples.len() <= config.bw_samples);
        if model.state == State::ProbeRtt {
            assert_eq!(model.cwnd, 4 * mss);
        }
        let line = model
            .trace
            .last()
            .map(|step| step.line())
            .unwrap_or_default();
        assert_eq!(line.split(',').count(), 5);
    }
    assert!(model.trace.len() >= 1_000);
    Run {
        trace: model.trace.iter().map(|step| step.line()).collect(),
        min_rtt: model.min_rtt_us,
        cwnd: model.cwnd,
        state: model.state,
    }
}

#[test]
fn ten_thousand_randomized_trace_steps_are_deterministic_and_bounded() {
    let seeds = [
        0x3260_0001,
        0x3260_0002,
        0x3260_0003,
        0x3260_0004,
        0x3260_0005,
        0x3260_0006,
        0x3260_0007,
        0x3260_0008,
        0x3260_0009,
        0x3260_000a,
    ];
    let mut total = 0usize;
    for seed in seeds {
        let first = run(seed);
        let second = run(seed);
        assert_eq!(first, second, "seed {seed:#x} is deterministic");
        assert!(first.trace.len() >= 1_000);
        assert!(first.cwnd >= 4 * 500);
        assert!(first.min_rtt > 0);
        total += 1_000;
    }
    assert_eq!(total, 10_000, "10k randomized trace events");
}
