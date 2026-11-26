// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-API integration tests for the RACK transport callbacks.

use raven_tcpip::{
    AddRackMode, HptsStart, PACE_TMR_PERSIT, PACE_TMR_RACK, RackHpts, RackOs, RackPortState,
    RackStack, RackTimeout, RackTransport, SockoptDir, TCP_RACK_PACE_MIN_SEG, TCPS_ESTABLISHED,
    TimerProcess, TimerProcessResult, rack_ctloutput, rack_process_timers, rack_start_hpts_timer,
    rack_stop_all_timers, tcp_addrack,
};

#[test]
fn hpts_handoff_dispatches_and_stops() {
    let mut hpts = RackHpts::new(0);
    let mut stack = RackStack::new();
    stack.state = RackPortState::Running;
    stack.tp.t_state = TCPS_ESTABLISHED;
    let request = HptsStart {
        id: 7,
        cts_us: 0,
        usecs: 0,
        timers: PACE_TMR_RACK,
        timer_us: 2500,
        delayed_ack_us: 0,
        keepalive_us: 0,
        early_us: 0,
        late_us: 0,
        use_hpts_min: false,
        dgp_on: false,
        min_sleep_us: 0,
        max_reduction: 0,
    };
    let started = rack_start_hpts_timer(&mut hpts, &stack, &request);
    assert!(started.inserted);
    assert_eq!(started.timeout_us, 2500);
    assert_eq!(started.flags, PACE_TMR_RACK);

    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 2500;
    let process = TimerProcess {
        id: 7,
        cts_us: 2500,
        hpts_calling: true,
        min_to: false,
    };
    assert_eq!(
        rack_process_timers(&mut hpts, &mut stack, &process),
        TimerProcessResult::Dispatched(RackTimeout::Rack)
    );

    hpts.rack_timer_start(7, PACE_TMR_PERSIT, 500);
    stack.hpts_flags = u32::from(PACE_TMR_PERSIT);
    let stopped = rack_stop_all_timers(&mut hpts, &mut stack, 7);
    assert!(stopped.persist_pending);
    assert!(stack.in_persist);
}

#[test]
fn module_and_transport_callbacks_are_public() {
    let mut os = RackOs::new();
    let mut stack = RackStack::new();
    let mut transport = RackTransport::default();
    assert!(tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Load).registered);
    assert_eq!(stack.state, RackPortState::Running);

    let set = raven_tcpip::RackSockopt {
        dir: SockoptDir::Set,
        level: 6,
        name: TCP_RACK_PACE_MIN_SEG,
        value: 4,
    };
    assert_eq!(rack_ctloutput(&mut stack, &set), Ok(None));
    let get = raven_tcpip::RackSockopt {
        dir: SockoptDir::Get,
        ..set
    };
    assert_eq!(rack_ctloutput(&mut stack, &get), Ok(Some(4)));

    assert!(raven_tcpip::rack_hw_tls_change(&mut stack, true).enabled);
    assert_eq!(raven_tcpip::rack_pru_options(0), Ok(()));
    stack.hpts_flags = u32::from(PACE_TMR_RACK);
    stack.timer_exp = 10;
    stack.os.now_us = 10;
    assert!(raven_tcpip::rack_wake_check(&stack));
    assert!(tcp_addrack(&mut transport, &mut os, &mut stack, AddRackMode::Unload).unregistered);
}
