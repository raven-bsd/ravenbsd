// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Named flag and corner cases for `glob`.

mod common;

use common::{Fake, FakeEnv, Kind, paths};
use raven_libc_glob::consts::*;
use raven_libc_glob::{GlobState, Utf8Locale};

fn g(fs: &mut Fake, pattern: &str, flags: u32) -> (i32, Vec<String>) {
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    let rv = raven_libc_glob::glob(
        fs,
        &env,
        &Utf8Locale,
        pattern.as_bytes(),
        flags,
        None,
        &mut out,
    );
    (rv, paths(&out))
}

fn tree() -> Fake {
    let mut fs = Fake::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    fs.file("d/b", 2);
    fs.file("d/.h", 3);
    fs.file("top", 4);
    fs
}

#[test]
fn a_literal_existing_path() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "top", 0), (0, vec!["top".to_string()]));
}

#[test]
fn a_literal_missing_path_is_nomatch() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "missing", 0).0, GLOB_NOMATCH);
}

#[test]
fn a_star_in_the_current_directory() {
    let mut fs = tree();
    assert_eq!(
        g(&mut fs, "*", 0),
        (0, vec!["d".to_string(), "top".to_string()])
    );
}

#[test]
fn a_star_skips_the_initial_dot() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/*", 0).1, vec!["d/a", "d/b"]);
}

#[test]
fn a_literal_dot_pattern_matches_dot_names() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/.h", 0).1, vec!["d/.h"]);
}

#[test]
fn a_question_matches_one_name_character() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/?", 0).1, vec!["d/a", "d/b"]);
}

#[test]
fn a_range_matches_its_letters() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/[a-b]", 0).1, vec!["d/a", "d/b"]);
}

#[test]
fn a_negated_range_excludes_its_letters() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/[!a]", 0).1, vec!["d/b"]);
}

#[test]
fn mark_marks_a_directory_but_not_a_file() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d", GLOB_MARK).1, vec!["d/"]);
    assert_eq!(g(&mut fs, "top", GLOB_MARK).1, vec!["top"]);
}

#[test]
fn nosort_keeps_the_fake_creation_order() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/[ab]", GLOB_NOSORT).1, vec!["d/a", "d/b"]);
}

#[test]
fn nocheck_returns_the_pattern() {
    let mut fs = tree();
    assert_eq!(
        g(&mut fs, "no*", GLOB_NOCHECK),
        (0, vec!["no*".to_string()])
    );
}

#[test]
fn nomagic_returns_a_magic_free_pattern_only() {
    let mut fs = tree();
    assert_eq!(
        g(&mut fs, "plain", GLOB_NOMAGIC),
        (0, vec!["plain".to_string()])
    );
    assert_eq!(g(&mut fs, "p*", GLOB_NOMAGIC).0, GLOB_NOMATCH);
}

#[test]
fn quote_protects_a_star() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "\\*", GLOB_NOCHECK).1, vec!["\\*"]);
}

#[test]
fn noescape_makes_backslash_literal() {
    let mut fs = Fake::new();
    fs.file("\\x", 1);
    assert_eq!(g(&mut fs, "\\x", GLOB_NOESCAPE).1, vec!["\\x"]);
    assert_eq!(g(&mut fs, "\\x", 0).0, GLOB_NOMATCH);
}

#[test]
fn magchar_reports_a_bracket() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    assert_eq!(
        raven_libc_glob::glob(&mut fs, &env, &Utf8Locale, b"d/[ab]", 0, None, &mut out),
        0
    );
    assert_ne!(out.flags & GLOB_MAGCHAR, 0);
}

#[test]
fn append_keeps_the_old_results() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    assert_eq!(
        raven_libc_glob::glob(&mut fs, &env, &Utf8Locale, b"top", 0, None, &mut out),
        0
    );
    assert_eq!(
        raven_libc_glob::glob(
            &mut fs,
            &env,
            &Utf8Locale,
            b"d/a",
            GLOB_APPEND,
            None,
            &mut out
        ),
        0
    );
    assert_eq!(paths(&out), vec!["top", "d/a"]);
}

#[test]
fn brace_expands_and_sorts() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/{b,a}", GLOB_BRACE).1, vec!["d/a", "d/b"]);
}

#[test]
fn a_quoted_comma_is_not_a_brace_separator() {
    let mut fs = Fake::new();
    fs.file("a,b", 1);
    // A brace group with no unprotected comma is still one alternative: `{a\,b}` expands to the
    // literal protected comma name, while `{a,b}` is two.
    assert_eq!(g(&mut fs, "a\\,b", 0).1, vec!["a,b"]);
    assert_eq!(g(&mut fs, "{a\\,b}", GLOB_BRACE).1, vec!["a,b"]);
}

#[test]
fn tilde_with_a_slash_uses_home() {
    let mut fs = Fake::new();
    fs.mkdir("h", 0o755);
    fs.file("h/x", 1);
    let env = FakeEnv::with_home(b"/h");
    let mut out = GlobState::default();
    assert_eq!(
        raven_libc_glob::glob(
            &mut fs,
            &env,
            &Utf8Locale,
            b"~/x",
            GLOB_TILDE,
            None,
            &mut out
        ),
        0
    );
    assert_eq!(paths(&out), vec!["/h/x"]);
}

#[test]
fn tilde_without_a_tail() {
    let mut fs = Fake::new();
    fs.mkdir("h", 0o755);
    let env = FakeEnv::with_home(b"/h");
    let mut out = GlobState::default();
    assert_eq!(
        raven_libc_glob::glob(&mut fs, &env, &Utf8Locale, b"~", GLOB_TILDE, None, &mut out),
        0
    );
    assert_eq!(paths(&out), vec!["/h"]);
}

#[test]
fn dooffs_with_append_keeps_the_offset() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState {
        offs: 2,
        ..GlobState::default()
    };
    assert_eq!(
        raven_libc_glob::glob(
            &mut fs,
            &env,
            &Utf8Locale,
            b"top",
            GLOB_DOOFFS | GLOB_APPEND,
            None,
            &mut out
        ),
        0
    );
    assert_eq!(out.offs, 2);
}

#[test]
fn a_special_node_type_is_a_result() {
    let mut fs = Fake::new();
    fs.node("fifo", Kind::Fifo);
    assert_eq!(g(&mut fs, "*", 0).1, vec!["fifo"]);
}

#[test]
fn a_symlink_is_a_result_and_mark_follows_it() {
    let mut fs = tree();
    fs.symlink("lnk", "d");
    assert_eq!(g(&mut fs, "lnk", GLOB_MARK).1, vec!["lnk/"]);
}

#[test]
fn altdirfunc_flag_is_ignored_on_the_seam() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/a", GLOB_ALTDIRFUNC).1, vec!["d/a"]);
}

#[test]
fn globfree_twice_on_an_empty_state() {
    let mut out = GlobState::default();
    raven_libc_glob::globfree(&mut out);
    raven_libc_glob::globfree(&mut out);
    assert!(out.pathv.is_empty());
}

#[test]
fn a_pattern_with_no_slash_in_a_subdirectory_context() {
    let mut fs = tree();
    fs.mkdir("x", 0o755);
    fs.file("x/y", 1);
    assert_eq!(g(&mut fs, "x/y", 0).1, vec!["x/y"]);
}

#[test]
fn a_trailing_slash_in_the_pattern() {
    let mut fs = tree();
    assert_eq!(g(&mut fs, "d/", 0).1, vec!["d/"]);
}

#[test]
fn an_empty_directory_matches_nothing() {
    let mut fs = tree();
    fs.mkdir("empty", 0o755);
    assert_eq!(g(&mut fs, "empty/*", 0).0, GLOB_NOMATCH);
}

#[test]
fn a_dotdot_segment_is_literal() {
    let mut fs = tree();
    fs.mkdir("d/sub", 0o755);
    assert_eq!(g(&mut fs, "d/sub/../a", 0).1, vec!["d/sub/../a"]);
}
