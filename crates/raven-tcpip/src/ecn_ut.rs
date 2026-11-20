// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the NetBSD ECN port.

use super::*;

#[test]
fn syn_ecn_uses_the_retry_budget() {
    let mut ecn = EcnState::new(true);
    assert!(ecn.on_syn_sent(false, 1));
    assert_eq!(ecn.retries, 1);
    assert!(ecn.on_syn_sent(true, 1));
    assert_eq!(ecn.retries, 0);
    assert!(!ecn.on_syn_sent(true, 1));
}

#[test]
fn disabled_ecn_never_marks_a_syn() {
    let mut ecn = EcnState::new(false);
    assert!(!ecn.on_syn_sent(false, 1));
    assert!(!ecn.on_syn_sent(true, 1));
}

#[test]
fn syn_ack_requires_both_sides() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    assert!(!ecn.on_syn_ack(false, true, &mut counters));
    assert!(!ecn.on_syn_ack(true, false, &mut counters));
    assert!(ecn.on_syn_ack(true, true, &mut counters));
    assert_eq!(counters.handshakes, 1);
    assert!(ecn.permit);
}

#[test]
fn ce_queues_ece_and_cwr_clears_it() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    ecn.permit = true;
    assert!(!ecn.on_segment(false, false, ECN_CE, 100, 100, &mut counters));
    assert!(ecn.snd_ece);
    assert_eq!(counters.ce, 1);
    ecn.on_segment(true, false, 0, 100, 100, &mut counters);
    assert!(!ecn.snd_ece);
}

#[test]
fn ece_is_a_congestion_event_only_at_or_after_recovery() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    ecn.permit = true;
    assert!(ecn.on_segment(false, true, 0, 100, 100, &mut counters));
    assert!(!ecn.on_segment(false, true, 0, 99, 100, &mut counters));
}

#[test]
fn ect0_marks_only_fresh_data() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    ecn.permit = true;
    let fresh = ecn.on_output(1000, 100, 100, false, &mut counters);
    assert!(fresh.ect0);
    assert_eq!(counters.ect, 1);
    let retransmit = ecn.on_output(1000, 99, 100, false, &mut counters);
    assert!(!retransmit.ect0);
    let ack = ecn.on_output(0, 100, 100, false, &mut counters);
    assert!(!ack.ect0);
    let probe = ecn.on_output(1, 100, 100, true, &mut counters);
    assert!(!probe.ect0);
}

#[test]
fn cwr_is_consumed_and_ece_persists() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    ecn.permit = true;
    ecn.snd_cwr = true;
    ecn.snd_ece = true;
    let first = ecn.on_output(100, 10, 10, false, &mut counters);
    assert!(first.cwr);
    assert!(first.ece);
    assert!(!ecn.snd_cwr);
    let second = ecn.on_output(100, 20, 20, false, &mut counters);
    assert!(!second.cwr);
    assert!(second.ece);
}

#[test]
fn unpermitted_connection_ignores_input_and_output() {
    let mut counters = EcnCounters::default();
    let mut ecn = EcnState::new(true);
    assert!(!ecn.on_segment(false, true, ECN_CE, 0, 0, &mut counters));
    let out = ecn.on_output(100, 1, 1, false, &mut counters);
    assert_eq!(out, EcnOutput::default());
    assert_eq!(counters.ce, 0);
}
