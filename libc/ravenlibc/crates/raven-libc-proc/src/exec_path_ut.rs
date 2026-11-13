// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `execvpe` path search.

#![allow(clippy::arithmetic_side_effects)] // test path construction

use super::*;
use crate::errors::{E2BIG, EISDIR, ELOOP, ENAMETOOLONG, ENOMEM, ENOTDIR, ETXTBSY};
use crate::errors::{EIO, EPERM};
use alloc::vec;
use alloc::vec::Vec;

struct Run {
    result: Result<u8, Errno>,
    attempts: Vec<(Vec<u8>, bool)>,
    diags: Vec<Vec<u8>>,
}

fn run(file: &[u8], path: Option<&[u8]>, results: Vec<Result<u8, Errno>>, prefix: &[u8]) -> Run {
    let mut results = results.into_iter();
    let mut attempts = Vec::new();
    let mut diags = Vec::new();
    let result = search_path(
        file,
        path,
        prefix,
        &mut |attempt| {
            attempts.push((attempt.path.to_vec(), attempt.shell_fallback));
            results.next().unwrap_or(Err(ENOENT))
        },
        &mut |message| diags.push(message.to_vec()),
    );
    Run {
        result,
        attempts,
        diags,
    }
}

#[test]
fn an_empty_name_is_enoent_without_an_attempt() {
    let outcome = run(b"", Some(b"/bin"), vec![], b"execvp");
    assert_eq!(outcome.result, Err(ENOENT));
    assert!(outcome.attempts.is_empty());
}

#[test]
fn an_absolute_name_is_tried_once() {
    let outcome = run(b"/bin/x", Some(b"/a:/b"), vec![Ok(7)], b"execvp");
    assert_eq!(outcome.result, Ok(7));
    assert_eq!(outcome.attempts, vec![(b"/bin/x".to_vec(), false)]);
}

#[test]
fn a_relative_path_name_is_tried_once() {
    let outcome = run(b"./x", Some(b"/a"), vec![Ok(7)], b"execvp");
    assert_eq!(outcome.attempts, vec![(b"./x".to_vec(), false)]);
}

#[test]
fn a_slash_in_the_name_disables_the_search() {
    let outcome = run(b"a/b", Some(b"/a:/b"), vec![Ok(7)], b"execvp");
    assert_eq!(outcome.attempts, vec![(b"a/b".to_vec(), false)]);
}

#[test]
fn an_empty_path_element_means_current_directory() {
    let outcome = run(b"x", Some(b""), vec![Ok(1)], b"execvp");
    assert_eq!(outcome.attempts, vec![(b"./x".to_vec(), false)]);
}

#[test]
fn a_colon_yields_two_current_directory_elements() {
    let outcome = run(b"x", Some(b":"), vec![Err(ENOENT), Ok(1)], b"execvp");
    assert_eq!(
        outcome.attempts,
        vec![(b"./x".to_vec(), false), (b"./x".to_vec(), false)]
    );
}

#[test]
fn a_double_colon_yields_three_current_directory_elements() {
    let outcome = run(
        b"x",
        Some(b"::"),
        vec![Err(ENOENT), Err(ENOENT), Ok(1)],
        b"execvp",
    );
    assert_eq!(outcome.attempts.len(), 3);
}

#[test]
fn interior_empty_elements_are_current_directory() {
    let outcome = run(
        b"x",
        Some(b"a::b"),
        vec![Err(ENOENT), Err(ENOENT), Ok(1)],
        b"execvp",
    );
    assert_eq!(
        outcome.attempts,
        vec![
            (b"a/x".to_vec(), false),
            (b"./x".to_vec(), false),
            (b"b/x".to_vec(), false)
        ]
    );
}

#[test]
fn a_leading_colon_is_current_directory() {
    let outcome = run(b"x", Some(b":a"), vec![Err(ENOENT), Ok(1)], b"execvp");
    assert_eq!(
        outcome.attempts,
        vec![(b"./x".to_vec(), false), (b"a/x".to_vec(), false)]
    );
}

#[test]
fn a_trailing_colon_is_current_directory() {
    let outcome = run(b"x", Some(b"a:"), vec![Err(ENOENT), Ok(1)], b"execvp");
    assert_eq!(
        outcome.attempts,
        vec![(b"a/x".to_vec(), false), (b"./x".to_vec(), false)]
    );
}

#[test]
fn an_unset_path_uses_defpath() {
    let outcome = run(b"x", None, vec![Err(ENOENT), Ok(1)], b"execvp");
    assert_eq!(
        outcome.attempts,
        vec![(b"/bin/x".to_vec(), false), (b"/usr/bin/x".to_vec(), false)]
    );
    assert_eq!(DEFPATH, b"/bin:/usr/bin");
}

#[test]
fn a_too_long_element_is_reported_and_skipped() {
    let long = vec![b'a'; 5000];
    let mut path = long.clone();
    path.push(b':');
    path.extend_from_slice(b"b");
    let outcome = run(b"x", Some(&path), vec![Ok(1)], b"execvp");
    assert_eq!(outcome.attempts, vec![(b"b/x".to_vec(), false)]);
    assert_eq!(outcome.diags.len(), 1);
    let message = &outcome.diags[0];
    assert!(message.starts_with(b"execvp: "));
    assert!(message.ends_with(b": path too long\n"));
    assert!(
        message
            .windows(long.len())
            .any(|window| window == long.as_slice())
    );
}

#[test]
fn an_element_with_a_newline_is_used_verbatim() {
    let outcome = run(b"x", Some(b"a\nb"), vec![Ok(1)], b"execvp");
    assert_eq!(outcome.attempts, vec![(b"a\nb/x".to_vec(), false)]);
}

#[test]
fn e2big_aborts_at_once() {
    let outcome = run(b"x", Some(b"a:b"), vec![Err(E2BIG)], b"execvp");
    assert_eq!(outcome.result, Err(E2BIG));
    assert_eq!(outcome.attempts.len(), 1);
}

#[test]
fn enomem_aborts_at_once() {
    let outcome = run(b"x", Some(b"a:b"), vec![Err(ENOMEM)], b"execvp");
    assert_eq!(outcome.result, Err(ENOMEM));
    assert_eq!(outcome.attempts.len(), 1);
}

#[test]
fn etxtbsy_aborts_at_once() {
    let outcome = run(b"x", Some(b"a:b"), vec![Err(ETXTBSY)], b"execvp");
    assert_eq!(outcome.result, Err(ETXTBSY));
    assert_eq!(outcome.attempts.len(), 1);
}

#[test]
fn an_unknown_error_aborts_at_once() {
    let outcome = run(b"x", Some(b"a:b"), vec![Err(EIO)], b"execvp");
    assert_eq!(outcome.result, Err(EIO));
    assert_eq!(outcome.attempts.len(), 1);
}

#[test]
fn continue_errors_try_the_next_element() {
    for error in [EISDIR, ELOOP, ENAMETOOLONG, ENOENT, ENOTDIR] {
        let outcome = run(b"x", Some(b"a:b"), vec![Err(error), Ok(9)], b"execvp");
        assert_eq!(outcome.result, Ok(9), "error {error}");
        assert_eq!(outcome.attempts.len(), 2);
    }
}

#[test]
fn eacces_is_remembered_and_returned_at_the_end() {
    let outcome = run(
        b"x",
        Some(b"a:b"),
        vec![Err(EACCES), Err(ENOENT)],
        b"execvp",
    );
    assert_eq!(outcome.result, Err(EACCES));
}

#[test]
fn success_after_eacces_wins() {
    let outcome = run(b"x", Some(b"a:b"), vec![Err(EACCES), Ok(4)], b"execvp");
    assert_eq!(outcome.result, Ok(4));
}

#[test]
fn enoexec_gets_one_shell_fallback_attempt() {
    let outcome = run(b"x", Some(b"a"), vec![Err(ENOEXEC), Ok(5)], b"execvp");
    assert_eq!(outcome.result, Ok(5));
    assert_eq!(
        outcome.attempts,
        vec![(b"a/x".to_vec(), false), (b"a/x".to_vec(), true)]
    );
}

#[test]
fn a_failed_shell_fallback_returns_its_errno() {
    let outcome = run(b"x", Some(b"a"), vec![Err(ENOEXEC), Err(EPERM)], b"execvp");
    assert_eq!(outcome.result, Err(EPERM));
    assert_eq!(outcome.attempts.len(), 2);
}

#[test]
fn script_argv_replaces_argv_zero() {
    let argv = vec![b"script".to_vec(), b"one".to_vec(), b"two".to_vec()];
    assert_eq!(
        script_argv(&argv, b"/tmp/script"),
        vec![
            b"sh".to_vec(),
            b"/tmp/script".to_vec(),
            b"one".to_vec(),
            b"two".to_vec()
        ]
    );
}

#[test]
fn script_argv_handles_an_empty_argv() {
    assert_eq!(
        script_argv(&[], b"/tmp/script"),
        vec![b"sh".to_vec(), b"/tmp/script".to_vec()]
    );
}

#[test]
fn a_three_hundred_byte_name_is_accepted() {
    let name = vec![b'n'; 300];
    let outcome = run(&name, Some(b"a"), vec![Ok(1)], b"execvp");
    assert_eq!(outcome.attempts.len(), 1);
    assert_eq!(outcome.attempts[0].0.len(), 302);
}
