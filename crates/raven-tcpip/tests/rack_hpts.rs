// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface checks for the HPTS/rack timer context.

use raven_tcpip::{
    PACE_TMR_DELACK, PACE_TMR_KEEP, PACE_TMR_RACK, RackHpts, RackTimeout, rack_timeout_action,
};

#[test]
fn public_rack_hpts_schedules_and_dispatches() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(5);
    assert_eq!(hpts.rack_timer_start(5, PACE_TMR_RACK, 250), 250);
    assert_eq!(hpts.now_ms(), 0);
    let callbacks = hpts.tcp_hpts_softclock(250);
    assert_eq!(callbacks.len(), 1);
    assert_eq!(callbacks[0].id, 5);
    assert_eq!(callbacks[0].timers, PACE_TMR_RACK);
    assert_eq!(callbacks[0].now_ms, 0);
}

#[test]
fn public_timeout_priority_and_cancel() {
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP | PACE_TMR_DELACK),
        Some(RackTimeout::Delack)
    );
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP),
        Some(RackTimeout::Keepalive)
    );
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    assert_eq!(hpts.rack_timer_cancel(1), PACE_TMR_RACK);
    assert!(hpts.tcp_hpts_softclock(100).is_empty());
}
