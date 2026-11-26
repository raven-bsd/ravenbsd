// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent K343 matrix calls into the merged RACK output/timer/state API.

use super::MATRIX;
use rack_tlp_model::model::{Action, Config as TlpConfig, RackModel};
use rack_vtable_model::model::{TcpState as VtableState, VtableModel};
use raven_tcpip::{
    PACE_TMR_DELACK, PACE_TMR_KEEP, PACE_TMR_PERSIT, PACE_TMR_RACK, PACE_TMR_RXT, PACE_TMR_TLP,
    RackOutput, TCP_FUNC_DEFAULT_OK, TCP_FUNC_OUTPUT_CANDROP,
};

fn queued(bytes: usize) -> RackOutput {
    let mut output = RackOutput::new();
    assert!(output.queue(&vec![0x41; bytes]));
    output
}

#[test]
fn rack_output_sends_new_data() {
    let mut output = queued(2_000);
    output.rack_output().unwrap();
    assert_eq!(output.bytes_transmitted, 1_460);
    assert_eq!(output.last_action, "send");
    assert!(output.packet_output_armed());
}

#[test]
fn tcp_rack_output_returns_candrop_on_closed() {
    let mut output = RackOutput::new();
    assert_eq!(
        output.tcp_rack_output(),
        i32::try_from(TCP_FUNC_DEFAULT_OK).unwrap()
    );
    output.rack_set_state(0);
    assert_eq!(
        output.tcp_rack_output(),
        i32::try_from(TCP_FUNC_OUTPUT_CANDROP).unwrap()
    );
}

#[test]
fn persist_timer_is_bounded() {
    let mut output = RackOutput::new();
    output.rxtshift = 0;
    assert_eq!(output.rack_get_persists_timer_val(), 250_000);
    output.os.tcpcb.t_rxtcur = 20_000;
    output.rxtshift = 0;
    assert_eq!(output.rack_get_persists_timer_val(), 1_000_000);
}

#[test]
fn timer_start_arms_and_sets_bits() {
    let mut output = RackOutput::new();
    let slot = output.rack_timer_start(PACE_TMR_RXT, 1_000);
    assert_ne!(slot, 0);
    assert_ne!(output.timers & PACE_TMR_RXT, 0);
    assert_eq!(output.last_action, "timer_start");
}

#[test]
fn hpts_timer_arms_a_slot() {
    let mut output = RackOutput::new();
    let slot = output.rack_start_hpts_timer(2_000, PACE_TMR_RACK);
    assert_ne!(slot, 0);
    assert_ne!(output.timers & PACE_TMR_RACK, 0);
}

#[test]
fn timer_cancel_clears_the_bits() {
    let mut output = RackOutput::new();
    output.rack_timer_start(PACE_TMR_RXT, 1_000);
    let flags = output.rack_timer_cancel();
    assert_ne!(flags & PACE_TMR_RXT, 0);
    assert_eq!(output.timers & PACE_TMR_RXT, 0);
}

#[test]
fn timer_audit_returns_due_bits() {
    let mut output = RackOutput::new();
    output.rack_timer_start(PACE_TMR_RXT, 100);
    assert_eq!(output.rack_timer_audit(0), 0);
    assert_ne!(output.rack_timer_audit(1_000), 0);
}

#[test]
fn remxt_sets_override_and_backoff() {
    let mut output = RackOutput::new();
    output.rack_timer_start(PACE_TMR_RXT, 1_000);
    let flags = output.rack_remxt_tmr();
    assert_ne!(flags & PACE_TMR_RXT, 0);
    assert!(output.must_retran);
    assert_eq!(output.rxtshift, 1);
}

#[test]
fn stopall_clears_all_timers() {
    let mut output = RackOutput::new();
    output.rack_timer_start(PACE_TMR_RXT | PACE_TMR_TLP, 1_000);
    assert_ne!(output.rack_stopall(), 0);
    assert_eq!(output.timers, 0);
    assert!(!output.persist);
}

#[test]
fn stop_all_timers_clears_bits() {
    let mut output = RackOutput::new();
    output.rack_timer_start(PACE_TMR_RXT, 1_000);
    assert_ne!(output.rack_stop_all_timers(), 0);
    assert_eq!(output.timers & PACE_TMR_RXT, 0);
}

#[test]
fn enter_persist_arms_once() {
    let mut output = RackOutput::new();
    assert!(output.rack_enter_persist());
    assert!(output.persist);
    assert_ne!(output.timers & PACE_TMR_PERSIT, 0);
    assert!(!output.rack_enter_persist());
}

#[test]
fn exit_persist_clears_once() {
    let mut output = RackOutput::new();
    assert!(!output.rack_exit_persist());
    assert!(output.rack_enter_persist());
    assert!(output.rack_exit_persist());
    assert!(!output.persist);
    assert!(!output.rack_exit_persist());
}

#[test]
fn delack_sets_acknow_once() {
    let mut output = RackOutput::new();
    assert!(!output.rack_timeout_delack());
    output.timers |= PACE_TMR_DELACK;
    assert!(output.rack_timeout_delack());
    assert_ne!(output.os.tcpcb.t_flags & 0x0020, 0);
    assert!(!output.rack_timeout_delack());
}

#[test]
fn timeout_rack_requests_retransmit() {
    let mut output = RackOutput::new();
    output.os.tcpcb.snd_max = 1_460;
    assert!(!output.rack_timeout_rack());
    output.timers |= PACE_TMR_RACK;
    assert!(output.rack_timeout_rack());
    assert!(output.must_retran);
}

#[test]
fn timeout_tlp_caps_at_two_probes() {
    let mut output = RackOutput::new();
    output.os.tcpcb.snd_max = 1_460;
    output.timers |= PACE_TMR_TLP;
    assert_eq!(output.rack_timeout_tlp().bytes, 1);
    assert_eq!(output.rack_timeout_tlp().bytes, 1);
    assert_eq!(output.rack_timeout_tlp().bytes, 0);
}

#[test]
fn timeout_rxt_backs_off_and_closes() {
    let mut output = RackOutput::new();
    output.os.tcpcb.snd_max = 1_460;
    output.timers |= PACE_TMR_RXT;
    let mut closed = false;
    for _ in 0..20 {
        if output.rack_timeout_rxt().closed {
            closed = true;
            break;
        }
    }
    assert!(closed, "RXT did not close at the backoff limit");
    assert_eq!(output.os.tcpcb.t_state, 0);
}

#[test]
fn timeout_persist_probes_and_rearms() {
    let mut output = queued(2_000);
    output.snd_wnd = 0;
    output.rack_output().unwrap();
    assert!(output.persist);
    let result = output.rack_timeout_persist();
    assert_eq!(result.bytes, 1);
    assert_ne!(output.timers & PACE_TMR_PERSIT, 0);
    assert_eq!(output.rxtshift, 1);
}

#[test]
fn keepalive_counts_and_closes() {
    let mut output = RackOutput::new();
    output.timers |= PACE_TMR_KEEP;
    let idle = output.rack_timeout_keepalive(0);
    assert_eq!(idle.bytes, 0);
    output.last_rx_us = 0;
    let mut closed = false;
    for _ in 0..12 {
        let result = output.rack_timeout_keepalive(i64::from(output.keepidle_us) + 1);
        if result.closed {
            closed = true;
            break;
        }
    }
    assert!(closed, "keepalive did not close at the probe limit");
    assert_eq!(output.os.tcpcb.t_state, 0);
}

#[test]
fn process_timers_dispatches_in_donor_order() {
    let mut output = RackOutput::new();
    output.os.tcpcb.snd_max = 1_460;
    output.timers |= PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP;
    let result = output.rack_process_timers(PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP, 1_000);
    assert_eq!(
        result.fired & (PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP),
        PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP
    );
    assert_eq!(result.bytes, 1);
}

#[test]
fn set_state_updates_timers() {
    let mut output = RackOutput::new();
    assert!(output.rack_set_state(4));
    assert_ne!(output.timers & PACE_TMR_KEEP, 0);
    assert!(output.rack_set_state(0));
    assert_eq!(output.timers, 0);
    assert!(!output.rack_set_state(99));
}

#[test]
fn handoff_rejects_fin_and_closed() {
    let mut output = RackOutput::new();
    assert!(output.rack_handoff_ok().is_ok());
    output.fin_sent = true;
    assert!(output.rack_handoff_ok().is_err());
    output.fin_sent = false;
    output.rack_set_state(0);
    assert!(output.rack_handoff_ok().is_err());
}

#[test]
fn trace_alignment_with_tlp_and_vtable_models() {
    // TLP: the independent model and the product both stop after two probes.
    let mut model = RackModel::new(TlpConfig::default(), 14_600, u32::MAX);
    model.on_sent(1_000, 2_000, 0);
    let mut model_fires = 0;
    for index in 1..=3 {
        if let Action::Fire { .. } = model.poll(index * 1_000) {
            model_fires += 1;
        }
    }
    assert_eq!(model_fires, 2);
    let mut output = RackOutput::new();
    output.os.tcpcb.snd_max = 2_000;
    output.timers |= PACE_TMR_TLP;
    let product_bytes: u32 = (0..3).map(|_| output.rack_timeout_tlp().bytes).sum();
    assert_eq!(product_bytes, 2);

    // FIN handoff: both reject a FIN with outstanding data.
    let mut vtable = VtableModel::new();
    vtable.register();
    vtable.init();
    vtable.state = VtableState::Established;
    vtable.sent_fin = true;
    vtable.snd_max = 2_000;
    vtable.snd_una = 0;
    assert_ne!(vtable.handoff_ok(), 0);
    let mut product = RackOutput::new();
    product.fin_sent = true;
    assert!(product.rack_handoff_ok().is_err());
}

#[test]
fn matrix_rows_are_complete_and_unique() {
    assert_eq!(MATRIX.len(), 21);
    let mut names: Vec<&str> = MATRIX.iter().map(|row| row.0).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 21);
    for row in MATRIX {
        assert!(!row.1.is_empty() && !row.2.is_empty() && !row.3.is_empty());
    }
}
