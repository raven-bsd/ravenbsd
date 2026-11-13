// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[derive(Clone, Default)]
struct Recorder {
    log: std::cell::RefCell<std::vec::Vec<&'static str>>,
}

impl ForkEvents for Recorder {
    fn prefork(&mut self) {
        self.log.borrow_mut().push("prefork");
    }
    fn postfork_parent(&mut self) {
        self.log.borrow_mut().push("parent");
    }
    fn postfork_child(&mut self) {
        self.log.borrow_mut().push("child");
    }
}

#[test]
fn trait_calls_are_ordered_by_the_caller() {
    let mut parent = Recorder::default();
    parent.prefork();
    let mut child = parent.clone();
    parent.postfork_parent();
    child.postfork_child();
    assert_eq!(*parent.log.borrow(), ["prefork", "parent"]);
    assert_eq!(*child.log.borrow(), ["prefork", "child"]);
}

#[test]
fn hooks_are_object_safe() {
    let mut recorder = Recorder::default();
    let hooks: &mut dyn ForkEvents = &mut recorder;
    hooks.prefork();
    hooks.postfork_parent();
    hooks.postfork_child();
    assert_eq!(recorder.log.borrow().len(), 3);
}

#[test]
fn clone_keeps_the_hook_state() {
    let mut recorder = Recorder::default();
    recorder.prefork();
    let clone = recorder.clone();
    assert_eq!(*clone.log.borrow(), ["prefork"]);
}

#[test]
fn parent_and_child_are_independent() {
    let mut parent = Recorder::default();
    let mut child = parent.clone();
    parent.postfork_parent();
    child.postfork_child();
    assert_eq!(*parent.log.borrow(), ["parent"]);
    assert_eq!(*child.log.borrow(), ["child"]);
}

#[test]
fn empty_recorder_has_no_entries() {
    assert!(Recorder::default().log.borrow().is_empty());
}

#[test]
fn default_hooks_are_idempotent() {
    let mut recorder = Recorder::default();
    recorder.prefork();
    recorder.prefork();
    assert_eq!(recorder.log.borrow().len(), 2);
}
