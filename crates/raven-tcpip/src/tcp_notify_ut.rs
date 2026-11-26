// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD TCP state-change/notification helpers.

use super::*;
use crate::{Errno, Event, SocketId, TcpState};

fn notify(kind: TcpState) -> TcpNotify {
    TcpNotify::new(kind)
}

#[test]
fn state_change_returns_the_from_and_to_states() {
    let mut state = notify(TcpState::SynSent);
    let change = state.state_change(TcpState::Established);
    assert_eq!(
        change,
        TcpStateChange {
            from: TcpState::SynSent,
            to: TcpState::Established,
        }
    );
    assert_eq!(state.state, TcpState::Established);
}

#[test]
fn established_host_and_network_unreachable_errors_are_ignored() {
    for error in [Errno::EHOSTUNREACH, Errno::ENETUNREACH, Errno::EHOSTDOWN] {
        let mut state = notify(TcpState::Established);
        state.softerror = Some(Errno::ETIMEDOUT);
        state.rxtshift = 5;
        assert_eq!(state.notify(error), TcpNotifyOutcome::Ignored);
        assert_eq!(state.state, TcpState::Established);
        assert_eq!(state.softerror, Some(Errno::ETIMEDOUT));
    }
}

#[test]
fn a_soft_error_is_recorded_before_established() {
    let mut state = notify(TcpState::SynSent);
    assert_eq!(
        state.notify(Errno::ETIMEDOUT),
        TcpNotifyOutcome::SoftError(Errno::ETIMEDOUT)
    );
    assert_eq!(state.state, TcpState::SynSent);
    assert_eq!(state.softerror, Some(Errno::ETIMEDOUT));
}

#[test]
fn an_early_connection_drops_after_four_shifts_and_a_prior_soft_error() {
    let mut state = notify(TcpState::SynSent);
    state.rxtshift = 4;
    state.softerror = Some(Errno::ECONNREFUSED);
    let outcome = state.notify(Errno::ETIMEDOUT);
    assert_eq!(outcome, TcpNotifyOutcome::Dropped(Errno::ETIMEDOUT));
    assert_eq!(state.state, TcpState::Closed);
    assert_eq!(
        outcome.event(SocketId(7)),
        Some(Event::Error(SocketId(7), Errno::ETIMEDOUT))
    );
}

#[test]
fn three_shifts_are_not_enough_to_drop_early() {
    let mut state = notify(TcpState::SynReceived);
    state.rxtshift = 3;
    state.softerror = Some(Errno::ECONNREFUSED);
    assert_eq!(
        state.notify(Errno::ETIMEDOUT),
        TcpNotifyOutcome::SoftError(Errno::ETIMEDOUT)
    );
    assert_eq!(state.state, TcpState::SynReceived);
}

#[test]
fn established_soft_errors_do_not_drop_the_connection() {
    let mut state = notify(TcpState::Established);
    assert_eq!(
        state.notify(Errno::ECONNRESET),
        TcpNotifyOutcome::SoftError(Errno::ECONNRESET)
    );
    assert_eq!(state.state, TcpState::Established);
    assert_eq!(state.softerror, Some(Errno::ECONNRESET));
}

#[test]
fn drop_syn_sent_only_drops_in_syn_sent() {
    let mut syn_sent = notify(TcpState::SynSent);
    syn_sent.fastopen = true;
    let outcome = syn_sent.drop_syn_sent(Errno::ECONNREFUSED);
    assert_eq!(outcome, TcpNotifyOutcome::Dropped(Errno::ECONNREFUSED));
    assert_eq!(syn_sent.state, TcpState::Closed);
    assert!(!syn_sent.fastopen);
    assert_eq!(
        outcome.event(SocketId(9)),
        Some(Event::Error(SocketId(9), Errno::ECONNREFUSED))
    );

    let mut established = notify(TcpState::Established);
    assert_eq!(
        established.drop_syn_sent(Errno::ETIMEDOUT),
        TcpNotifyOutcome::Ignored
    );
    assert_eq!(established.state, TcpState::Established);
}

#[test]
fn only_a_drop_maps_to_a_readiness_error_event() {
    assert_eq!(TcpNotifyOutcome::Ignored.event(SocketId(1)), None);
    assert_eq!(
        TcpNotifyOutcome::SoftError(Errno::ETIMEDOUT).event(SocketId(1)),
        None
    );
    assert_eq!(
        TcpNotifyOutcome::Dropped(Errno::ETIMEDOUT).event(SocketId(1)),
        Some(Event::Error(SocketId(1), Errno::ETIMEDOUT))
    );
}
