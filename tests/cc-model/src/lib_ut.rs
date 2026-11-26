// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit, comparison and golden-trace tests for the congestion-control model.

use super::*;
use std::fs;
use std::path::PathBuf;

fn scenario(kind: Kind, events: Vec<Event>) -> Scenario {
    Scenario {
        seed: 0,
        config: Config {
            kind,
            ..Config::default()
        },
        events,
    }
}

#[test]
fn newreno_slow_start_matches_the_ki_implementation() {
    use raven_tcpip::{CcContext, CongestionControl, NewReno};

    let events = vec![
        Event {
            at: 10,
            ack: FIRST_SEQ + MSS,
            loss: false,
            rtt: Some(100),
        },
        Event {
            at: 20,
            ack: FIRST_SEQ + 2 * MSS,
            loss: false,
            rtt: None,
        },
        Event {
            at: 30,
            ack: FIRST_SEQ + 3 * MSS,
            loss: false,
            rtt: None,
        },
    ];
    let model = run(&scenario(Kind::NewReno, events.clone()));
    let mut newreno = NewReno::new(MSS, 10 * MSS, 1_000_000, 1_000_000);
    let mut ctx = CcContext {
        mss: MSS,
        ..CcContext::default()
    };
    for event in &events {
        let acked = event.ack.saturating_sub(ctx.snd_una);
        ctx.snd_una = event.ack;
        newreno.on_ack(&mut ctx, acked, None);
    }
    assert_eq!(model.cwnd, newreno.cwnd());
    assert_eq!(model.ssthresh, newreno.ssthresh());
}

#[test]
fn cubic_fast_convergence_reduces_w_max_on_a_second_loss() {
    let mut trace = scenario(
        Kind::Cubic,
        vec![
            Event {
                at: 100,
                ack: FIRST_SEQ + 4 * MSS,
                loss: true,
                rtt: Some(80),
            },
            Event {
                at: 200,
                ack: FIRST_SEQ + 4 * MSS,
                loss: true,
                rtt: Some(80),
            },
        ],
    );
    trace.config.ssthresh = 1_000_000;
    let outcome = run(&trace);
    assert!(outcome.w_max > 0);
    assert!(outcome.w_max < 10 * MSS, "fast convergence reduced W_max");
    assert!(outcome.ssthresh < 10 * MSS);
}

#[test]
fn rack_pipe_subtracts_lost_bytes_and_keeps_one_mss() {
    let events = vec![Event {
        at: 100,
        ack: FIRST_SEQ,
        loss: true,
        rtt: Some(50),
    }];
    let outcome = run(&scenario(Kind::Rack, events));
    assert_eq!(outcome.lost.len(), 1);
    assert_eq!(outcome.retransmits.len(), 1);
    assert!(outcome.pipe >= MSS);
    assert!(outcome.pipe <= SEGMENTS as u32 * MSS);
}

#[test]
fn generated_scenarios_are_deterministic_and_cover_all_algorithms() {
    let first = Scenario::generate(42);
    assert_eq!(first, Scenario::generate(42));
    assert_ne!(first.events, Scenario::generate(43).events);
    let kinds: Vec<Kind> = (1..=30)
        .map(|seed| Scenario::generate(seed).config.kind)
        .collect();
    assert!(kinds.contains(&Kind::NewReno));
    assert!(kinds.contains(&Kind::Cubic));
    assert!(kinds.contains(&Kind::Rack));
    assert!(first.events.windows(2).all(|pair| pair[0].at < pair[1].at));
}

fn trace_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("traces/cc-10k.tsv")
}

#[test]
fn ten_thousand_trace_file_matches_the_model() {
    let text = fs::read_to_string(trace_path()).expect("generated traces/cc-10k.tsv");
    let mut rows = 0usize;
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        assert_eq!(cells.len(), 9, "trace row shape: {line}");
        let seed: u64 = cells[0].parse().unwrap();
        let scenario = Scenario::generate(seed);
        let outcome = run(&scenario);
        assert_eq!(line, trace_row(&scenario, &outcome), "seed {seed}");
        rows += 1;
    }
    assert_eq!(rows, TRACE_COUNT as usize);
}
