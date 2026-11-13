// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `glob` behaviour on the path-based fake: flag matrix, limits, tilde and errors.

mod common;

use common::{Fake, FakeEnv, Kind, paths, run, run_locale};
use raven_libc_abi::generated::errno::{E2BIG, EACCES};
use raven_libc_glob::consts::*;
use raven_libc_glob::{ByteLocale, GlobState, globfree};

fn g(fs: &mut Fake, pattern: &str, flags: u32) -> (i32, Vec<String>, i32) {
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    let rv = run(fs, &env, pattern.as_bytes(), flags, &mut out);
    (rv, paths(&out), out.errno)
}

fn tree() -> Fake {
    let mut fs = Fake::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    fs.file("d/b", 2);
    fs.file("d/c", 3);
    fs.file("d/.hidden", 4);
    fs.file("top", 5);
    fs.mkdir("empty", 0o755);
    fs.symlink("lnk", "d");
    fs.symlink("broken", "nowhere");
    fs
}

#[test]
fn literal_pattern() {
    let mut fs = tree();
    let (rv, got, errno) = g(&mut fs, "top", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["top"]);
    assert_eq!(errno, 0);
}

#[test]
fn star_in_a_directory_is_sorted_in_byte_order() {
    let mut fs = Fake::new();
    fs.file("b", 1);
    fs.file("a", 1);
    fs.file("C", 1);
    let (rv, got, _) = g(&mut fs, "*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["C", "a", "b"]);
}

#[test]
fn star_does_not_match_an_initial_dot() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "d/*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b", "d/c"]);
    let (rv, got, _) = g(&mut fs, "d/.*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/.", "d/..", "d/.hidden"]);
    let (rv, got, _) = g(&mut fs, "d/.*h*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/.hidden"]);
}

#[test]
fn question_and_brackets() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "d/?", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b", "d/c"]);
    let (rv, got, _) = g(&mut fs, "d/[ab]", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b"]);
    let (rv, got, _) = g(&mut fs, "d/[!ab]", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/c"]);
    let (rv, got, _) = g(&mut fs, "d/[a-c]", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b", "d/c"]);
}

#[test]
fn multiple_segments() {
    let mut fs = Fake::new();
    fs.mkdir("a", 0o755);
    fs.mkdir("a/b", 0o755);
    fs.file("a/b/x", 1);
    fs.file("a/b/y", 1);
    let (rv, got, _) = g(&mut fs, "a/*/x", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["a/b/x"]);
    let (rv, got, _) = g(&mut fs, "*/b/*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["a/b/x", "a/b/y"]);
}

#[test]
fn glob_mark_appends_slash_only_to_directories() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "d", GLOB_MARK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/"]);
    let (rv, got, _) = g(&mut fs, "top", GLOB_MARK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["top"]);
    let (rv, got, _) = g(&mut fs, "lnk", GLOB_MARK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["lnk/"]);
    let (rv, got, _) = g(&mut fs, "broken", GLOB_MARK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["broken"]);
}

#[test]
fn glob_nosort_keeps_directory_order() {
    let mut fs = Fake::new();
    fs.file("b", 1);
    fs.file("a", 1);
    let (rv, got, _) = g(&mut fs, "?", GLOB_NOSORT);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["b", "a"]);
}

#[test]
fn nocheck_and_nomagic() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "nope*", 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
    let (rv, got, _) = g(&mut fs, "nope*", GLOB_NOCHECK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["nope*"]);
    let (rv, got, _) = g(&mut fs, "plain", GLOB_NOMAGIC);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["plain"]);
    let (rv, _got, _) = g(&mut fs, "pl*", GLOB_NOMAGIC);
    assert_eq!(rv, GLOB_NOMATCH);
}

#[test]
fn empty_pattern_is_nomatch() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "", 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
}

#[test]
fn quote_makes_a_metacharacter_literal() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "\\*", 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
    let (rv, got, _) = g(&mut fs, "\\*", GLOB_NOCHECK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["\\*"]);
    fs.file("*", 1);
    let (rv, got, _) = g(&mut fs, "\\*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["*"]);
}

#[test]
fn noescape_makes_backslash_ordinary() {
    let mut fs = tree();
    fs.file("\\x", 1);
    let (rv, got, _) = g(&mut fs, "\\x", GLOB_NOESCAPE);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["\\x"]);
    let (rv, _got, _) = g(&mut fs, "\\x", 0);
    assert_eq!(rv, GLOB_NOMATCH);
}

#[test]
fn magchar_is_set_and_cleared() {
    let mut fs = tree();
    let mut out = GlobState::default();
    let env = FakeEnv::new();
    assert_eq!(run(&mut fs, &env, b"top", 0, &mut out), 0);
    assert_eq!(out.flags & GLOB_MAGCHAR, 0);
    assert_eq!(run(&mut fs, &env, b"*", 0, &mut out), 0);
    assert_ne!(out.flags & GLOB_MAGCHAR, 0);
}

#[test]
fn append_sorts_only_the_new_part() {
    let mut fs = Fake::new();
    fs.file("z1", 1);
    fs.file("a1", 1);
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    assert_eq!(run(&mut fs, &env, b"z*", 0, &mut out), 0);
    assert_eq!(paths(&out), vec!["z1"]);
    assert_eq!(run(&mut fs, &env, b"a*", GLOB_APPEND, &mut out), 0);
    assert_eq!(paths(&out), vec!["z1", "a1"]);
}

#[test]
fn append_without_previous_result_starts_empty() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    assert_eq!(run(&mut fs, &env, b"top", GLOB_APPEND, &mut out), 0);
    assert_eq!(paths(&out), vec!["top"]);
}

#[test]
fn dooffs_keeps_or_resets_the_offset_count() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState {
        offs: 3,
        ..GlobState::default()
    };
    assert_eq!(run(&mut fs, &env, b"top", GLOB_DOOFFS, &mut out), 0);
    assert_eq!(out.offs, 3);
    assert_eq!(run(&mut fs, &env, b"top", 0, &mut out), 0);
    assert_eq!(out.offs, 0);
}

#[test]
fn altdirfunc_flag_is_accepted() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "d/[ab]", GLOB_ALTDIRFUNC);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b"]);
}

#[test]
fn brace_expansion() {
    let mut fs = tree();
    fs.file("d/ax", 1);
    fs.file("d/bx", 1);
    let (rv, got, _) = g(&mut fs, "d/{a,b}", GLOB_BRACE);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b"]);
    let (rv, got, _) = g(&mut fs, "d/{a,b}{,x}", GLOB_BRACE);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/ax", "d/b", "d/bx"]);
    let (rv, got, _) = g(&mut fs, "d/{b,a}", GLOB_BRACE);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["d/a", "d/b"]);
}

#[test]
fn brace_single_pair_is_literal() {
    let mut fs = tree();
    fs.file("{}", 1);
    let (rv, got, _) = g(&mut fs, "{}", GLOB_BRACE);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["{}"]);
}

#[test]
fn brace_limit_applies_with_the_flag() {
    let mut fs = tree();
    let mut pattern = String::new();
    for _ in 0..200 {
        pattern.push_str("{a,");
    }
    pattern.push('b');
    for _ in 0..200 {
        pattern.push('}');
    }
    let (rv, _got, errno) = g(&mut fs, &pattern, GLOB_BRACE | GLOB_LIMIT);
    assert_eq!(rv, GLOB_NOSPACE);
    assert_eq!(errno, E2BIG);
}

#[test]
fn brace_limit_applies_without_the_flag_h_gl_1() {
    let mut fs = tree();
    let mut pattern = String::new();
    for _ in 0..200 {
        pattern.push_str("{a,");
    }
    pattern.push('b');
    for _ in 0..200 {
        pattern.push('}');
    }
    let (rv, _got, _errno) = g(&mut fs, &pattern, GLOB_BRACE);
    assert_eq!(rv, GLOB_NOSPACE);
}

#[test]
fn pattern_longer_than_path_max_is_nomatch() {
    let mut fs = tree();
    let pattern = "a".repeat(4096);
    let (rv, got, _) = g(&mut fs, &pattern, 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
}

#[test]
fn matchc_is_an_input_path_limit_under_glob_limit() {
    let mut fs = Fake::new();
    for i in 0..10 {
        fs.file(format!("f{i}"), 1);
    }
    let env = FakeEnv::new();
    let mut out = GlobState {
        matchc: 3,
        ..GlobState::default()
    };
    let rv = run(&mut fs, &env, b"f*", GLOB_LIMIT, &mut out);
    assert_eq!(rv, GLOB_NOSPACE);
    assert_eq!(out.errno, E2BIG);
}

#[test]
fn readdir_limit_stops_at_16384_entries() {
    let mut fs = Fake::new();
    fs.mkdir("d", 0o755);
    for i in 0..20000 {
        fs.file(format!("d/n{i}"), 1);
    }
    let (rv, _got, errno) = g(&mut fs, "d/nomatch*", GLOB_LIMIT);
    assert_eq!(rv, GLOB_NOSPACE);
    assert_eq!(errno, E2BIG);
}

#[test]
fn stat_limit_stops_at_1024_matches() {
    let mut fs = Fake::new();
    fs.mkdir("d", 0o755);
    for i in 0..2000 {
        fs.file(format!("d/f{i}"), 1);
    }
    let (rv, _got, errno) = g(&mut fs, "d/f*", GLOB_LIMIT);
    assert_eq!(rv, GLOB_NOSPACE);
    assert_eq!(errno, E2BIG);
}

#[test]
fn string_limit_stops_at_131072_bytes() {
    let mut fs = Fake::new();
    fs.mkdir("d", 0o755);
    let long = "a".repeat(200);
    for i in 0..1024 {
        fs.file(format!("d/{long}{i}"), 1);
    }
    let (rv, _got, errno) = g(&mut fs, "d/aaa*", GLOB_LIMIT);
    assert_eq!(rv, GLOB_NOSPACE);
    assert_eq!(errno, E2BIG);
}

#[test]
fn globfree_is_idempotent() {
    let mut fs = tree();
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    assert_eq!(run(&mut fs, &env, b"d/*", 0, &mut out), 0);
    assert_eq!(out.pathv.len(), 3);
    globfree(&mut out);
    assert!(out.pathv.is_empty());
    globfree(&mut out);
    assert!(out.pathv.is_empty());
}

#[test]
fn invalid_utf8_names_are_kept_literal_in_the_byte_locale() {
    let mut fs = Fake::new();
    fs.file(b"\xff.bin", 1);
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    let rv = run_locale(&mut fs, &env, &ByteLocale, b"*.bin", 0, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(out.pathv, vec![b"\xff.bin".to_vec()]);
}

#[test]
fn error_callback_can_abort() {
    let mut fs = tree();
    fs.chmod("d", 0o000);
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    let mut seen: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut hook = |path: &[u8], err: i32| {
        seen.push((path.to_vec(), err));
        1
    };
    let rv = raven_libc_glob::glob(
        &mut fs,
        &env,
        &raven_libc_glob::Utf8Locale,
        b"d/*",
        0,
        Some(&mut hook),
        &mut out,
    );
    assert_eq!(rv, GLOB_ABORTED);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, b"d/");
    assert_eq!(seen[0].1, EACCES);
}

#[test]
fn glob_err_aborts_on_an_unreadable_directory() {
    let mut fs = tree();
    fs.chmod("d", 0o000);
    let (rv, _got, _errno) = g(&mut fs, "d/*", GLOB_ERR);
    assert_eq!(rv, GLOB_ABORTED);
}

#[test]
fn unreadable_directory_without_err_is_skipped() {
    let mut fs = tree();
    fs.chmod("d", 0o000);
    let (rv, got, _) = g(&mut fs, "d/*", 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
}

#[test]
fn tilde_uses_home_from_the_environment_seam() {
    let mut fs = Fake::new();
    fs.mkdir("home", 0o755);
    fs.mkdir("home/me", 0o755);
    fs.file("home/me/x", 1);
    let env = FakeEnv::with_home(b"/home/me");
    let mut out = GlobState::default();
    let rv = run(&mut fs, &env, b"~/x", GLOB_TILDE, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["/home/me/x"]);
    assert_eq!(env.home_calls.get(), 1);
}

#[test]
fn tilde_falls_back_to_login_then_passwd() {
    let mut fs = Fake::new();
    fs.mkdir("home", 0o755);
    fs.mkdir("home/alice", 0o755);
    fs.file("home/alice/x", 1);
    let mut env = FakeEnv::new();
    env.login = Some(b"alice".to_vec());
    env.passwd
        .insert(b"alice".to_vec(), b"/home/alice".to_vec());
    let mut out = GlobState::default();
    let rv = run(&mut fs, &env, b"~/x", GLOB_TILDE, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["/home/alice/x"]);
}

#[test]
fn tilde_falls_back_to_getpwuid() {
    let mut fs = Fake::new();
    fs.mkdir("home", 0o755);
    fs.mkdir("home/me", 0o755);
    fs.file("home/me/x", 1);
    let mut env = FakeEnv::new();
    env.uid_dir = Some(b"/home/me".to_vec());
    let mut out = GlobState::default();
    let rv = run(&mut fs, &env, b"~/x", GLOB_TILDE, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["/home/me/x"]);
}

#[test]
fn tilde_user_and_unknown_user() {
    let mut fs = Fake::new();
    fs.mkdir("home", 0o755);
    fs.mkdir("home/bob", 0o755);
    fs.file("home/bob/x", 1);
    let mut env = FakeEnv::new();
    env.passwd.insert(b"bob".to_vec(), b"/home/bob".to_vec());
    let mut out = GlobState::default();
    let rv = run(&mut fs, &env, b"~bob/x", GLOB_TILDE, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["/home/bob/x"]);

    let mut out = GlobState::default();
    let rv = run(
        &mut fs,
        &env,
        b"~nouser/x",
        GLOB_TILDE | GLOB_NOCHECK,
        &mut out,
    );
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["~nouser/x"]);
}

#[test]
fn tilde_does_not_run_without_the_flag() {
    let mut fs = Fake::new();
    fs.file("~", 1);
    let env = FakeEnv::with_home(b"/home/me");
    let mut out = GlobState::default();
    let rv = run(&mut fs, &env, b"~", 0, &mut out);
    assert_eq!(rv, 0);
    assert_eq!(paths(&out), vec!["~"]);
    assert_eq!(env.home_calls.get(), 0);
}

#[test]
fn symlink_segments_are_followed() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "lnk/a", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["lnk/a"]);
}

#[test]
fn broken_symlink_is_a_result_not_an_error() {
    let mut fs = tree();
    let (rv, got, _) = g(&mut fs, "b*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["broken"]);
}

#[test]
fn special_node_types_are_results() {
    let mut fs = Fake::new();
    fs.node("fifo", Kind::Fifo);
    fs.node("sock", Kind::Socket);
    fs.node("chr", Kind::Char);
    fs.node("blk", Kind::Block);
    let (rv, got, _) = g(&mut fs, "*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["blk", "chr", "fifo", "sock"]);
}
