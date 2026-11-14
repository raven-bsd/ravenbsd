// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setproctitle` prefix, restore and fast-path rules (ticket C66 5.H family).

use super::*;
use crate::testutil::FakeProc;

#[test]
fn prefix_is_added() {
    assert_eq!(
        format_title(b"prog", Some(b"work")),
        Some(b"prog: work".to_vec())
    );
}

#[test]
fn leading_dash_suppresses_the_prefix() {
    assert_eq!(format_title(b"prog", Some(b"-raw")), Some(b"raw".to_vec()));
}

#[test]
fn null_format_restores() {
    assert_eq!(format_title(b"prog", None), None);
}

#[test]
fn empty_program_means_no_prefix() {
    assert_eq!(format_title(b"", Some(b"text")), Some(b"text".to_vec()));
}

#[test]
fn title_is_truncated_to_255_bytes() {
    let long = crate::testutil::repeated_bytes(b'x', 400);
    let title = format_title(b"p", Some(&long)).unwrap();
    assert_eq!(title.len(), MAX_TITLE);
}

#[test]
fn set_title_is_forwarded() {
    let mut proc = FakeProc::new();
    let mut cache = Cache::new();
    assert_eq!(
        setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), false),
        Ok(())
    );
    assert_eq!(proc.title.as_deref(), Some(&b"p: x"[..]));
    assert_eq!(cache.last(), Some(&b"p: x"[..]));
}

#[test]
fn null_restores_and_clears_the_cache() {
    let mut proc = FakeProc::new();
    let mut cache = Cache::new();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), false).unwrap();
    setproctitle(&mut proc, &mut cache, b"p", None, false).unwrap();
    assert_eq!(proc.title, None);
    assert_eq!(cache.last(), None);
    assert_eq!(proc.titles.len(), 2);
}

#[test]
fn fast_path_skips_an_unchanged_title() {
    let mut proc = FakeProc::new();
    let mut cache = Cache::new();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), true).unwrap();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), true).unwrap();
    assert_eq!(proc.titles.len(), 1);
}

#[test]
fn slow_path_always_calls() {
    let mut proc = FakeProc::new();
    let mut cache = Cache::new();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), false).unwrap();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), false).unwrap();
    assert_eq!(proc.titles.len(), 2);
}

#[test]
fn fast_path_calls_again_after_a_change() {
    let mut proc = FakeProc::new();
    let mut cache = Cache::new();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), true).unwrap();
    setproctitle(&mut proc, &mut cache, b"p", Some(b"y"), true).unwrap();
    assert_eq!(proc.titles.len(), 2);
    assert_eq!(proc.title.as_deref(), Some(&b"p: y"[..]));
}

#[test]
fn a_backend_error_does_not_change_the_cache() {
    let mut proc = FakeProc::new();
    proc.title_error = Some(Errno::ENOSYS);
    let mut cache = Cache::new();
    assert_eq!(
        setproctitle(&mut proc, &mut cache, b"p", Some(b"x"), false),
        Err(Errno::ENOSYS)
    );
    assert_eq!(cache.last(), None);
}
