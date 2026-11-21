// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface checks for the FreeBSD TCP notification helpers.

use raven_tcpip::{Errno, Event, SocketId, TcpNotify, TcpNotifyOutcome, TcpState};

#[test]
fn established_unreachable_is_silent_and_early_drop_emits_error() {
    let mut state = TcpNotify::new(TcpState::Established);
    assert_eq!(state.notify(Errno::EHOSTUNREACH), TcpNotifyOutcome::Ignored);

    let mut syn = TcpNotify::new(TcpState::SynSent);
    syn.rxtshift = 4;
    syn.softerror = Some(Errno::ECONNREFUSED);
    let outcome = syn.notify(Errno::ETIMEDOUT);
    assert_eq!(outcome, TcpNotifyOutcome::Dropped(Errno::ETIMEDOUT));
    assert_eq!(syn.state, TcpState::Closed);
    assert_eq!(
        outcome.event(SocketId(3)),
        Some(Event::Error(SocketId(3), Errno::ETIMEDOUT))
    );
}

#[test]
fn syn_sent_drop_is_state_gated_and_fastopen_is_disabled() {
    let mut state = TcpNotify::new(TcpState::SynSent);
    state.fastopen = true;
    assert_eq!(
        state.drop_syn_sent(Errno::ECONNREFUSED),
        TcpNotifyOutcome::Dropped(Errno::ECONNREFUSED)
    );
    assert_eq!(state.state, TcpState::Closed);
    assert!(!state.fastopen);

    let mut established = TcpNotify::new(TcpState::Established);
    assert_eq!(
        established.drop_syn_sent(Errno::ETIMEDOUT),
        TcpNotifyOutcome::Ignored
    );
    assert_eq!(established.state, TcpState::Established);
}
