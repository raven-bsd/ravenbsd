// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the logged-in terminal peer registry.

use super::*;

#[test]
fn registers_and_looks_up_by_user_and_terminal() {
    let mut peers = Peers::new();
    assert!(peers.is_empty());
    let alice = peers.register("alice", "pty:0/master", 1, true);
    let bob = peers.register("bob", "pty:1/slave", 2, true);
    assert_eq!(peers.len(), 2);
    assert_eq!(
        peers.lookup("alice", None).map(|peer| peer.user.as_str()),
        Some("alice")
    );
    assert_eq!(
        peers.lookup("bob", None).map(|peer| peer.tty.as_str()),
        Some("pty:1/slave")
    );
    assert_eq!(
        peers
            .lookup("bob", Some("pty:1/slave"))
            .map(|peer| peer.session),
        Some(2)
    );
    assert!(peers.lookup("bob", Some("pty:9/slave")).is_none());
    assert!(peers.lookup("carol", None).is_none());
    assert_eq!(
        peers.get(alice).map(|peer| peer.user.as_str()),
        Some("alice")
    );
    assert_eq!(peers.get(bob).map(|peer| peer.session), Some(2));
}

#[test]
fn a_repeated_user_and_tty_replaces_the_old_record() {
    let mut peers = Peers::new();
    let first = peers.register("alice", "tty:console", 1, true);
    let second = peers.register("alice", "tty:console", 2, false);
    assert_ne!(first, second);
    assert_eq!(peers.len(), 1);
    assert_eq!(
        peers.lookup("alice", None).map(|peer| peer.session),
        Some(2)
    );
    assert!(!peers.lookup("alice", None).unwrap().writable);
}

#[test]
fn writable_state_updates_by_record_and_session() {
    let mut peers = Peers::new();
    let alice = peers.register("alice", "pty:1/master", 7, true);
    let other = peers.register("alice", "pty:2/master", 7, true);
    let bob = peers.register("bob", "pty:3/slave", 8, true);
    assert!(peers.set_writable(alice, false));
    assert!(!peers.get(alice).unwrap().writable);
    assert!(peers.get(other).unwrap().writable);
    assert_eq!(peers.set_writable_for_session(7, false), 2);
    assert!(!peers.get(alice).unwrap().writable);
    assert!(!peers.get(other).unwrap().writable);
    assert!(peers.get(bob).unwrap().writable);
    assert!(!peers.set_writable(99, false));
}

#[test]
fn unregister_and_session_cleanup_remove_records() {
    let mut peers = Peers::new();
    let first = peers.register("alice", "pty:1/master", 4, true);
    let second = peers.register("alice", "pty:2/master", 4, true);
    let bob = peers.register("bob", "pty:3/slave", 5, true);
    assert_eq!(peers.unregister_session(4), 2);
    assert!(peers.get(first).is_none());
    assert!(peers.get(second).is_none());
    assert!(peers.get(bob).is_some());
    assert!(peers.unregister(bob));
    assert!(peers.is_empty());
    assert!(!peers.unregister(bob));
    assert_eq!(peers.unregister_session(5), 0);
}
