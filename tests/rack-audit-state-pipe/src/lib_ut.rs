// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the audit: live findings plus injected negative fixtures.

use super::*;

fn with_state(state: &str) -> Sources<'_> {
    let live = live_sources();
    Sources {
        state,
        pipe: live.pipe,
        misc: live.misc,
        retransmit: live.retransmit,
        bbr: live.bbr,
        output: live.output,
        sack: live.sack,
        state_ut: live.state_ut,
        pipe_ut: live.pipe_ut,
    }
}

#[test]
fn the_live_audit_covers_every_assigned_function_and_test() {
    let report = audit();
    assert_eq!(
        report.functions_checked, 28,
        "18 K332 + 9 K333 + rack_grab_rtt"
    );
    assert!(report.of_kind(Kind::MissingFunction).is_empty());
    assert!(report.of_kind(Kind::MissingTest).is_empty());
    assert!(report.constants_checked > 100);
}

#[test]
fn the_live_audit_finds_the_conflicting_state_and_flag_values() {
    let report = audit();
    for id in [
        "wrong:TCPS_FIN_WAIT_2",
        "wrong:TCPS_CLOSING",
        "wrong:TCPS_LAST_ACK",
        "wrong:RACK_SACK_PASSED",
        "wrong:RACK_WAS_SACKPASS",
        "wrong:RACK_WAS_LOST",
        "wrong:RACK_RWND_COLLAPSED",
        "wrong:PACE_MAX_IP_BYTES",
    ] {
        assert!(report.has(id), "missing audit finding {id}");
    }
    assert_eq!(
        report
            .of_kind(Kind::WrongConstant)
            .iter()
            .filter(|finding| finding.location.starts_with("rack_output.rs"))
            .count(),
        3,
        "rack_output.rs has three wrong TCPS values"
    );
    assert_eq!(
        report
            .of_kind(Kind::WrongConstant)
            .iter()
            .filter(|finding| finding.location.starts_with("rack_sack.rs"))
            .count(),
        4,
        "rack_sack.rs has four wrong RACK flag values"
    );
}

#[test]
fn the_live_audit_finds_duplicate_constants_and_the_duplicate_sendmap() {
    let report = audit();
    for id in [
        "duplicate:RACK_ACKED",
        "duplicate:RACK_SACK_PASSED",
        "duplicate:TF_FASTRECOVERY",
        "duplicate:RACK_RTO_MAX",
        "duplicate:RACK_INITIAL_RTO",
        "duplicate-sendmap",
    ] {
        assert!(report.has(id), "missing audit finding {id}");
    }
    let acked = report
        .findings
        .iter()
        .find(|finding| finding.id == "duplicate:RACK_ACKED")
        .expect("RACK_ACKED duplicate");
    assert_eq!(
        acked.location,
        "rack_misc.rs,rack_retransmit.rs,rack_sack.rs"
    );
}

#[test]
fn the_live_audit_flags_the_shared_test_and_non_donor_entry() {
    let report = audit();
    assert!(
        report.has("shared-test:do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes")
    );
    assert!(report.has("non-donor:rack_enter_recovery"));
}

#[test]
fn the_live_audit_reproduces_the_tlp_gap_divergence() {
    let report = audit();
    assert_eq!(
        tlp_gap_vector(),
        (6_000, 4_000),
        "donor gap 4000 + srtt/1 2000 = 6000; merged uses the wrong indices"
    );
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "tlp-gap-index")
        .expect("TLP index finding");
    assert_eq!(finding.kind, Kind::BehaviourDivergence);
    assert!(finding.detail.contains("donor 6000, merged 4000"));
}

#[test]
fn the_live_audit_reproduces_the_fin_wait_gate_gap() {
    let report = audit();
    assert_eq!(
        fin_wait_vector(),
        (6, 9),
        "donor stays in FIN_WAIT_1 without TF_SENTFIN; merged moves to FIN_WAIT_2"
    );
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "fin-wait-without-fin")
        .expect("FIN_WAIT_1 gate finding");
    assert_eq!(finding.kind, Kind::BehaviourDivergence);
}

#[test]
fn a_missing_function_is_detected() {
    let broken = live_sources().state.replacen(
        "pub fn rack_do_syn_sent(",
        "fn removed_rack_do_syn_sent(",
        1,
    );
    let report = audit_sources(&with_state(&broken));
    assert!(report.has("missing-function:rack_do_syn_sent"));
}

#[test]
fn a_missing_test_is_detected() {
    let live = live_sources();
    let broken = live.state_ut.replacen(
        "fn rack_do_syn_sent_establishes_on_syn_ack(",
        "fn removed_test(",
        1,
    );
    let sources = Sources {
        state: live.state,
        pipe: live.pipe,
        misc: live.misc,
        retransmit: live.retransmit,
        bbr: live.bbr,
        output: live.output,
        sack: live.sack,
        state_ut: &broken,
        pipe_ut: live.pipe_ut,
    };
    let report = audit_sources(&sources);
    assert!(report.has("missing-test:rack_do_syn_sent"));
}

#[test]
fn a_wrong_constant_is_detected() {
    let broken =
        live_sources()
            .state
            .replacen("TCPS_FIN_WAIT_2: u8 = 9", "TCPS_FIN_WAIT_2: u8 = 7", 1);
    let report = audit_sources(&with_state(&broken));
    assert!(report.has("wrong:TCPS_FIN_WAIT_2"));
}

#[test]
fn a_duplicate_constant_is_detected() {
    let mut broken = String::from(live_sources().state);
    broken.push_str("\npub const RACK_ACKED: u32 = 1;\n");
    let report = audit_sources(&with_state(&broken));
    assert!(report.has("duplicate:RACK_ACKED"));
}
