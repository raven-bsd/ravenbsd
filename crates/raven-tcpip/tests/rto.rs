// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RFC 6298 estimator vectors and retransmission behaviour.

#![cfg(feature = "testutil")]

use raven_tcpip::RtoEstimator;

#[derive(Clone, Debug, Default)]
struct Step {
    kind: String,
    rtt_ms: u64,
    srtt_q8: u64,
    rttvar_q8: u64,
    rto_computed_ms: u64,
    rto_ms: u64,
}

#[derive(Clone, Debug, Default)]
struct Sequence {
    name: String,
    rto_min_ms: u64,
    rto_max_ms: u64,
    clock_granularity_ms: u64,
    timeline: Vec<Step>,
    expected: Vec<Step>,
}

fn parse_table(text: &str) -> Step {
    let mut step = Step::default();
    let text = text
        .trim()
        .trim_end_matches(',')
        .trim_start_matches('{')
        .trim_end_matches('}');
    for part in text.split(',') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        let number = value.parse::<u64>().unwrap_or(0);
        match key {
            "kind" => step.kind = value.to_string(),
            "rtt_ms" => step.rtt_ms = number,
            "srtt_q8" => step.srtt_q8 = number,
            "rttvar_q8" => step.rttvar_q8 = number,
            "rto_computed_ms" => step.rto_computed_ms = number,
            "rto_ms" => step.rto_ms = number,
            _ => {}
        }
    }
    step
}

fn parse_sequences() -> Vec<Sequence> {
    let text = include_str!("../../../spec/tcpip/vectors/rto.toml");
    let mut sequences: Vec<Sequence> = Vec::new();
    let mut current: Option<Sequence> = None;
    let mut array = 0u8;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[sequence]]" {
            if let Some(sequence) = current.take() {
                sequences.push(sequence);
            }
            current = Some(Sequence::default());
            array = 0;
            continue;
        }
        let Some(sequence) = current.as_mut() else {
            continue;
        };
        if line == "timeline = [" {
            array = 1;
            continue;
        }
        if line == "expected = [" {
            array = 2;
            continue;
        }
        if line == "]" {
            array = 0;
            continue;
        }
        if line.starts_with('{') {
            let step = parse_table(line);
            if array == 1 {
                sequence.timeline.push(step);
            } else if array == 2 {
                sequence.expected.push(step);
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "name" => sequence.name = value.to_string(),
                "rto_min_ms" => sequence.rto_min_ms = value.parse().unwrap_or(0),
                "rto_max_ms" => sequence.rto_max_ms = value.parse().unwrap_or(0),
                "clock_granularity_ms" => {
                    sequence.clock_granularity_ms = value.parse().unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    if let Some(sequence) = current {
        sequences.push(sequence);
    }
    sequences
}

fn check_sequence(name: &str) {
    let sequences = parse_sequences();
    let sequence = sequences
        .iter()
        .find(|sequence| sequence.name == name)
        .unwrap_or_else(|| panic!("missing RTO sequence {name}"));
    let mut estimator = RtoEstimator::new(
        1000,
        sequence.rto_min_ms,
        sequence.rto_max_ms,
        sequence.clock_granularity_ms,
    );
    for (index, step) in sequence.timeline.iter().enumerate() {
        match step.kind.as_str() {
            "sample" => estimator.sample(step.rtt_ms),
            "timeout" => estimator.timeout(),
            other => panic!("unknown step {other}"),
        }
        let expected = &sequence.expected[index];
        assert_eq!(
            estimator.srtt_q8(),
            expected.srtt_q8,
            "{name} step {index} srtt"
        );
        assert_eq!(
            estimator.rttvar_q8(),
            expected.rttvar_q8,
            "{name} step {index} rttvar"
        );
        if step.kind == "sample" {
            assert_eq!(
                estimator.rto_computed_ms(),
                expected.rto_computed_ms,
                "{name} step {index} computed"
            );
        }
        assert_eq!(
            estimator.rto_ms(),
            expected.rto_ms,
            "{name} step {index} rto"
        );
    }
}

macro_rules! rto_test {
    ($function:ident, $name:literal) => {
        #[test]
        fn $function() {
            check_sequence($name);
        }
    };
}

rto_test!(sequence_single_100, "single-100");
rto_test!(sequence_single_1, "single-1");
rto_test!(sequence_two_100_200, "two-100-200");
rto_test!(sequence_decreasing, "decreasing");
rto_test!(sequence_increasing, "increasing");
rto_test!(sequence_spike, "spike");
rto_test!(sequence_zero_rtt, "zero-rtt");
rto_test!(sequence_large_5000, "large-5000");
rto_test!(sequence_cap_20000, "cap-20000");
rto_test!(sequence_min_clamp_100, "min-clamp-100");
rto_test!(sequence_backoff_100, "backoff-100");
rto_test!(sequence_backoff_cap, "backoff-cap");
rto_test!(sequence_backoff_then_sample, "backoff-then-sample");
rto_test!(sequence_fractional_rounding, "fractional-rounding");
rto_test!(sequence_alternating, "alternating");
rto_test!(sequence_steady_40, "steady-40");
rto_test!(sequence_rttvar_dominated, "rttvar-dominated");
rto_test!(sequence_min_default_200, "min-default-200");
rto_test!(sequence_backoff_to_cap, "backoff-to-cap");
rto_test!(sequence_rto_max_clamp, "rto-max-clamp");
rto_test!(
    sequence_timeout_before_any_sample,
    "timeout-before-any-sample"
);
rto_test!(sequence_many_samples, "many-samples");

#[test]
fn every_vector_sequence_is_present() {
    let sequences = parse_sequences();
    assert_eq!(sequences.len(), 22);
    let mut samples = 0;
    let mut timeouts = 0;
    for sequence in &sequences {
        for step in &sequence.timeline {
            match step.kind.as_str() {
                "sample" => samples += 1,
                "timeout" => timeouts += 1,
                _ => {}
            }
        }
    }
    assert_eq!(samples, 54);
    assert_eq!(timeouts, 12);
}
