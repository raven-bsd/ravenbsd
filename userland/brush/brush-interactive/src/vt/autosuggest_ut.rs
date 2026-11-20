// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Host tests for the history autosuggestion matcher.

use super::suffix;

struct History(Vec<&'static str>);

impl crate::vt::line_editor::History for History {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, index: usize) -> Option<&str> {
        self.0.get(index).copied()
    }
}

#[test]
fn newest_matching_entry_wins() {
    let history = History(vec!["echo one", "ls", "echo hello"]);
    assert_eq!(suffix("echo ", &history).as_deref(), Some("hello"));
    assert_eq!(suffix("echo h", &history).as_deref(), Some("ello"));
}

#[test]
fn exact_and_absent_prefixes_have_no_hint() {
    let history = History(vec!["echo hello"]);
    assert_eq!(suffix("echo hello", &history), None);
    assert_eq!(suffix("cat ", &history), None);
    assert_eq!(suffix("", &history), None);
}

#[test]
fn multi_line_entries_are_skipped() {
    let history = History(vec!["echo first\nsecond", "echo first"]);
    assert_eq!(suffix("echo f", &history).as_deref(), Some("irst"));
}
