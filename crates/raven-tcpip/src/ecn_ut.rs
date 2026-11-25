// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the FreeBSD ECN port.

use super::*;

#[test]
fn rfc3168_syn_ack_requires_ece_without_cwr() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(2, 1);
    ecn.input_syn_sent(TH_ECE | TH_CWR, 0, &mut counters);
    assert!(!ecn.permit);
    ecn.input_syn_sent(TH_ECE, 0, &mut counters);
    assert!(ecn.permit);
    assert_eq!(counters.handshakes, 1);
}

#[test]
fn disabled_ecn_does_nothing() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(0, 1);
    ecn.input_syn_sent(TH_ECE, 0, &mut counters);
    assert!(!ecn.permit);
    assert_eq!(ecn.output_syn_sent(0), 0);
}

#[test]
fn rfc3168_syn_output_uses_the_retry_limit() {
    let ecn = EcnState::new(1, 1);
    assert_eq!(ecn.output_syn_sent(0), TH_ECE | TH_CWR);
    assert_eq!(ecn.output_syn_sent(1), TH_ECE | TH_CWR);
    assert_eq!(ecn.output_syn_sent(2), 0);
}

#[test]
fn accurate_syn_output_sets_ae() {
    let ecn = EcnState::new(3, 1);
    assert_eq!(ecn.output_syn_sent(0), TH_ECE | TH_CWR | TH_AE);
}

#[test]
fn accurate_syn_ack_sets_the_receive_codepoint() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_syn_sent(TH_AE, ECN_ECT0, &mut counters);
    assert!(ecn.ace_permit);
    assert_eq!(ecn.scep, 5);
    assert_eq!(ecn.rcep, 0b100);
}

#[test]
fn ce_queues_ece_and_cwr_clears_it() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(2, 1);
    ecn.permit = true;
    ecn.input_segment(0, 100, 1, ECN_CE, &mut counters);
    assert!(ecn.snd_ece);
    assert_eq!(counters.received_ce, 1);
    ecn.input_segment(TH_CWR, 0, 1, 0, &mut counters);
    assert!(!ecn.snd_ece);
}

#[test]
fn established_output_marks_only_new_data() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(2, 1);
    ecn.permit = true;
    let mut flags = 0;
    assert_eq!(
        ecn.output_established(&mut flags, 100, 100, 100, false, false, &mut counters),
        ECN_ECT0
    );
    assert_eq!(
        ecn.output_established(&mut flags, 100, 100, 100, true, false, &mut counters),
        ECN_NOTECT
    );
    assert_eq!(
        ecn.output_established(&mut flags, 0, 100, 100, false, false, &mut counters),
        ECN_NOTECT
    );
    assert_eq!(
        ecn.output_established(&mut flags, 1, 100, 100, false, true, &mut counters),
        ECN_NOTECT
    );
}

#[test]
fn cwr_is_consumed_and_ece_persists() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(2, 1);
    ecn.permit = true;
    ecn.snd_cwr = true;
    ecn.snd_ece = true;
    let mut flags = 0;
    ecn.output_established(&mut flags, 100, 100, 100, false, false, &mut counters);
    assert!(flags & TH_CWR != 0);
    assert!(flags & TH_ECE != 0);
    let mut flags = 0;
    ecn.output_established(&mut flags, 100, 100, 100, false, false, &mut counters);
    assert!(flags & TH_CWR == 0);
    assert!(flags & TH_ECE != 0);
}

#[test]
fn ace_counter_round_trip() {
    let mut flags = 0;
    set_ace(&mut flags, 6);
    assert_eq!(get_ace(flags), 6);
    set_ace(&mut flags, 2);
    assert_eq!(get_ace(flags), 2);
}

#[test]
fn accurate_final_ack_selects_the_send_counter() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.ace_permit = true;
    ecn.rcep = 0b100;
    // ACE 0b110 (CE SYN or SYN,ACK) sets t_scep to 6.
    ecn.input_segment(TH_AE | TH_CWR, 0, 1, ECN_ECT0, &mut counters);
    assert_eq!(ecn.scep, 6);
    assert!(ecn.permit);

    let mut ecn = EcnState::new(3, 1);
    ecn.ace_permit = true;
    // ACE 0b011 (ECT1 SYN) sets t_scep to 5.
    ecn.input_segment(TH_CWR | TH_ECE, 0, 1, ECN_ECT1, &mut counters);
    assert_eq!(ecn.scep, 5);
}

#[test]
fn parallel_syn_marks_ece_for_rfc3168() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(2, 1);
    ecn.input_parallel_syn(TH_SYN | TH_CWR | TH_ECE, 0, &mut counters);
    assert!(ecn.permit);
    assert!(ecn.snd_ece);
    assert_eq!(counters.handshakes, 1);

    let mut ecn = EcnState::new(2, 1);
    ecn.input_parallel_syn(TH_ACK | TH_CWR | TH_ECE, 0, &mut counters);
    assert!(!ecn.permit);
}

#[test]
fn parallel_syn_accurate_sets_the_receive_codepoint() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_parallel_syn(TH_SYN | TH_AE | TH_CWR | TH_ECE, ECN_ECT0, &mut counters);
    assert!(ecn.ace_permit);
    assert!(ecn.snd_ece);
    assert_eq!(ecn.rcep, 0b100);
}

#[test]
fn modes_without_a_syn_option_emit_no_flags() {
    assert_eq!(EcnState::new(2, 1).output_syn_sent(0), 0);
    assert_eq!(EcnState::new(4, 1).output_syn_sent(0), 0);
}

#[test]
fn accurate_syn_counters_follow_the_donor_table() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_syn_sent(TH_CWR, 0, &mut counters);
    assert_eq!(counters.ace_nect, 1);
    assert_eq!(counters.handshakes, 1);

    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_syn_sent(TH_AE, 0, &mut counters);
    assert_eq!(counters.ace_ect0, 1);

    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_syn_sent(TH_CWR | TH_ECE, 0, &mut counters);
    assert_eq!(counters.ace_ect1, 1);

    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(3, 1);
    ecn.input_syn_sent(TH_AE | TH_CWR, 0, &mut counters);
    assert_eq!(counters.ace_nect, 1);
}
