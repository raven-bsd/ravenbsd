// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit and golden-trace tests for the RACK reference model.

use super::*;
use std::fs;
use std::path::PathBuf;

fn scenario(config: Config, events: Vec<Event>) -> Scenario {
    Scenario {
        seed: 0,
        config,
        events,
    }
}

#[test]
fn reo_wnd_is_base_plus_reordering_and_dsack_bounded() {
    let config = Config::default();
    assert_eq!(reo_wnd(config, 100, false, 0), 120);
    assert_eq!(reo_wnd(config, 100, true, 0), 145);
    assert_eq!(reo_wnd(config, 100, true, 2), 195);
    assert_eq!(reo_wnd(config, 100, true, 5), 200, "capped at 2 * srtt");

    let no_dsack = Config {
        use_dsack: false,
        ..config
    };
    assert_eq!(reo_wnd(no_dsack, 100, true, 5), 145);
    assert_eq!(reo_wnd(config, 0, false, 0), 2, "srtt clamp and 2*srtt cap");
}

#[test]
fn rack_marks_only_segments_older_than_a_later_sacked_one() {
    let config = Config::default();
    let events = vec![
        Event {
            at: 150,
            ack: FIRST_SEQ,
            sacks: vec![(FIRST_SEQ + 4 * MSS, FIRST_SEQ + 5 * MSS)],
            dsack: false,
            rtt: None,
        },
        Event {
            at: 300,
            ack: FIRST_SEQ + 2 * MSS,
            sacks: Vec::new(),
            dsack: false,
            rtt: None,
        },
    ];
    let outcome = run(&scenario(config, events));
    assert_eq!(
        outcome.lost,
        vec![(FIRST_SEQ + 2 * MSS, FIRST_SEQ + 4 * MSS)]
    );
    assert_eq!(
        outcome.retransmits,
        vec![
            (FIRST_SEQ, FIRST_SEQ + MSS),
            (FIRST_SEQ + 2 * MSS, FIRST_SEQ + 3 * MSS)
        ]
    );
    assert_eq!(outcome.recovery_entries, 1);
    assert_eq!(outcome.recovery_exits, 0);
}

#[test]
fn no_loss_before_the_reordering_window() {
    let config = Config::default();
    let events = vec![Event {
        at: 100,
        ack: FIRST_SEQ,
        sacks: vec![(FIRST_SEQ + 4 * MSS, FIRST_SEQ + 5 * MSS)],
        dsack: false,
        rtt: None,
    }];
    let outcome = run(&scenario(config, events));
    assert!(outcome.lost.is_empty(), "0+120 > 100, so nothing is lost");
    assert!(outcome.retransmits.is_empty());
    assert_eq!(
        outcome.pipe, 7000,
        "8000 outstanding minus the 1000-byte SACK"
    );
}

#[test]
fn retransmit_selects_lowest_lost_once_and_pipe_counts_it() {
    let config = Config::default();
    let events = vec![Event {
        at: 150,
        ack: FIRST_SEQ,
        sacks: vec![(FIRST_SEQ + 5 * MSS, FIRST_SEQ + 6 * MSS)],
        dsack: false,
        rtt: None,
    }];
    let outcome = run(&scenario(config, events));
    assert_eq!(outcome.retransmits, vec![(FIRST_SEQ, FIRST_SEQ + MSS)]);
    // 8000 outstanding - 1000 SACKed - 4000 lost + 1000 retransmitted.
    assert_eq!(outcome.pipe, 4000);
}

#[test]
fn dsack_raises_the_reordering_window_and_is_counted() {
    let config = Config {
        dupack_threshold: 2,
        ..Config::default()
    };
    let events = vec![Event {
        at: 130,
        ack: FIRST_SEQ + 2 * MSS,
        sacks: vec![
            (FIRST_SEQ, FIRST_SEQ + MSS),
            (FIRST_SEQ + 4 * MSS, FIRST_SEQ + 5 * MSS),
        ],
        dsack: true,
        rtt: None,
    }];
    let outcome = run(&scenario(config, events));
    assert_eq!(outcome.recovery_entries, 1, "DSACK triggers recovery");
    assert!(outcome.lost.is_empty(), "the grown window covers the age");
}

#[test]
fn generated_scenarios_are_deterministic_and_well_formed() {
    let first = Scenario::generate(42);
    let second = Scenario::generate(42);
    assert_eq!(first, second);
    assert_ne!(first.events, Scenario::generate(43).events);
    assert!((7..=14).contains(&first.events.len()));
    assert!(first.events.windows(2).all(|pair| pair[0].at < pair[1].at));
    assert!(
        first.events.iter().any(|event| event.dsack),
        "every seventh event is a DSACK"
    );
    assert!(first.events.iter().any(|event| event.ack >= FIRST_SEQ));
}

fn trace_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("traces/rack-10k.tsv")
}

#[test]
fn ten_thousand_trace_file_matches_the_model() {
    let text = fs::read_to_string(trace_path()).expect("generated traces/rack-10k.tsv");
    let mut rows = 0usize;
    let mut lost_rows = 0usize;
    let mut retransmit_rows = 0usize;
    let mut recovery_rows = 0usize;
    let mut recovery_exits = 0u32;
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        assert_eq!(cells.len(), 7, "trace row shape: {line}");
        let seed: u64 = cells[0].parse().unwrap();
        let scenario = Scenario::generate(seed);
        let outcome = run(&scenario);
        assert_eq!(trace_row(&scenario, &outcome), line, "seed {seed}");
        rows += 1;
        if !outcome.lost.is_empty() {
            lost_rows += 1;
        }
        if !outcome.retransmits.is_empty() {
            retransmit_rows += 1;
        }
        if outcome.recovery_entries > 0 {
            recovery_rows += 1;
        }
        recovery_exits += outcome.recovery_exits;
    }
    assert_eq!(rows, TRACE_COUNT as usize);
    assert!(lost_rows > 1000, "lost coverage: {lost_rows}");
    assert!(
        retransmit_rows > 1000,
        "retransmit coverage: {retransmit_rows}"
    );
    assert!(recovery_rows > 1000, "recovery coverage: {recovery_rows}");
    assert!(
        recovery_exits > 1000,
        "recovery exit coverage: {recovery_exits}"
    );
}
