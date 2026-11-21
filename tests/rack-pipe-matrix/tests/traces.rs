// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent trace oracles for the K333 pipe/lost surface.
//!
//! K319 (loss/reordering), K320 (TLP threshold) and K324 (recovery/pipe) are applicable to this
//! group; K326 (BBR/pacing) and K327 (vtable) do not exercise the pipe/lost arithmetic and are
//! not used here. The checked-in 10k traces are regenerated in memory and compared byte for byte,
//! then the shared arithmetic is compared against the model functions on the same inputs.

use std::fs;
use std::path::{Path, PathBuf};

use rack_loss_model::{Scenario as LossScenario, TRACE_COUNT, run, trace_row};
use rack_recovery_model::{
    FIRST_SEQ, MSS, RecoveryModel, SEGMENTS, Scenario as RecoveryScenario,
    TRACE_COUNT as RECOVERY_TRACE_COUNT, generate, header,
};
use rack_tlp_model::model::{Config as TlpConfig, RackModel};
use raven_tcpip::{RACK_REORDER_FADE_DEFAULT, RackPipe, RackSendmap, TLP_USE_TWO_ONE};

const K319_HEADER: &str = "# RACK loss/reordering reference traces (K319)\n# seed\tevents\tlost\tpipe\tretransmits\trecovery_entries/exits\tevent_digest\n";

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn entry(start: u32, end: u32, last: u32) -> RackSendmap {
    RackSendmap::new(start, end, last.into())
}

#[test]
fn k319_loss_trace_regenerates_exactly() {
    let mut text = String::from(K319_HEADER);
    for seed in 1..=TRACE_COUNT {
        let scenario = LossScenario::generate(seed);
        let outcome = run(&scenario);
        text.push_str(&trace_row(&scenario, &outcome));
        text.push('\n');
    }
    let expected = fs::read_to_string(workspace_path("rack-loss-model/traces/rack-10k.tsv"))
        .expect("K319 trace file");
    assert_eq!(
        text, expected,
        "the checked-in K319 loss/reordering oracle drifted"
    );
    assert_eq!(
        expected.lines().count(),
        usize::try_from(TRACE_COUNT).expect("count") + 2,
        "the K319 trace keeps its header plus one row per seed"
    );
}

#[test]
fn k324_recovery_trace_regenerates_exactly() {
    let mut text = header();
    for row in generate(RECOVERY_TRACE_COUNT) {
        text.push_str(&row);
        text.push('\n');
    }
    let expected = fs::read_to_string(workspace_path(
        "rack-recovery-model/traces/rack-recovery-10k.tsv",
    ))
    .expect("K324 trace file");
    assert_eq!(
        text, expected,
        "the checked-in K324 recovery/pipe oracle drifted"
    );
    assert_eq!(
        expected.lines().count(),
        usize::try_from(RECOVERY_TRACE_COUNT).expect("count") + 2,
        "the K324 trace keeps its header plus one row per seed"
    );
}

#[test]
fn k319_reorder_window_matches_rack_calc_thresh_rack() {
    for seed in 1..=1_000 {
        let scenario = LossScenario::generate(seed);
        let config = scenario.config;
        for dsacks in 0..=3 {
            let mut pipe = RackPipe::new();
            pipe.rack_rtt = config.srtt;
            pipe.state.pkt_delay = config.pkt_delay;
            pipe.state.reorder_shift =
                u8::try_from(config.reorder_shift).expect("reorder shift fits u8");
            pipe.state.use_dsack = config.use_dsack;
            pipe.state.rto_max = config.rto_max;
            pipe.state.reorder_ts = 1;
            pipe.state.reorder_fade = RACK_REORDER_FADE_DEFAULT;
            pipe.state.tmr_std_based = false;
            pipe.stack.num_dsacks = dsacks;

            let expected = rack_loss_model::reo_wnd(config, config.srtt, true, dsacks);
            assert_eq!(
                pipe.rack_calc_thresh_rack(config.srtt, 1),
                expected,
                "seed {seed}, dsacks {dsacks}"
            );
        }
    }
}

#[test]
fn k320_tlp_threshold_matches_rack_calc_thresh_tlp() {
    for srtt in [1, 10, 50, 100, 250, 1_000, 50_000] {
        for rto in [1, 40, 100, 200, 1_000, 100_000] {
            let model = RackModel::new(
                TlpConfig {
                    srtt_ms: srtt,
                    rttvar_ms: 10,
                    rto_ms: rto,
                    max_probes: 2,
                    delayed_ack_ms: 40,
                },
                10,
                4,
            );
            let expected = model.tlp_threshold();

            let mut pipe = RackPipe::new();
            pipe.stack.tp.t_maxseg = 1_000;
            pipe.stack.pace_min_segs = 1_000;
            pipe.stack.tp.t_rxtcur = u32::MAX;
            pipe.state.tlp_threshold = 2;
            pipe.state.tlp_threshold_use = TLP_USE_TWO_ONE;
            pipe.state.tlp_min = 1;
            pipe.state.rto_max = rto;
            pipe.state.delayed_ack_time = 40;
            pipe.push_sendmap(entry(1_000, 2_000, 0));

            assert_eq!(
                pipe.rack_calc_thresh_tlp(0, srtt),
                expected,
                "srtt {srtt}, rto {rto}"
            );
        }
    }
}

#[test]
fn k324_recovery_pipe_matches_rack_pipe_on_model_states() {
    for seed in 1..=200 {
        let scenario = RecoveryScenario::new(seed);
        let outcome = scenario.run();

        let mut model = RecoveryModel::new(TlpConfig::default());
        for index in 0..SEGMENTS {
            let start = FIRST_SEQ + index as u32 * MSS;
            model.send(start, start + MSS, index as u32 * 5);
        }
        for event in &scenario.events {
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

        assert_eq!(model.pipe(), outcome.pipe, "seed {seed} model pipe");

        let snd_una = model.inner.snd_una;
        let snd_max = model.inner.snd_max;
        let outstanding = snd_max.saturating_sub(snd_una);
        // The donor `rc_sacked` counts SACKed bytes above `snd_una`. The K324 model keeps
        // ranges whose tail extends past `snd_una`, so clip every retained range before mapping
        // it to the product state.
        let sacked = model
            .inner
            .sacked
            .iter()
            .map(|(start, end)| {
                let low = (*start).max(snd_una);
                let high = (*end).min(snd_max);
                high.saturating_sub(low)
            })
            .sum::<u32>();
        let lost = outcome
            .lost
            .iter()
            .map(|(start, end)| {
                let low = (*start).max(snd_una);
                let high = (*end).min(snd_max);
                high.saturating_sub(low)
            })
            .sum::<u32>();
        let retransmitted = outcome
            .retransmits
            .iter()
            .map(|(start, end)| {
                let low = (*start).max(snd_una);
                let high = (*end).min(snd_max);
                high.saturating_sub(low)
            })
            .sum::<u32>();
        assert!(
            sacked.saturating_add(lost) <= outstanding,
            "seed {seed}: the mapped state must be a valid sender state"
        );

        let mut pipe = RackPipe::new();
        pipe.stack.tp.t_state = 4;
        pipe.stack.tp.snd_una = snd_una;
        pipe.stack.tp.snd_max = snd_max;
        pipe.stack.sacked = sacked;
        pipe.stack.considered_lost = lost;
        pipe.stack.holes_rxt = retransmitted;

        assert_eq!(
            pipe.rack_compute_pipe() as u32,
            outcome.pipe,
            "seed {seed} RackPipe formula"
        );
    }
}
