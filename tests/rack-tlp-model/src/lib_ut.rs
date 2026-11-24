// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference vectors, property traces and current-primitive comparisons.

use raven_tcpip::{Rack, SpuriousUndo, Tlp};

use crate::model::{Action, Config, RackModel};
use crate::rng::Rng;

fn config(srtt: u32, rttvar: u32, rto: u32) -> Config {
    Config {
        srtt_ms: srtt,
        rttvar_ms: rttvar,
        rto_ms: rto,
        max_probes: 2,
        delayed_ack_ms: 40,
    }
}

#[test]
fn tlp_pto_and_threshold_follow_rfc8985_and_the_donor_cap() {
    let model = RackModel::new(config(100, 10, 1_000), 10, 4);
    assert_eq!(model.pto(), 142, "SRTT 100 + 4*10 + 2");
    assert_eq!(model.tlp_threshold(), 190, "SRTT + SRTT/2 + delayed ACK");
    let capped = RackModel::new(config(100, 10, 150), 10, 4);
    assert_eq!(capped.pto(), 142);
    assert_eq!(capped.tlp_threshold(), 150, "capped by RTO");
}

#[test]
fn tlp_arms_fires_twice_and_disarms_on_ack() {
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    assert_eq!(model.on_sent(1_000, 2_000, 1_000), Action::Arm(1_142));
    assert_eq!(model.poll(1_141), Action::Noop);
    assert_eq!(
        model.poll(1_142),
        Action::Fire {
            seq: 1_000,
            probe: 1
        }
    );
    assert_eq!(model.tlp_deadline, Some(1_284));
    assert_eq!(
        model.poll(1_284),
        Action::Fire {
            seq: 1_000,
            probe: 2
        }
    );
    assert_eq!(model.poll(1_426), Action::Noop, "two probes per flight");
    assert_eq!(model.tlp_deadline, None);
    assert_eq!(model.on_ack(2_000, 1_500), Action::Disarm);
    assert_eq!(model.probes, 0);
}

#[test]
fn dsack_grows_reo_wnd_once_per_round_and_ignores_tlp_dsacks() {
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    let first = model.on_dsack(100, 200);
    assert!(!first.was_tlp);
    assert_eq!(first.grow, Some(Action::GrowReoWnd { from: 0, to: 25 }));
    assert_eq!(model.on_dsack(100, 200).grow, None, "same round");
    model.new_dsack_round();
    assert_eq!(
        model.on_dsack(100, 200).grow,
        Some(Action::GrowReoWnd { from: 25, to: 50 })
    );
    model.new_dsack_round();
    assert_eq!(
        model.on_dsack(100, 200).grow,
        Some(Action::GrowReoWnd { from: 50, to: 75 })
    );
    model.new_dsack_round();
    assert_eq!(
        model.on_dsack(100, 200).grow,
        Some(Action::GrowReoWnd { from: 75, to: 100 })
    );
    model.new_dsack_round();
    assert_eq!(
        model.on_dsack(100, 200).grow,
        Some(Action::GrowReoWnd { from: 100, to: 100 })
    );

    let mut tlp = RackModel::new(config(100, 10, 1_000), 10, 4);
    tlp.last_tlp = Some((300, 400));
    let outcome = tlp.on_dsack(310, 390);
    assert!(outcome.was_tlp);
    assert_eq!(
        outcome.grow, None,
        "a TLP-caused DSACK does not grow reo_wnd"
    );
}

#[test]
fn dsack_spurious_signal_restores_congestion_state_once() {
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    model.enter_recovery(1_000);
    model.note_retransmit(500);
    assert_eq!(model.on_ack(1_000, 1_100), Action::Disarm);
    assert_eq!(model.recovery, None);
    let outcome = model.on_dsack(100, 600);
    assert_eq!(
        outcome.undo,
        Some(Action::Undo {
            cwnd: 10,
            ssthresh: 4
        })
    );
    assert_eq!(model.undo_count, 1);
    assert_eq!(model.cwnd, 10);
    assert_eq!(model.ssthresh, 4);
    assert_eq!(model.on_dsack(100, 600).undo, None, "restore only once");

    let mut eifel = RackModel::new(config(100, 10, 1_000), 20, 8);
    eifel.enter_recovery(2_000);
    assert_eq!(
        eifel.on_eifel(1_999, 2_000),
        Some(Action::Undo {
            cwnd: 20,
            ssthresh: 8
        })
    );
    assert_eq!(eifel.on_eifel(1_999, 2_000), None);
    assert_eq!(eifel.undo_count, 1);
}

#[test]
fn reneging_clears_sacked_ranges_and_allows_loss_again() {
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    model.on_sent(100, 200, 0);
    model.on_sack(100, 200);
    model.on_sack(300, 400);
    assert_eq!(model.sacked, [(100, 200), (300, 400)]);
    assert!(model.is_sacked(150));
    assert_eq!(model.on_renege(100, 200), Action::ClearSacked { ranges: 1 });
    assert!(!model.is_sacked(150));
    assert!(model.is_sacked(350));
    assert_eq!(model.reneged_bytes, 100);
    assert!(
        model.is_lost(150, 100),
        "the range can be declared lost again"
    );
}

#[test]
fn duplicate_sack_merges_without_changing_the_byte_count() {
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    model.on_sack(100, 200);
    model.on_sack(150, 250);
    assert_eq!(model.sacked, [(100, 250)]);
    assert_eq!(model.sacked_bytes(), 150);
    model.on_sack(100, 250);
    assert_eq!(model.sacked_bytes(), 150);
}

#[test]
fn model_matches_the_current_product_primitives() {
    let mut product_rack = Rack::new(0);
    product_rack.on_dsack(100);
    let mut model = RackModel::new(config(100, 10, 1_000), 10, 4);
    let outcome = model.on_dsack(0, 100);
    assert_eq!(
        outcome.grow,
        Some(Action::GrowReoWnd {
            from: 0,
            to: product_rack.reorder_window
        })
    );
    assert_eq!(product_rack.reorder_window, 25);

    let mut product_tlp = Tlp::default();
    product_tlp.arm(1_000, 100, 10);
    let model_tlp = RackModel::new(config(100, 10, 1_000), 10, 4);
    assert_eq!(product_tlp.deadline, Some(1_142));
    assert_eq!(model_tlp.pto(), 142);
    assert!(product_tlp.fires(1_142));

    let mut product_undo = SpuriousUndo::record(10, 4);
    assert_eq!(product_undo.restore(), Some((10, 4)));
    assert_eq!(product_undo.restore(), None);
    let mut model_undo = RackModel::new(config(100, 10, 1_000), 10, 4);
    model_undo.enter_recovery(1_000);
    assert_eq!(
        model_undo.restore_undo(),
        Some(Action::Undo {
            cwnd: 10,
            ssthresh: 4
        })
    );
    assert_eq!(model_undo.restore_undo(), None);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TraceStep {
    now: u32,
    event: u8,
    action: Action,
    deadline: Option<u32>,
    reo_wnd: u32,
}

fn run_trace(seed: u64) -> Vec<TraceStep> {
    let mut rng = Rng::new(seed);
    let srtt = rng.range(1, 500);
    let rttvar = rng.range(0, 100);
    let rto = rng.range(100, 3_000);
    let mut model = RackModel::new(config(srtt, rttvar, rto), 10, 4);
    let mut next_seq = 1_000u32;
    let mut now = 0u32;
    let mut trace = Vec::new();
    let mut recoveries = 0usize;
    for _ in 0..2_000 {
        now = now.saturating_add(rng.range(1, 50));
        let event = u8::try_from(rng.below(10)).unwrap_or(0);
        let action = match event {
            0..=2 => {
                let length = rng.range(1, 900);
                let start = next_seq;
                let end = start.saturating_add(length);
                next_seq = end;
                model.on_sent(start, end, now)
            }
            3 => {
                let span = model.snd_max.saturating_sub(model.snd_una);
                let ack = model
                    .snd_una
                    .saturating_add(rng.below(span.saturating_add(1)));
                model.on_ack(ack, now)
            }
            4 => {
                let start = model.snd_una.saturating_add(rng.below(20_000));
                let end = start.saturating_add(rng.range(1, 900));
                model.on_sack(start, end);
                Action::Noop
            }
            5 => {
                let start = model.snd_una.saturating_add(rng.below(20_000));
                let end = start.saturating_add(rng.range(1, 900));
                let outcome = model.on_dsack(start, end);
                if rng.below(3) == 0 {
                    model.new_dsack_round();
                }
                outcome.undo.or(outcome.grow).unwrap_or(Action::Noop)
            }
            6 => {
                let start = model.snd_una.saturating_add(rng.below(20_000));
                let end = start.saturating_add(rng.range(1, 900));
                model.on_renege(start, end)
            }
            7 => model.poll(now),
            8 => {
                model.enter_recovery(model.snd_max);
                model.note_retransmit(rng.range(1, 2_000));
                recoveries = recoveries.saturating_add(1);
                Action::Noop
            }
            _ => model
                .on_eifel(now.saturating_sub(1), now)
                .unwrap_or(Action::Noop),
        };
        assert!(model.snd_una <= model.snd_max);
        assert!(model.reo_wnd_ms <= srtt.max(1));
        assert!(model.probes <= model.config.max_probes);
        assert!(model.undo_count <= recoveries);
        for pair in model.sacked.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "SACK ranges overlap");
        }
        assert!(
            model.sacked.iter().all(|(start, end)| start < end),
            "empty SACK range"
        );
        trace.push(TraceStep {
            now,
            event,
            action,
            deadline: model.tlp_deadline,
            reo_wnd: model.reo_wnd_ms,
        });
    }
    trace
}

#[test]
fn randomized_timers_and_sack_traces_are_deterministic_and_invariant() {
    for seed in [
        0x3200_0001,
        0x3200_0002,
        0x3200_0003,
        0x3200_0004,
        0x3200_0005,
    ] {
        let first = run_trace(seed);
        let second = run_trace(seed);
        assert_eq!(first, second, "seed {seed:#x} is deterministic");
        assert_eq!(first.len(), 2_000);
    }
}
