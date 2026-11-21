// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the RACK transport integration callbacks.

use super::*;
use crate::{
    RackPortState, RackSockopt, RackTimeout, SockoptDir, TCP_RACK_PACE_MIN_SEG, TCPS_ESTABLISHED,
};

fn stack_running() -> RackStack {
    let mut stack = RackStack::new();
    stack.state = RackPortState::Running;
    stack
}

fn start(usecs: u32, timer_us: u32) -> HptsStart {
    HptsStart {
        id: 1,
        cts_us: 0,
        usecs,
        timers: PACE_TMR_RACK,
        timer_us,
        delayed_ack_us: 0,
        keepalive_us: 0,
        early_us: 0,
        late_us: 0,
        use_hpts_min: false,
        dgp_on: false,
        min_sleep_us: 0,
        max_reduction: 0,
    }
}

#[test]
fn start_timer_late_compensation_and_timer() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    let mut request = start(1000, 5000);
    request.late_us = 300;
    request.use_hpts_min = true;
    request.dgp_on = true;
    request.min_sleep_us = 400;
    request.max_reduction = 50;
    let outcome = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert_eq!(outcome.usecs, 700);
    assert_eq!(outcome.timeout_us, 5000);
    assert_eq!(outcome.scheduled_us, 5000);
    assert_eq!(outcome.timer_exp_us, 5000);
    assert!(outcome.inserted);
    assert!(outcome.late_compensated);
    assert_eq!(outcome.flags, PACE_TMR_RACK);
}

#[test]
fn start_timer_early_carry_and_pacing() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    let mut request = start(1000, 0);
    request.early_us = 250;
    request.timers = 0;
    let outcome = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert_eq!(outcome.usecs, 1250);
    assert_eq!(outcome.timeout_us, 0);
    assert_eq!(outcome.scheduled_us, 1250);
    assert_eq!(outcome.flags, PACE_PKT_OUTPUT);
    assert!(outcome.early_consumed);
}

#[test]
fn start_timer_keepalive_and_closed() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    let mut request = start(0, 0);
    request.timers = 0;
    request.keepalive_us = 1_000_000;
    let outcome = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert_eq!(outcome.timeout_us, 1_000_000);
    assert_eq!(outcome.flags, PACE_TMR_KEEP);
    assert!(outcome.inserted);

    stack.tp.t_state = TCPS_CLOSED;
    let closed = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert!(!closed.inserted);
    assert_eq!(closed.scheduled_us, 0);
}

#[test]
fn start_timer_clamps_to_donor_maximum() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    let request = start(1, u32::MAX);
    let outcome = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert_eq!(outcome.timeout_us, MAX_HPTS_TIMEOUT_US);
}

#[test]
fn process_timers_dispatches_in_donor_order() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 1000;
    let request = TimerProcess {
        id: 1,
        cts_us: 1000,
        hpts_calling: true,
        min_to: false,
    };
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &request),
        TimerProcessResult::Dispatched(RackTimeout::Rack)
    );

    stack.hpts_flags = u32::from(PACE_TMR_RACK | PACE_TMR_DELACK);
    stack.timer_exp = 1000;
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &request),
        TimerProcessResult::Dispatched(RackTimeout::Delack)
    );
}

#[test]
fn process_timers_early_paths() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 1000;
    let user = TimerProcess {
        id: 1,
        cts_us: 500,
        hpts_calling: false,
        min_to: false,
    };
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &user),
        TimerProcessResult::EarlyUser
    );
    let callback = TimerProcess {
        hpts_calling: true,
        ..user
    };
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &callback),
        TimerProcessResult::EarlyRearm(500)
    );
    stack.hpts_flags = u32::from(PACE_TMR_RACK) | u32::from(PACE_PKT_OUTPUT);
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &callback),
        TimerProcessResult::EarlyPaced
    );
    stack.hpts_flags = 0;
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &callback),
        TimerProcessResult::NoTimers
    );
    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.tp.t_state = TCPS_LISTEN;
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &callback),
        TimerProcessResult::Listen
    );
}

#[test]
fn stop_all_timers_marks_pending_persist() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    hpts.rack_timer_start(1, PACE_TMR_PERSIT, 500);
    stack.hpts_flags = u32::from(PACE_TMR_PERSIT);
    let outcome = rack_stop_all_timers(&mut hpts, &mut stack, 1);
    assert!(outcome.persist_pending);
    assert!(outcome.removed);
    assert_eq!(outcome.stopped_flags, PACE_TMR_PERSIT);
    assert!(stack.in_persist);
}

#[test]
fn addrack_load_and_unload_manage_context() {
    let mut os = RackOs::new();
    let mut stack = stack_running();
    let mut transport = RackTransport::default();
    let load = tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Load);
    assert!(load.registered);
    assert!(!load.unregistered);
    assert!(load.map_zone.is_some());
    assert!(load.pcb_zone.is_some());
    assert!(load.sysctl_ctx.is_some());
    assert!(load.sysctl_root.is_some());
    assert_eq!(stack.state, RackPortState::Running);

    let quiesce = tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Quiesce);
    assert!(quiesce.quiesced);

    let unload = tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Unload);
    assert!(unload.unregistered);
    assert_eq!(stack.state, RackPortState::Fini);
    assert_eq!(
        tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Unload).error,
        None
    );
}

#[test]
fn ctloutput_routes_set_get_and_errors() {
    let mut stack = stack_running();
    let set = RackSockopt {
        dir: SockoptDir::Set,
        level: 6,
        name: TCP_RACK_PACE_MIN_SEG,
        value: 5,
    };
    assert_eq!(rack_ctloutput(&mut stack, &set), Ok(None));
    let get = RackSockopt {
        dir: SockoptDir::Get,
        ..set
    };
    assert_eq!(rack_ctloutput(&mut stack, &get), Ok(Some(5)));
    let bad = RackSockopt { level: 1, ..set };
    assert_eq!(rack_ctloutput(&mut stack, &bad), Err(Errno::EOPNOTSUPP));
}

#[test]
fn hwtls_change_sets_flag_and_counter() {
    let mut stack = stack_running();
    let on = rack_hw_tls_change(&mut stack, true);
    assert!(on.enabled);
    assert!(stack.hw_tls);
    assert_eq!(on.changes, 1);
    let off = rack_hw_tls_change(&mut stack, false);
    assert!(!off.enabled);
    assert!(!stack.hw_tls);
    assert_eq!(off.changes, 2);
}

#[test]
fn pru_options_rejects_oob_only() {
    assert_eq!(rack_pru_options(PRUS_OOB), Err(Errno::EOPNOTSUPP));
    assert_eq!(rack_pru_options(0), Ok(()));
}

#[test]
fn wake_check_uses_pacing_and_timer_deadlines() {
    let mut stack = stack_running();
    stack.hpts_flags = u32::from(PACE_PKT_OUTPUT);
    stack.last_output_to = 1000;
    stack.os.now_us = 1000;
    assert!(rack_wake_check(&stack));
    stack.os.now_us = 999;
    assert!(!rack_wake_check(&stack));

    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 2000;
    stack.os.now_us = 2000;
    assert!(rack_wake_check(&stack));
    stack.os.now_us = 1999;
    assert!(!rack_wake_check(&stack));

    stack.hpts_flags = 0;
    assert!(!rack_wake_check(&stack));
}

#[test]
fn switch_failed_sets_mbufq_and_rearms() {
    let mut hpts = RackHpts::new(0);
    let mut stack = stack_running();
    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 5000;
    stack.os.now_us = 1000;
    let caps = SwitchCaps {
        mbuf_queue: true,
        always_pace: true,
        use_cmp_ack: true,
        established: true,
        in_hpts: false,
    };
    let outcome = rack_switch_failed(&mut hpts, &mut stack, 1, &caps);
    assert_ne!(outcome.t_flags2 & TF2_SUPPORTS_MBUFQ, 0);
    assert_ne!(outcome.t_flags2 & TF2_MBUF_ACKCMP, 0);
    assert_eq!(outcome.toval_us, 4000);
    assert!(outcome.inserted);

    let stuck = SwitchCaps {
        in_hpts: true,
        ..caps
    };
    let no_rearm = rack_switch_failed(&mut hpts, &mut stack, 1, &stuck);
    assert!(!no_rearm.inserted);
}
