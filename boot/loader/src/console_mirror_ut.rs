// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the best-effort UEFI console mirror.

use core::fmt;

use super::*;

struct Failing;

impl fmt::Write for Failing {
    fn write_str(&mut self, _text: &str) -> fmt::Result {
        Err(fmt::Error)
    }
}

struct Recording {
    text: alloc::string::String,
}

impl fmt::Write for Recording {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.text.push_str(text);
        Ok(())
    }
}

#[test]
fn a_failed_console_mirror_is_ignored_without_panicking() {
    assert!(!write_best_effort(&mut Failing, "loader: line\n"));
}

#[test]
fn a_successful_console_mirror_writes_the_whole_line() {
    let mut writer = Recording {
        text: alloc::string::String::new(),
    };
    assert!(write_best_effort(&mut writer, "loader: line\n"));
    assert_eq!(writer.text, "loader: line\n");
}
