// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Renderer tests: panes, labels, plain/ANSI modes and width fitting.

use std::path::Path;

use super::render;
use crate::app::{Event, Options, Tui};

fn tui(options: Options) -> Tui {
    Tui::new(Path::new("/tmp/opencode/raven-tui-render"), options)
}

#[test]
fn plain_frame_has_both_panes_and_the_form() {
    let frame = render(&tui(Options::default()));
    assert!(frame.contains("Modules"));
    assert!(frame.contains("Network"));
    assert!(frame.contains("Hostname: raven"));
    assert!(frame.contains("[ Save ]"));
    assert!(frame.contains("status:"));
    assert!(!frame.contains("\x1b["));
}

#[test]
fn color_frame_contains_ansi_styling() {
    let frame = render(&tui(Options {
        color: true,
        ..Options::default()
    }));
    assert!(frame.contains("\x1b[2J"));
    assert!(frame.contains("\x1b[1m"));
}

#[test]
fn read_only_module_shows_back_not_save() {
    let mut tui = tui(Options::default());
    for character in "about".chars() {
        tui.handle(Event::Char(character));
    }
    tui.handle(Event::Enter);
    let frame = render(&tui);
    assert!(frame.contains("[ Back ]"));
    assert!(!frame.contains("[ Save ]"));
}

#[test]
fn lines_fit_the_requested_width() {
    let frame = render(&tui(Options {
        width: 80,
        ..Options::default()
    }));
    assert!(frame.lines().all(|line| line.chars().count() <= 80));
}
