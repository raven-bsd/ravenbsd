// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One named test per donor output/timer/state function (K330).

use super::*;

fn running() -> RackOutput {
    let mut out = RackOutput::new();
    out.note_receive(0);
    out
}

#[test]
fn rack_output_sends_new_data_and_arms_rxt() {
    let mut out = running();
    assert!(out.queue(&alloc::vec![0x5a; 1_000]));
    assert_eq!(out.rack_output(), Ok(()));
    assert_eq!(out.os.tcpcb.snd_nxt, 1_000);
    assert_eq!(out.os.tcpcb.snd_max, 1_000);
    assert_eq!(out.bytes_transmitted, 1_000);
    assert_ne!(out.timers & PACE_TMR_RXT, 0);
    assert!(out.packet_output_armed());
    assert_eq!(out.last_action, "send");
}

#[test]
fn tcp_rack_output_returns_default_ok_and_candrop() {
    let mut out = running();
    assert!(out.queue(&alloc::vec![1u8; 10]));
    assert_eq!(
        out.tcp_rack_output(),
        i32::try_from(TCP_FUNC_DEFAULT_OK).unwrap()
    );
    out.os.tcpcb.t_state = TCPS_CLOSED;
    assert_eq!(
        out.tcp_rack_output(),
        i32::try_from(TCP_FUNC_OUTPUT_CANDROP).unwrap()
    );
}

#[test]
fn rack_output_sends_fin_after_close() {
    let mut out = running();
    out.note_receive(0);
    out.close_requested = true;
    assert_eq!(out.rack_output(), Ok(()));
    assert!(out.fin_sent);
    assert_eq!(out.os.tcpcb.t_flags & TF_SENTFIN, TF_SENTFIN);
    assert_eq!(out.os.tcpcb.snd_max, 1);
    assert_eq!(out.last_action, "fin");
}

#[test]
fn rack_output_retransmits_when_flagged() {
    let mut out = running();
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 100;
    out.must_retran = true;
    assert_eq!(out.rack_output(), Ok(()));
    assert_eq!(out.bytes_retransmitted, 100);
    assert!(!out.must_retran);
    assert_eq!(out.os.tcpcb.snd_nxt, 0);
    assert_eq!(out.last_action, "retransmit");
}

#[test]
fn rack_output_enters_persist_on_zero_window() {
    let mut out = running();
    assert!(out.queue(&alloc::vec![2u8; 100]));
    out.snd_wnd = 0;
    assert_eq!(out.rack_output(), Ok(()));
    assert!(out.persist);
    assert_ne!(out.timers & PACE_TMR_PERSIT, 0);
    assert_eq!(out.last_action, "persist");
}

#[test]
fn rack_timer_start_and_rack_timer_cancel() {
    let mut out = running();
    let slot = out.rack_timer_start(PACE_TMR_RXT, 5_000);
    assert_eq!(slot, 5_000);
    assert_ne!(out.timers & PACE_TMR_RXT, 0);
    assert_eq!(out.rack_timer_cancel(), PACE_TMR_RXT);
    assert_eq!(out.timers & PACE_TMR_RXT, 0);
}

#[test]
fn rack_start_hpts_timer_arms_a_slot() {
    let mut out = running();
    let slot = out.rack_start_hpts_timer(2_000, PACE_TMR_DELACK);
    assert_eq!(slot, 2_000);
    assert_ne!(out.timers & PACE_TMR_DELACK, 0);
    assert_eq!(out.rack_timer_audit(1_999), 0);
    assert_eq!(out.rack_timer_audit(2_000), PACE_TMR_DELACK);
}

#[test]
fn rack_timer_audit_returns_due_bits_only() {
    let mut out = running();
    let _ = out.rack_timer_start(PACE_TMR_TLP, 1_000);
    assert_eq!(out.rack_timer_audit(999), 0);
    assert_eq!(out.rack_timer_audit(1_000), PACE_TMR_TLP);
    assert_eq!(out.rack_timer_audit(1_001), 0);
}

#[test]
fn rack_remxt_tmr_sets_override_and_backoff() {
    let mut out = running();
    let _ = out.rack_timer_start(PACE_TMR_RXT, 1_000);
    let flags = out.rack_remxt_tmr();
    assert_eq!(flags, PACE_TMR_RXT);
    assert_eq!(out.rxtshift, 1);
    assert!(out.must_retran);
    assert_ne!(out.timers & PACE_TMR_RXT, 0);
}

#[test]
fn rack_stopall_refuses_future_timers() {
    let mut out = running();
    let _ = out.rack_timer_start(PACE_TMR_RXT, 1_000);
    assert_eq!(out.rack_stopall(), PACE_TMR_RXT);
    assert_eq!(out.rack_timer_start(PACE_TMR_TLP, 1_000), 0);
    assert_eq!(out.timers, 0);
}

#[test]
fn rack_stop_all_timers_reports_persist() {
    let mut out = running();
    out.snd_wnd = 0;
    assert!(out.queue(&alloc::vec![3u8; 10]));
    let _ = out.rack_output();
    let flags = out.rack_stop_all_timers();
    assert_eq!(flags & PACE_TMR_PERSIT, PACE_TMR_PERSIT);
}

#[test]
fn rack_enter_persist_and_rack_exit_persist() {
    let mut out = running();
    assert!(out.rack_enter_persist());
    assert!(!out.rack_enter_persist());
    assert!(out.persist);
    assert!(out.rack_exit_persist());
    assert!(!out.rack_exit_persist());
    assert!(!out.persist);
}

#[test]
fn rack_timeout_delack_sets_acknow() {
    let mut out = running();
    out.timers = PACE_TMR_DELACK;
    assert!(out.rack_timeout_delack());
    assert_ne!(out.os.tcpcb.t_flags & 0x20, 0);
    assert!(!out.rack_timeout_delack());
}

#[test]
fn rack_timeout_rack_requests_retransmit() {
    let mut out = running();
    out.timers = PACE_TMR_RACK;
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 10;
    assert!(out.rack_timeout_rack());
    assert!(out.must_retran);
    assert!(!out.rack_timeout_rack());
}

#[test]
fn rack_timeout_tlp_is_bounded_to_two_probes() {
    let mut out = running();
    out.timers = PACE_TMR_TLP;
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 100;
    let first = out.rack_timeout_tlp();
    assert_eq!(first.bytes, 1);
    out.timers |= PACE_TMR_TLP;
    let second = out.rack_timeout_tlp();
    assert_eq!(second.bytes, 1);
    out.timers |= PACE_TMR_TLP;
    let third = out.rack_timeout_tlp();
    assert_eq!(third.bytes, 0);
    assert_eq!(out.tlp_probes, 2);
}

#[test]
fn rack_timeout_rxt_backs_off_and_closes_at_the_limit() {
    let mut out = running();
    out.timers = PACE_TMR_RXT;
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 100;
    let result = out.rack_timeout_rxt();
    assert_eq!(result.bytes, 100);
    assert_eq!(out.rxtshift, 1);
    assert!(out.must_retran);
    out.rxtshift = RXT_MAX_SHIFT;
    out.timers = PACE_TMR_RXT;
    let closed = out.rack_timeout_rxt();
    assert!(closed.closed);
    assert_eq!(out.os.tcpcb.t_state, TCPS_CLOSED);
}

#[test]
fn rack_timeout_persist_probes_and_rearms() {
    let mut out = running();
    assert!(out.queue(&alloc::vec![4u8; 100]));
    out.snd_wnd = 0;
    out.timers = PACE_TMR_PERSIT;
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 0;
    let result = out.rack_timeout_persist();
    assert_eq!(result.bytes, 1);
    assert_eq!(out.rxtshift, 1);
    assert_ne!(out.timers & PACE_TMR_PERSIT, 0);
}

#[test]
fn rack_timeout_keepalive_counts_and_closes() {
    let mut out = running();
    out.keepidle_us = 10;
    out.keepintvl_us = 1;
    out.timers = PACE_TMR_KEEP;
    let first = out.rack_timeout_keepalive(20);
    assert_eq!(first.bytes, 1);
    assert_eq!(out.keepalive_probes, 1);
    for _ in 0..10 {
        out.timers = PACE_TMR_KEEP;
        let _ = out.rack_timeout_keepalive(20);
        if out.os.tcpcb.t_state == TCPS_CLOSED {
            break;
        }
    }
    assert_eq!(out.os.tcpcb.t_state, TCPS_CLOSED);
}

#[test]
fn rack_process_timers_dispatches_in_order() {
    let mut out = running();
    assert!(out.queue(&alloc::vec![5u8; 100]));
    out.snd_wnd = 0;
    out.os.tcpcb.snd_una = 0;
    out.os.tcpcb.snd_max = 100;
    out.timers = PACE_TMR_DELACK | PACE_TMR_TLP | PACE_TMR_PERSIT;
    let result = out.rack_process_timers(PACE_TMR_DELACK | PACE_TMR_TLP | PACE_TMR_PERSIT, 100);
    assert_ne!(result.fired & PACE_TMR_DELACK, 0);
    assert_ne!(result.fired & PACE_TMR_TLP, 0);
    assert_ne!(result.fired & PACE_TMR_PERSIT, 0);
    assert_eq!(result.bytes, 1, "the TLP probe advanced SND.MAX");
}

#[test]
fn rack_set_state_updates_timers_and_rack_handoff_ok_checks_it() {
    let mut out = running();
    assert!(out.rack_set_state(TCPS_ESTABLISHED));
    assert_eq!(out.rack_handoff_ok(), Ok(()));
    assert_ne!(out.timers & PACE_TMR_KEEP, 0);
    assert!(out.rack_set_state(TCPS_CLOSED));
    assert_eq!(out.rack_handoff_ok(), Err(Errno::EINVAL));
    assert!(!out.rack_set_state(99));
    out.os.tcpcb.t_state = TCPS_ESTABLISHED;
    out.fin_sent = true;
    assert_eq!(out.rack_handoff_ok(), Err(Errno::EINVAL));
}

#[test]
fn rack_get_persists_timer_val_is_bounded() {
    let mut out = running();
    let base = out.rack_get_persists_timer_val();
    assert!(base >= PERSIST_MIN_US);
    assert!(base <= PERSIST_MAX_US);
    out.rxtshift = 12;
    out.os.tcpcb.t_rxtcur = 20_000;
    assert_eq!(out.rack_get_persists_timer_val(), PERSIST_MAX_US);
}
