// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the independent RACK output/timer/HPTS model.

use super::*;

#[test]
fn slot_conversion_and_timeout_cap_are_exact() {
    assert_eq!(HPTS_USECS_PER_SLOT, 10);
    assert_eq!(NUM_OF_HPTSI_SLOTS, 102_400);
    assert_eq!(hpts_usec_to_slots(0), 0);
    assert_eq!(hpts_usec_to_slots(1), 1);
    assert_eq!(hpts_usec_to_slots(9), 1);
    assert_eq!(hpts_usec_to_slots(10), 1);
    assert_eq!(hpts_usec_to_slots(11), 2);

    let mut model = TimerModel::new(0);
    let output = model.output(1, State::Established, Timer::Rack, u32::MAX);
    assert!(output.scheduled);
    assert_eq!(output.deadline_us, Some(u64::from(MAX_HPTS_TIMEOUT_US)));
    assert_eq!(
        output.slot,
        Some(hpts_usec_to_slots(u64::from(MAX_HPTS_TIMEOUT_US)))
    );
}

#[test]
fn schedule_orders_by_slot_then_fifo_within_a_slot() {
    let mut model = TimerModel::new(0);
    assert!(
        model
            .output(2, State::Established, Timer::Rack, 100)
            .scheduled
    );
    assert!(
        model
            .output(1, State::Established, Timer::Tlp, 100)
            .scheduled
    );
    assert!(
        model
            .output(3, State::Established, Timer::Rxt, 200)
            .scheduled
    );

    let result = model.softclock(200);
    assert_eq!(result.early_reinserts, 0);
    assert_eq!(
        result
            .callbacks
            .iter()
            .map(|callback| callback.id)
            .collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
    assert_eq!(
        result
            .callbacks
            .iter()
            .map(|callback| callback.action)
            .collect::<Vec<_>>(),
        vec![Some(Timer::Rack), Some(Timer::Tlp), Some(Timer::Rxt)]
    );
    assert_eq!(
        result
            .callbacks
            .iter()
            .map(|callback| callback.deadline_us)
            .collect::<Vec<_>>(),
        vec![100, 100, 200]
    );
    assert!(
        result
            .callbacks
            .iter()
            .all(|callback| callback.fired_at_us == 200)
    );
}

#[test]
fn cancel_invalidates_the_queued_callback() {
    let mut model = TimerModel::new(0);
    model.output(1, State::Established, Timer::Rack, 100);
    assert_eq!(model.cancel(1), PACE_TMR_RACK);
    assert_eq!(model.deadline(1), None);
    assert!(!model.in_hpts(1));
    assert!(model.softclock(100).callbacks.is_empty());
}

#[test]
fn output_refuses_closed_listen_and_already_queued() {
    let mut model = TimerModel::new(0);
    assert_eq!(
        model.output(1, State::Closed, Timer::Rack, 100).refusal,
        Some(Refusal::ClosedOrListen)
    );
    assert_eq!(
        model.output(1, State::Listen, Timer::Rack, 100).refusal,
        Some(Refusal::ClosedOrListen)
    );
    assert!(
        model
            .output(1, State::Established, Timer::Rack, 100)
            .scheduled
    );
    let again = model.output(1, State::Established, Timer::Tlp, 200);
    assert_eq!(again.refusal, Some(Refusal::AlreadyQueued));
    assert_eq!(again.deadline_us, Some(100));
    assert_eq!(model.deadline(1), Some(100));

    model.cancel(1);
    assert!(
        model
            .output(1, State::Established, Timer::Tlp, 200)
            .scheduled
    );
    assert_eq!(model.deadline(1), Some(200));
}

#[test]
fn stopall_blocks_future_output() {
    let mut model = TimerModel::new(0);
    model.output(1, State::Established, Timer::Rack, 100);
    assert_eq!(model.stopall(1), PACE_TMR_RACK);
    assert!(model.stopped(1));
    assert_eq!(
        model.output(1, State::Established, Timer::Tlp, 100).refusal,
        Some(Refusal::Stopped)
    );
    assert!(model.softclock(100).callbacks.is_empty());
}

#[test]
fn stop_all_timers_enters_persist_until_exit() {
    let mut model = TimerModel::new(0);
    model.output(1, State::Established, Timer::Persist, 100);
    assert_eq!(model.stop_all_timers(1), PACE_TMR_PERSIT);
    assert!(model.persist(1));
    assert_eq!(
        model.output(1, State::Established, Timer::Tlp, 100).refusal,
        Some(Refusal::Persist)
    );

    model.exit_persist(1);
    assert!(!model.persist(1));
    assert!(
        model
            .output(1, State::Established, Timer::Tlp, 100)
            .scheduled
    );
    assert_eq!(model.softclock(100).callbacks.len(), 1);
}

#[test]
fn remxt_cancels_and_sets_override_until_output() {
    let mut model = TimerModel::new(0);
    model.output(1, State::Established, Timer::Rack, 100);
    assert_eq!(model.remxt(1), PACE_TMR_RACK);
    assert!(model.timer_override(1));
    assert_eq!(model.deadline(1), None);
    assert!(model.softclock(100).callbacks.is_empty());

    assert!(
        model
            .output(1, State::Established, Timer::Tlp, 100)
            .scheduled
    );
    assert!(!model.timer_override(1));
}

#[test]
fn dispatch_priority_matches_rack_process_timers() {
    let all = PACE_TMR_DELACK
        | PACE_TMR_RACK
        | PACE_TMR_TLP
        | PACE_TMR_RXT
        | PACE_TMR_PERSIT
        | PACE_TMR_KEEP;
    assert_eq!(timeout_action(all), Some(Timer::Delack));
    assert_eq!(timeout_action(all & !PACE_TMR_DELACK), Some(Timer::Rack));
    assert_eq!(
        timeout_action(all & !(PACE_TMR_DELACK | PACE_TMR_RACK)),
        Some(Timer::Tlp)
    );
    assert_eq!(
        timeout_action(all & !(PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP)),
        Some(Timer::Rxt)
    );
    assert_eq!(
        timeout_action(all & !(PACE_TMR_DELACK | PACE_TMR_RACK | PACE_TMR_TLP | PACE_TMR_RXT)),
        Some(Timer::Persist)
    );
    assert_eq!(
        timeout_action(
            all & !(PACE_TMR_DELACK
                | PACE_TMR_RACK
                | PACE_TMR_TLP
                | PACE_TMR_RXT
                | PACE_TMR_PERSIT)
        ),
        Some(Timer::Keepalive)
    );
    assert_eq!(timeout_action(0), None);
}

#[test]
fn softclock_reinserts_an_early_slot_instead_of_firing() {
    let mut model = TimerModel::new(12);
    let output = model.output(1, State::Established, Timer::Rack, 10);
    assert_eq!(output.deadline_us, Some(22));
    assert_eq!(output.slot, Some(2));

    let first = model.softclock(20);
    assert!(first.callbacks.is_empty());
    assert_eq!(first.early_reinserts, 1);
    assert_eq!(model.deadline(1), Some(22));
    assert!(model.in_hpts(1));

    assert!(model.softclock(29).callbacks.is_empty());
    let fired = model.softclock(30);
    assert_eq!(fired.callbacks.len(), 1);
    assert_eq!(fired.callbacks[0].deadline_us, 22);
    assert_eq!(fired.callbacks[0].fired_at_us, 30);
    assert_eq!(fired.callbacks[0].slot, 3);
    assert_eq!(fired.callbacks[0].action, Some(Timer::Rack));
}

#[test]
fn zero_delay_does_not_arm_a_timer() {
    let mut model = TimerModel::new(0);
    let output = model.output(1, State::Established, Timer::Rack, 0);
    assert!(!output.scheduled);
    assert_eq!(output.refusal, Some(Refusal::NoTimer));
    assert_eq!(model.deadline(1), None);
    assert!(!model.in_hpts(1));
}

#[test]
fn randomized_10k_traces_are_deterministic_and_invariant() {
    for seed in 1..=TRACE_COUNT {
        let scenario = Scenario::generate(seed);
        assert!(
            scenario
                .events
                .windows(2)
                .all(|pair| pair[0].at_us <= pair[1].at_us)
        );
        let first = run(&scenario);
        let second = run(&scenario);
        assert_eq!(first, second);
        assert_eq!(first.fired as usize, first.callbacks.len());

        let outputs = scenario
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::Output { .. }))
            .count() as u32;
        assert_eq!(first.scheduled + first.refused, outputs);

        let mut last_fired = 0;
        for callback in &first.callbacks {
            assert_ne!(callback.flags, 0);
            assert_eq!(callback.flags & !PACE_TMR_MASK, 0);
            assert_eq!(callback.action, timeout_action(callback.flags));
            assert!(callback.fired_at_us >= callback.deadline_us);
            assert!(callback.fired_at_us >= last_fired);
            last_fired = callback.fired_at_us;
        }

        if seed <= 100 {
            assert_eq!(
                trace_digest(&scenario, &first),
                trace_digest(&scenario, &second)
            );
        }
    }
}

#[test]
fn trace_artifact_matches_generation() {
    let generated = render_trace();
    assert_eq!(generated, include_str!("../traces/rack-timer-10k.tsv"));
    assert!(generated.lines().take(2).all(|line| line.starts_with('#')));

    let rows: Vec<&str> = generated
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(rows.len(), TRACE_COUNT as usize);
    assert_eq!(rows[0].split('\t').count(), 10);
    assert_eq!(
        rows[0]
            .split('\t')
            .next()
            .and_then(|seed| seed.parse().ok()),
        Some(1u64)
    );
    assert_eq!(
        rows[0]
            .split('\t')
            .nth(1)
            .and_then(|events| events.parse().ok()),
        Some(Scenario::generate(1).events.len())
    );
}
