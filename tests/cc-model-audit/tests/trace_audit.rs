// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Audit of the K341 10k traces and the donor formulas behind each trace class.

use std::fs;
use std::path::{Path, PathBuf};

use cc_model::{Config, Event, Kind, Scenario, TRACE_COUNT, run, trace_row};
use cc_model_audit::{
    FIRST_SEQ, MSS, SEGMENTS, TRACE_HEADER, donor_cubic_get_ssthresh, donor_cubic_k,
    donor_cubic_target, donor_cubic_w_cubic, donor_cubic_w_est, donor_newreno_cong_avoid,
    donor_newreno_md, donor_newreno_post_recovery, donor_rack_pipe, trace_digest,
};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn trace_path() -> PathBuf {
    workspace_path("cc-model/traces/cc-10k.tsv")
}

fn scenario(kind: Kind, cwnd: u32, ssthresh: u32, events: Vec<Event>) -> Scenario {
    Scenario {
        seed: 1,
        config: Config {
            kind,
            srtt_ms: 100,
            cwnd,
            ssthresh,
        },
        events,
    }
}

fn loss_then_full_ack() -> Vec<Event> {
    vec![
        Event {
            at: 10,
            ack: FIRST_SEQ,
            loss: true,
            rtt: None,
        },
        Event {
            at: 20,
            ack: FIRST_SEQ + SEGMENTS as u32 * MSS,
            loss: false,
            rtt: None,
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TraceRow {
    seed: u64,
    events: usize,
    cwnd: u32,
    ssthresh: u32,
    in_recovery: bool,
    lost: usize,
    retransmits: usize,
    pipe: u32,
    digest: u64,
}

fn parse_row(line: &str) -> TraceRow {
    let cells: Vec<&str> = line.split('\t').collect();
    assert_eq!(cells.len(), 9, "trace row has 9 columns: {line}");
    TraceRow {
        seed: cells[0].parse().expect("seed"),
        events: cells[1].parse().expect("events"),
        cwnd: cells[2].parse().expect("cwnd"),
        ssthresh: cells[3].parse().expect("ssthresh"),
        in_recovery: match cells[4] {
            "0" => false,
            "1" => true,
            other => panic!("in_recovery must be 0/1, got {other}"),
        },
        lost: cells[5].parse().expect("lost"),
        retransmits: cells[6].parse().expect("retransmits"),
        pipe: cells[7].parse().expect("pipe"),
        digest: u64::from_str_radix(cells[8], 16).expect("digest"),
    }
}

#[test]
fn k341_trace_regenerates_exactly() {
    let mut text = String::from(TRACE_HEADER);
    for seed in 1..=TRACE_COUNT {
        let scenario = Scenario::generate(seed);
        let outcome = run(&scenario);
        text.push_str(&trace_row(&scenario, &outcome));
        text.push('\n');
    }
    let expected = fs::read_to_string(trace_path()).expect("K341 trace file");
    assert_eq!(
        text, expected,
        "the checked-in K341 congestion-control oracle drifted"
    );
    assert_eq!(
        expected
            .lines()
            .filter(|line| !line.starts_with('#'))
            .count(),
        usize::try_from(TRACE_COUNT).expect("count")
    );
}

#[test]
fn every_trace_row_matches_the_model_and_its_digest() {
    let text = fs::read_to_string(trace_path()).expect("K341 trace file");
    let mut newreno = 0usize;
    let mut cubic = 0usize;
    let mut rack = 0usize;
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let row = parse_row(line);
        let scenario = Scenario::generate(row.seed);
        let outcome = run(&scenario);

        assert_eq!(
            trace_row(&scenario, &outcome),
            line,
            "seed {} row",
            row.seed
        );
        assert_eq!(
            trace_digest(&scenario.events),
            row.digest,
            "seed {} digest",
            row.seed
        );
        assert_eq!(
            row.events,
            scenario.events.len(),
            "seed {} event count",
            row.seed
        );
        assert!(
            (8..=16).contains(&row.events),
            "seed {} event range",
            row.seed
        );
        assert!(
            row.cwnd > 0 && row.ssthresh > 0,
            "seed {} windows",
            row.seed
        );
        assert!(row.lost <= row.events, "seed {} lost count", row.seed);
        assert!(
            row.retransmits <= row.lost,
            "seed {} retransmit count",
            row.seed
        );
        assert!(row.pipe >= MSS, "seed {} pipe floor", row.seed);
        match row.seed % 3 {
            0 => newreno += 1,
            1 => cubic += 1,
            _ => rack += 1,
        }
    }
    assert!(newreno > 3_000, "NewReno rows: {newreno}");
    assert!(cubic > 3_000, "CUBIC rows: {cubic}");
    assert!(rack > 3_000, "RACK rows: {rack}");
    assert_eq!(
        newreno + cubic + rack,
        usize::try_from(TRACE_COUNT).expect("count")
    );
}

#[test]
fn newreno_loss_and_recovery_differ_from_the_donor_formulas() {
    let scenario = scenario(Kind::NewReno, 10 * MSS, 20 * MSS, loss_then_full_ack());
    let outcome = run(&scenario);
    assert_eq!(outcome.cwnd, 5_000, "K341 model cwnd");
    assert_eq!(outcome.ssthresh, 5_000, "K341 model ssthresh");
    assert!(!outcome.in_recovery);

    let donor_ssthresh = donor_newreno_md(10 * MSS, MSS);
    let donor_cwnd = donor_newreno_post_recovery(0, donor_ssthresh, MSS);
    assert_eq!(donor_ssthresh, 5_000, "donor beta");
    assert_eq!(donor_cwnd, 2_000, "donor RFC 6582 floor");
    assert_ne!(
        outcome.cwnd, donor_cwnd,
        "K359-F01: the model sets cwnd=ssthresh at loss; the donor recovers via the pipe floor"
    );
}

#[test]
fn newreno_multiplicative_decrease_must_be_mss_rounded() {
    let scenario = scenario(
        Kind::NewReno,
        10_500,
        20 * MSS,
        vec![Event {
            at: 10,
            ack: FIRST_SEQ,
            loss: true,
            rtt: None,
        }],
    );
    let outcome = run(&scenario);
    assert_eq!(outcome.ssthresh, 5_250, "K341 model ssthresh");
    assert_eq!(
        donor_newreno_md(10_500, MSS),
        5_000,
        "donor MSS-rounded beta"
    );
    assert_ne!(
        outcome.ssthresh, 5_000,
        "K359-F01b: the donor rounds the multiplicative decrease down to whole MSS"
    );
}

#[test]
fn newreno_congestion_avoidance_misses_the_donor_abc_latch() {
    let model_increment = (u64::from(MSS) * u64::from(MSS) / 20_000).max(1);
    assert_eq!(model_increment, 50, "K341 no-ABC increment");
    assert_eq!(
        donor_newreno_cong_avoid(20_000, MSS, u32::MAX),
        20_050,
        "donor no-ABC path"
    );
    // The donor default has RFC 3465 ABC on: an ACK either adds one MSS or nothing,
    // depending on the CCF_ABC_SENTAWND latch; neither is the 50-byte no-ABC value.
    for abc_increment in [0u32, MSS] {
        assert_ne!(
            abc_increment, model_increment as u32,
            "K359-F02: the model has no per-ACK ABC latch"
        );
    }
}

#[test]
fn cubic_loss_uses_cwnd_instead_of_pipe_and_skips_first_fast_convergence() {
    let scenario = scenario(Kind::Cubic, 10 * MSS, 80 * MSS, loss_then_full_ack());
    let outcome = run(&scenario);
    assert_eq!(outcome.cwnd, 6_992, "K341 model cwnd");
    assert_eq!(outcome.ssthresh, 6_992, "K341 model ssthresh");
    assert_eq!(outcome.w_max, 10_000, "K341 model W_max");

    let donor_ssthresh = donor_cubic_get_ssthresh(10 * MSS, u32::MAX, 8_000, false, MSS);
    let donor_cwnd = donor_newreno_post_recovery(0, donor_ssthresh, MSS);
    let donor_w_max = u32::try_from(u64::from(10 * MSS) * 217 / 256).expect("W_max");
    assert_eq!(donor_w_max, 8_476, "donor fast convergence W_max");
    assert_eq!(donor_ssthresh, 5_593, "donor pipe*179/256");
    assert_eq!(donor_cwnd, 2_000, "donor post-recovery floor");
    assert_ne!(
        outcome.ssthresh, donor_ssthresh,
        "K359-F03: the donor derives ssthresh from the pipe, not the cwnd"
    );
    assert_ne!(
        outcome.cwnd, donor_cwnd,
        "K359-F03: the donor reduces cwnd only at post-recovery"
    );
    assert_ne!(
        outcome.w_max, donor_w_max,
        "K359-F03: the donor applies fast convergence on the first loss (W_max=UINT_MAX)"
    );
}

#[test]
fn cubic_w_est_and_target_cap_match_rfc_9438_not_the_model() {
    let donor_est = donor_cubic_w_est(10.0, 1.1, 0.1);
    let model_est: f64 = 0.7 * 10.0 + 1.1;
    assert!(
        (donor_est - 12.823_529_411_764_707).abs() < 1e-9,
        "{donor_est}"
    );
    assert!((model_est - 8.1).abs() < 1e-9, "{model_est}");
    assert!(
        (donor_est - model_est).abs() > 4.0,
        "K359-F04: the model's Reno-friendly term is 0.7*W_max + t, not 0.7*W_max + alpha*(t/RTT)"
    );

    let w_cubic = 5.0;
    let cwnd = 2.0;
    assert_eq!(donor_cubic_target(w_cubic, cwnd), 3.0);
    assert!(
        w_cubic > donor_cubic_target(w_cubic, cwnd),
        "K359-F05: the donor caps the CUBIC target at 1.5*cwnd"
    );

    let k = donor_cubic_k(10.0, 5.0);
    assert!((k - 2.320_794_416_806_389).abs() < 1e-9, "{k}");
    let w = donor_cubic_w_cubic(1.1, 10.0, 5.0);
    assert!((w - 9.272_240_983_902_34).abs() < 1e-9, "{w}");
}

#[test]
fn rack_loss_does_not_reduce_the_window_but_the_donor_does() {
    let scenario = scenario(Kind::Rack, 10 * MSS, 20 * MSS, loss_then_full_ack());
    let outcome = run(&scenario);
    assert_eq!(outcome.cwnd, 20_000, "K341 model cwnd");
    assert_eq!(outcome.ssthresh, 20_000, "K341 model ssthresh");
    assert!(!outcome.in_recovery);

    let donor_ssthresh = donor_newreno_md(10 * MSS, MSS);
    let donor_cwnd = donor_newreno_post_recovery(0, donor_ssthresh, MSS);
    assert_eq!(donor_ssthresh, 5_000);
    assert_eq!(donor_cwnd, 2_000);
    assert_ne!(
        outcome.cwnd, donor_cwnd,
        "K359-F06: rack_cong_signal(CC_NDUPACK) must perform the multiplicative decrease"
    );
}

#[test]
fn rack_pipe_misses_the_sacked_and_holes_terms_and_the_donor_floor() {
    let scenario = scenario(Kind::Rack, 10 * MSS, 20 * MSS, loss_then_full_ack());
    let outcome = run(&scenario);
    assert_eq!(outcome.pipe, MSS, "K341 model pipe floors at one MSS");
    assert_eq!(
        donor_rack_pipe(9_000, 9_000, 0, 0, 0),
        0,
        "donor pipe has no one-MSS floor"
    );
    assert_ne!(
        outcome.pipe,
        donor_rack_pipe(9_000, 9_000, 0, 0, 0),
        "K359-F07: the one-MSS floor is not in do_rack_compute_pipe"
    );

    let donor_with_sacked = donor_rack_pipe(9_000, 1_000, 2_000, 1_000, 0);
    let model_formula = (9_000u32 - 1_000 - 1_000).max(MSS);
    assert_eq!(donor_with_sacked, 5_000);
    assert_eq!(model_formula, 7_000);
    assert_ne!(
        donor_with_sacked, model_formula,
        "K359-F07: the model omits rc_sacked and rc_holes_rxt"
    );
}

#[test]
fn rack_retransmit_selection_can_select_the_same_range_twice() {
    let scenario = scenario(
        Kind::Rack,
        10 * MSS,
        20 * MSS,
        vec![
            Event {
                at: 10,
                ack: FIRST_SEQ,
                loss: true,
                rtt: None,
            },
            Event {
                at: 20,
                ack: FIRST_SEQ,
                loss: true,
                rtt: None,
            },
        ],
    );
    let outcome = run(&scenario);
    assert_eq!(outcome.retransmits.len(), 2);
    assert_eq!(
        outcome.retransmits[0], outcome.retransmits[1],
        "K359-F08: the model re-selects the same lost range without a new transmission"
    );
}
