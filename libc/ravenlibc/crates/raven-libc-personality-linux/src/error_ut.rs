// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GNU `error()` golden bytes (ticket C66 5.H family `error()`).

use super::*;
use alloc::vec::Vec;

#[test]
fn error_with_program_and_errnum() {
    let mut out = Vec::new();
    format_error(b"tool", 2, b"cannot open", Some(b"No such file"), &mut out);
    assert_eq!(out, b"tool: cannot open: No such file\n");
}

#[test]
fn error_without_errnum_has_no_strerror() {
    let mut out = Vec::new();
    format_error(b"tool", 0, b"plain", Some(b"ignored"), &mut out);
    assert_eq!(out, b"tool: plain\n");
}

#[test]
fn error_without_program_has_no_prefix() {
    let mut out = Vec::new();
    format_error(b"", 0, b"plain", None, &mut out);
    assert_eq!(out, b"plain\n");
}

#[test]
fn error_with_missing_strerror_still_prints_separator() {
    let mut out = Vec::new();
    format_error(b"p", 1, b"m", None, &mut out);
    assert_eq!(out, b"p: m: \n");
}

#[test]
fn at_line_formats_file_and_line() {
    let mut state = ErrorState::new();
    let mut out = Vec::new();
    let printed = format_error_at_line(
        &mut state,
        b"tool",
        0,
        b"file.c",
        42,
        b"bad value",
        None,
        &mut out,
    );
    assert!(printed);
    assert_eq!(out, b"tool:file.c:42: bad value\n");
    assert_eq!(state.message_count, 1);
}

#[test]
fn at_line_repeat_suppressed_only_with_one_per_line() {
    let mut state = ErrorState::new();
    let mut out = Vec::new();
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert_eq!(state.message_count, 2);
    state.one_per_line = 1;
    assert!(!format_error_at_line(
        &mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert_eq!(state.message_count, 2);
}

#[test]
fn at_line_different_files_are_not_suppressed() {
    let mut state = ErrorState::new();
    state.one_per_line = 1;
    let mut out = Vec::new();
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"b.c", 1, b"m", None, &mut out
    ));
    assert_eq!(state.message_count, 2);
}

#[test]
fn at_line_different_lines_are_not_suppressed() {
    let mut state = ErrorState::new();
    state.one_per_line = 1;
    let mut out = Vec::new();
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert!(format_error_at_line(
        &mut state, b"t", 0, b"a.c", 2, b"m", None, &mut out
    ));
    assert_eq!(state.message_count, 2);
}

#[test]
fn at_line_with_errnum_appends_strerror() {
    let mut state = ErrorState::new();
    let mut out = Vec::new();
    format_error_at_line(
        &mut state,
        b"t",
        13,
        b"a.c",
        7,
        b"denied",
        Some(b"Permission denied"),
        &mut out,
    );
    assert_eq!(out, b"t:a.c:7: denied: Permission denied\n");
}

#[test]
fn message_count_saturates() {
    let mut state = ErrorState::new();
    state.message_count = u32::MAX;
    let mut out = Vec::new();
    format_error_at_line(&mut state, b"t", 0, b"a.c", 1, b"m", None, &mut out);
    assert_eq!(state.message_count, u32::MAX);
}

#[test]
fn line_number_zero_and_large() {
    let mut state = ErrorState::new();
    let mut out = Vec::new();
    format_error_at_line(&mut state, b"t", 0, b"a.c", 0, b"m", None, &mut out);
    format_error_at_line(
        &mut state,
        b"t",
        0,
        b"a.c",
        4_294_967_295,
        b"m",
        None,
        &mut out,
    );
    assert_eq!(out, b"t:a.c:0: m\nt:a.c:4294967295: m\n");
}

#[test]
fn last_pair_is_recorded() {
    let mut state = ErrorState::new();
    let mut out = Vec::new();
    format_error_at_line(&mut state, b"t", 0, b"a.c", 9, b"m", None, &mut out);
    assert_eq!(state.last(), Some((&b"a.c"[..], 9)));
}
