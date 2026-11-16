// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the session/foreground table.

use super::*;

#[test]
fn a_session_tracks_fg_sid_and_hangup() {
    let mut sessions = Sessions::new();
    assert!(sessions.is_empty());
    let id = sessions.open();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions.fg(id), 0);
    assert!(sessions.set_fg(id, 42));
    assert!(sessions.set_sid(id, 7));
    assert_eq!(sessions.fg(id), 42);
    assert_eq!(sessions.sid(id), 7);
    assert!(!sessions.hung(id));
    assert!(sessions.hangup(id));
    assert!(sessions.hung(id));
}

#[test]
fn unknown_sessions_are_rejected_without_state_change() {
    let mut sessions = Sessions::new();
    assert!(sessions.get(99).is_none());
    assert!(!sessions.set_fg(99, 42));
    assert!(!sessions.set_sid(99, 7));
    assert!(!sessions.hangup(99));
    assert_eq!(sessions.fg(99), 0);
    assert_eq!(sessions.sid(99), 0);
    assert!(!sessions.close(99));
}

#[test]
fn close_removes_the_session() {
    let mut sessions = Sessions::new();
    let id = sessions.open();
    assert!(sessions.close(id));
    assert!(sessions.is_empty());
    assert!(!sessions.close(id));
}
