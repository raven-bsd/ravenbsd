// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `ps` pure logic.

use super::*;
use std::string::String;

#[test]
fn empty_args_lists() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::List));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn json_is_recognised() {
    assert_eq!(parse_args(["--json"].into_iter()), Ok(Action::ListJson));
    assert_eq!(
        parse_args(["--json", "--"].into_iter()),
        Ok(Action::ListJson)
    );
}

#[test]
fn conflicting_or_repeated_flags_are_rejected() {
    assert_eq!(
        parse_args(["--json", "--selftest"].into_iter()),
        Err(UsageError)
    );
    assert_eq!(
        parse_args(["--selftest", "--json"].into_iter()),
        Err(UsageError)
    );
    assert_eq!(
        parse_args(["--json", "--json"].into_iter()),
        Err(UsageError)
    );
    assert_eq!(
        parse_args(["--selftest", "--selftest"].into_iter()),
        Err(UsageError)
    );
}

#[test]
fn extra_or_unknown_arguments_are_rejected() {
    assert_eq!(parse_args(["x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--", "x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--bogus"].into_iter()), Err(UsageError));
}

#[test]
fn every_state_formats() {
    for state in STATES {
        let line = format!("logd {state} 12 3\n");
        let mut out = String::new();
        format_status(&line, &mut out).unwrap();
        assert_eq!(out, format!("logd {state} 3\n"));
    }
}

#[test]
fn malformed_status_lines_are_rejected() {
    assert_eq!(format_status("", &mut String::new()), Err(StatusError));
    assert_eq!(
        format_status("logd running 1", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd dancing 1 2", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running x 2", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running 1 two", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_status("logd running 1 2 extra", &mut String::new()),
        Err(StatusError)
    );
}

#[test]
fn whitespace_is_tolerated() {
    let mut out = String::new();
    format_status("  ps   running   7   0  \n", &mut out).unwrap();
    assert_eq!(out, "ps running 0\n");
}

#[test]
fn json_record_has_the_process_envelope_and_null_unknowns() {
    let mut out = String::new();
    format_json_status("logd running 12 3\n", &mut out).unwrap();
    assert_eq!(
        out,
        "{\"schema\":1,\"kind\":\"process\",\"pid\":null,\"ppid\":null,\"uid\":null,\
         \"gid\":null,\"state\":\"running\",\"name\":\"logd\",\"command\":null,\
         \"threads\":null,\"rss_bytes\":null}\n"
    );
}

#[test]
fn json_rejects_malformed_status() {
    assert_eq!(
        format_json_status("logd dancing 1 2\n", &mut String::new()),
        Err(StatusError)
    );
    assert_eq!(
        format_json_status("logd running x 2\n", &mut String::new()),
        Err(StatusError)
    );
}

#[test]
fn parsed_status_exposes_name_state_handle_and_restarts() {
    let status = parse_status("vfsd running 5 9\n").unwrap();
    assert_eq!(status.name, "vfsd");
    assert_eq!(status.state, "running");
    assert_eq!(status.handle, 5);
    assert_eq!(status.restarts, 9);
}

#[test]
fn posix_view_is_recognised() {
    assert_eq!(parse_args(["-e"].into_iter()), Ok(Action::Posix));
    assert_eq!(parse_args(["--all"].into_iter()), Ok(Action::Posix));
    assert_eq!(parse_args(["--posix"].into_iter()), Ok(Action::Posix));
    assert_eq!(parse_args(["--services"].into_iter()), Ok(Action::List));
}

#[test]
fn posix_conflicts_with_other_modes() {
    assert_eq!(parse_args(["-e", "--json"].into_iter()), Err(UsageError));
    assert_eq!(
        parse_args(["-e", "--selftest"].into_iter()),
        Err(UsageError)
    );
    assert_eq!(
        parse_args(["-e", "--services"].into_iter()),
        Err(UsageError)
    );
}

#[test]
fn posix_rows_format_as_space_separated_fields() {
    let row = ProcessRow {
        pid: 7,
        ppid: 1,
        uid: 0,
        gid: 0,
        cmd: "rsh".to_string(),
    };
    let mut out = String::new();
    format_posix_row(&row, &mut out).unwrap();
    assert_eq!(out, "7 1 0 0 rsh\n");
    let mut header = String::new();
    format_posix_header(&mut header).unwrap();
    assert_eq!(header, "PID PPID UID GID CMD\n");
}
