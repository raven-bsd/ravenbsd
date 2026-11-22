// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD HPTS/callout context.

use super::*;
use alloc::vec;
use alloc::vec::Vec;

#[test]
fn tcp_hpts_init_creates_an_entry() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(7);
    let entry = hpts.entries.get(&7).expect("entry");
    assert_eq!(entry.timers, 0);
    assert_eq!(entry.rc_tmr_stopped, 0);
    assert!(!entry.timers_stopped);
}

#[test]
fn tcp_hpts_insert_schedules_in_slot_and_fifo_order() {
    let mut hpts = RackHpts::new(0);
    for id in 1..=3 {
        hpts.tcp_hpts_init(id);
    }
    assert_eq!(hpts.tcp_hpts_insert(1, 100), 100);
    assert_eq!(hpts.tcp_hpts_insert(2, 100), 100);
    assert_eq!(hpts.tcp_hpts_insert(3, 500), 500);

    let callbacks = hpts.tcp_hpts_softclock(1_000);
    assert_eq!(
        callbacks
            .iter()
            .map(|callback| callback.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(callbacks.iter().all(|callback| callback.timers == 0));
    assert!(callbacks.iter().all(|callback| callback.now_us == 1_000));
    assert!(callbacks.iter().all(|callback| callback.now_ms == 1));
}

#[test]
fn tcp_hpts_insert_zero_delay_runs_in_the_current_slot() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    assert_eq!(hpts.tcp_hpts_insert(1, 0), 0);
    let callbacks = hpts.tcp_hpts_softclock(0);
    assert_eq!(callbacks.len(), 1);
    assert_eq!(callbacks[0].id, 1);
}

#[test]
fn tcp_hpts_remove_cancels_and_returns_flags() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    assert_eq!(hpts.tcp_hpts_remove(1), PACE_TMR_RACK);
    assert_eq!(hpts.entries[&1].timers, 0);
    assert_eq!(hpts.entries[&1].rc_tmr_stopped, PACE_TMR_RACK);
    assert!(hpts.tcp_hpts_softclock(1_000).is_empty());
}

#[test]
fn tcp_hpts_remove_unknown_id_returns_zero() {
    let mut hpts = RackHpts::new(0);
    assert_eq!(hpts.tcp_hpts_remove(99), 0);
}

#[test]
fn tcp_hpts_softclock_returns_due_callbacks_and_clears_them() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.tcp_hpts_init(2);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    hpts.rack_timer_start(2, PACE_TMR_TLP, 100);
    assert_eq!(hpts.tcp_hpts_softclock(150).len(), 2);
    assert!(hpts.tcp_hpts_softclock(200).is_empty());
    assert_eq!(hpts.now_us(), 200);
    assert_eq!(hpts.now_ms(), 0);
}

#[test]
fn rack_timer_start_sets_flags_and_returns_the_due_time() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    assert_eq!(hpts.rack_timer_start(1, PACE_TMR_DELACK, 500), 500);
    assert_eq!(hpts.entries[&1].timers, PACE_TMR_DELACK);
    let callbacks = hpts.tcp_hpts_softclock(500);
    assert_eq!(callbacks.len(), 1);
    assert_eq!(callbacks[0].timers, PACE_TMR_DELACK);
}

#[test]
fn rack_timer_start_replaces_pending_timer_state() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    hpts.rack_timer_start(1, PACE_TMR_TLP, 500);
    assert_eq!(hpts.entries[&1].timers, PACE_TMR_TLP);
    assert!(hpts.tcp_hpts_softclock(200).is_empty());
    let callbacks = hpts.tcp_hpts_softclock(500);
    assert_eq!(callbacks.len(), 1);
    assert_eq!(callbacks[0].timers, PACE_TMR_TLP);
}

#[test]
fn rack_timer_cancel_clears_all_timers() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK | PACE_TMR_TLP, 100);
    assert_eq!(hpts.rack_timer_cancel(1), PACE_TMR_RACK | PACE_TMR_TLP);
    assert_eq!(hpts.entries[&1].timers, 0);
    assert_eq!(
        hpts.entries[&1].rc_tmr_stopped,
        PACE_TMR_RACK | PACE_TMR_TLP
    );
    assert!(hpts.tcp_hpts_softclock(1_000).is_empty());
}

#[test]
fn rack_timer_start_refuses_in_persist() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_PERSIT, 100);
    assert_eq!(hpts.rack_stop_all_timers(1), PACE_TMR_PERSIT);
    assert!(hpts.entries[&1].in_persist);
    assert_eq!(hpts.rack_timer_start(1, PACE_TMR_RACK, 100), 0);
    assert_eq!(hpts.entries[&1].timers, 0);
    assert!(hpts.tcp_hpts_softclock(1_000).is_empty());
}

#[test]
fn rack_remxt_tmr_sets_override_and_cancels() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    assert_eq!(hpts.rack_remxt_tmr(1), PACE_TMR_RACK);
    assert!(hpts.entries[&1].timer_override);
    assert_eq!(hpts.entries[&1].timers, 0);
    assert_eq!(hpts.entries[&1].rc_tmr_stopped, PACE_TMR_RACK);
}

#[test]
fn rack_stopall_sets_stopped_and_removes() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_RACK, 100);
    assert_eq!(hpts.rack_stopall(1), PACE_TMR_RACK);
    assert!(hpts.entries[&1].timers_stopped);
    assert_eq!(hpts.rack_timer_start(1, PACE_TMR_RACK, 100), 0);
    assert!(hpts.tcp_hpts_softclock(1_000).is_empty());
}

#[test]
fn rack_stop_all_timers_records_persist() {
    let mut hpts = RackHpts::new(0);
    hpts.tcp_hpts_init(1);
    hpts.rack_timer_start(1, PACE_TMR_PERSIT, 100);
    assert_eq!(hpts.rack_stop_all_timers(1), PACE_TMR_PERSIT);
    assert!(hpts.entries[&1].in_persist);
    assert_eq!(hpts.entries[&1].timers, 0);
}

#[test]
fn rack_timeout_action_follows_the_donor_priority() {
    assert_eq!(
        rack_timeout_action(
            PACE_TMR_KEEP
                | PACE_TMR_PERSIT
                | PACE_TMR_RXT
                | PACE_TMR_TLP
                | PACE_TMR_RACK
                | PACE_TMR_DELACK
        ),
        Some(RackTimeout::Delack)
    );
    assert_eq!(
        rack_timeout_action(
            PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK
        ),
        Some(RackTimeout::Rack)
    );
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP),
        Some(RackTimeout::Tlp)
    );
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT),
        Some(RackTimeout::Rxt)
    );
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP | PACE_TMR_PERSIT),
        Some(RackTimeout::Persist)
    );
    assert_eq!(
        rack_timeout_action(PACE_TMR_KEEP),
        Some(RackTimeout::Keepalive)
    );
    assert_eq!(rack_timeout_action(0), None);
}

#[test]
fn now_and_slot_conversion_are_exact() {
    let hpts = RackHpts::new(1_234_567);
    assert_eq!(hpts.now_us(), 1_234_567);
    assert_eq!(hpts.now_ms(), 1_234);
    assert_eq!(hpts_usec_to_slots(0), 0);
    assert_eq!(hpts_usec_to_slots(1), 1);
    assert_eq!(hpts_usec_to_slots(10), 1);
    assert_eq!(hpts_usec_to_slots(11), 2);
    assert_eq!(HPTS_USECS_PER_SLOT, 10);
    assert_eq!(NUM_OF_HPTSI_SLOTS, 102_400);
}
