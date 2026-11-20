// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-to-event parser tests.

use super::parse;
use crate::app::Event;

#[test]
fn canonical_line_becomes_characters_and_enter() {
    assert_eq!(
        parse(b"/net\n"),
        vec![
            Event::Char('/'),
            Event::Char('n'),
            Event::Char('e'),
            Event::Char('t'),
            Event::Enter,
        ]
    );
}

#[test]
fn ansi_arrows_are_decoded() {
    assert_eq!(
        parse(b"\x1b[A\x1b[B\x1b[C\x1b[D"),
        vec![Event::Up, Event::Down, Event::Right, Event::Left]
    );
}

#[test]
fn control_keys_and_backspace_are_decoded() {
    assert_eq!(
        parse(b"\x13\x03\x7f\x08\t"),
        vec![
            Event::CtrlSave,
            Event::CtrlCancel,
            Event::Backspace,
            Event::Backspace,
            Event::Tab,
        ]
    );
}

#[test]
fn lone_escape_is_escape() {
    assert_eq!(parse(b"\x1b"), vec![Event::Escape]);
}
