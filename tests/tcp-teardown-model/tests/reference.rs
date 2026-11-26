// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Teardown reference transitions and deadlines.

use raven_tcpip::Instant;
use tcp_teardown_model::{Event, FIN_WAIT2_MS, RefState, TIME_WAIT_MS, deadline, transition};

#[test]
fn local_close_then_peer_fin_reaches_time_wait() {
    let mut state = RefState::Established;
    state = transition(state, Event::LocalClose);
    assert_eq!(state, RefState::FinWait1);
    state = transition(state, Event::RecvAck);
    assert_eq!(state, RefState::FinWait2);
    state = transition(state, Event::RecvFin);
    assert_eq!(state, RefState::TimeWait);
    state = transition(state, Event::TimerExpired);
    assert_eq!(state, RefState::Closed);
}

#[test]
fn peer_fin_then_local_close_is_close_wait_last_ack_closed() {
    let mut state = RefState::Established;
    state = transition(state, Event::RecvFin);
    assert_eq!(state, RefState::CloseWait);
    state = transition(state, Event::LocalClose);
    assert_eq!(state, RefState::LastAck);
    state = transition(state, Event::RecvAck);
    assert_eq!(state, RefState::Closed);
}

#[test]
fn simultaneous_close_goes_through_closing() {
    let mut a = RefState::Established;
    let mut b = RefState::Established;
    a = transition(a, Event::LocalClose);
    b = transition(b, Event::LocalClose);
    assert_eq!(a, RefState::FinWait1);
    assert_eq!(b, RefState::FinWait1);
    a = transition(a, Event::RecvFin);
    b = transition(b, Event::RecvFinAck);
    assert_eq!(a, RefState::Closing);
    assert_eq!(b, RefState::TimeWait);
    a = transition(a, Event::RecvAck);
    assert_eq!(a, RefState::TimeWait);
}

#[test]
fn rst_closes_every_state() {
    for state in [
        RefState::Established,
        RefState::FinWait1,
        RefState::FinWait2,
        RefState::CloseWait,
        RefState::Closing,
        RefState::LastAck,
        RefState::TimeWait,
    ] {
        assert_eq!(
            transition(state, Event::RecvRst),
            RefState::Closed,
            "RST in {state:?}"
        );
    }
}

#[test]
fn listener_close_is_closed() {
    assert_eq!(
        transition(RefState::Listen, Event::LocalClose),
        RefState::Closed
    );
}

#[test]
fn time_wait_and_fin_wait_two_have_the_configured_deadlines() {
    let now = Instant::from_millis(1_000);
    assert_eq!(
        deadline(RefState::TimeWait, now),
        Some(Instant::from_millis(1_000 + TIME_WAIT_MS as i64))
    );
    assert_eq!(
        deadline(RefState::FinWait2, now),
        Some(Instant::from_millis(1_000 + FIN_WAIT2_MS as i64))
    );
    assert_eq!(deadline(RefState::LastAck, now), None);
}
