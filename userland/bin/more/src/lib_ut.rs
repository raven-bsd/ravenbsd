// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the more pager.

use alloc::string::String;
use alloc::vec::Vec;

use super::{Pager, page, parse_args};

struct FakePager {
    tty: bool,
    commands: Vec<char>,
    output: String,
}

impl Pager for FakePager {
    fn is_tty(&self) -> bool {
        self.tty
    }
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }
    fn read_command(&mut self) -> Option<char> {
        if self.commands.is_empty() {
            None
        } else {
            Some(self.commands.remove(0))
        }
    }
}

#[test]
fn parses_page_and_files() {
    let options = parse_args(["-n".into(), "5".into(), "a".into(), "b".into()]).unwrap();
    assert_eq!(options.page, 5);
    assert_eq!(options.files, vec!["a", "b"]);
    assert!(parse_args(["-n".into(), "0".into(), "a".into()]).is_err());
    assert!(parse_args(Vec::<String>::new()).is_err());
}

#[test]
fn non_tty_streams_everything() {
    let mut pager = FakePager {
        tty: false,
        commands: Vec::new(),
        output: String::new(),
    };
    assert!(page("one\ntwo\n", 1, &mut pager));
    assert_eq!(pager.output, "one\ntwo\n");
}

#[test]
fn tty_pages_and_quits() {
    let mut pager = FakePager {
        tty: true,
        commands: vec![' ', 'q'],
        output: String::new(),
    };
    assert!(!page("1\n2\n3\n4\n5\n", 2, &mut pager));
    assert_eq!(pager.output, "1\n2\n--More--\r        \r3\n4\n--More--");
}

#[test]
fn enter_advances_one_line() {
    let mut pager = FakePager {
        tty: true,
        commands: vec!['\n', 'q'],
        output: String::new(),
    };
    assert!(!page("1\n2\n3\n", 1, &mut pager));
    assert_eq!(pager.output, "1\n--More--\r        \r2\n--More--");
}
