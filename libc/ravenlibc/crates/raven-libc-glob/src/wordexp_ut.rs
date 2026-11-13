// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `WRDE_NOCMD` pre-scan and the output splitter.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;
use crate::consts::{WRDE_NOCMD, WRDE_SHOWERR};

const NOCMD: u32 = WRDE_NOCMD;

fn scan(s: &str) -> i32 {
    prescan(s.as_bytes())
}

#[test]
fn plain_words_pass() {
    assert_eq!(scan("a b c"), 0);
    assert_eq!(scan(""), 0);
    assert_eq!(scan("$HOME/x"), 0);
    // RavenBSD: musl's pre-scan rejects `{` outside quotes, so `${HOME}` is WRDE_BADCHAR.
    assert_eq!(scan("${HOME}/x"), WRDE_BADCHAR);
}

#[test]
fn special_characters_outside_quotes_are_badchar() {
    for s in ["a|b", "a&b", "a;b", "a<b", "a>b", "a{b}", "a\nb"] {
        assert_eq!(scan(s), WRDE_BADCHAR, "{s}");
    }
    assert_eq!(scan("(a)"), WRDE_BADCHAR);
    assert_eq!(scan(")"), WRDE_BADCHAR);
}

#[test]
fn special_characters_inside_quotes_pass() {
    assert_eq!(scan("'a|b'"), 0);
    assert_eq!(scan("\"a&b\""), 0);
    assert_eq!(scan("'a(b)'"), 0);
    assert_eq!(scan("a(b)"), WRDE_BADCHAR);
}

#[test]
fn arithmetic_parentheses_are_tracked() {
    assert_eq!(scan("$((1+2))"), 0);
    assert_eq!(scan("$((1+(2)))"), 0);
    assert_eq!(scan("$(echo x)"), WRDE_CMDSUB);
    assert_eq!(scan("`echo x`"), WRDE_CMDSUB);
    assert_eq!(scan("'`echo x`'"), 0);
    assert_eq!(scan("\"`echo x`\""), WRDE_CMDSUB);
}

#[test]
fn backslash_at_end_is_syntax() {
    assert_eq!(scan("\\"), WRDE_SYNTAX);
    assert_eq!(scan("'\\'"), 0);
    assert_eq!(scan("a\\ b"), 0);
}

#[test]
fn sentinel_is_dropped_and_words_split() -> Result<(), i32> {
    assert_eq!(
        parse_output(b"x\x00a\x00b\x00")?,
        vec![b"a".to_vec(), b"b".to_vec()]
    );
    assert_eq!(parse_output(b"x\x00")?, Vec::<Vec<u8>>::new());
    assert_eq!(parse_output(b"x\x00a")?, vec![b"a".to_vec()]);
    assert_eq!(
        parse_output(b"x\x00\x00a")?,
        vec![Vec::new(), b"a".to_vec()]
    );
    assert_eq!(parse_output(b""), Err(WRDE_SYNTAX));
    assert_eq!(parse_output(b"x"), Err(WRDE_SYNTAX));
    Ok(())
}

struct Recorder {
    output: Vec<u8>,
    seen: Vec<(Vec<u8>, Vec<u8>, bool)>,
    fail: bool,
}

impl ShellRun for Recorder {
    fn run(&mut self, script: &[u8], arg: &[u8], show_stderr: bool) -> Result<Vec<u8>, Errno> {
        self.seen.push((script.to_vec(), arg.to_vec(), show_stderr));
        if self.fail {
            return Err(Errno(12));
        }
        Ok(self.output.clone())
    }
}

fn run_with(output: &[u8], flags: u32, out: &mut WordExp) -> i32 {
    let mut recorder = Recorder {
        output: output.to_vec(),
        seen: Vec::new(),
        fail: false,
    };
    wordexp(&mut recorder, b"a b", flags, out)
}

#[test]
fn wordexp_uses_the_donor_script_and_drops_the_sentinel() {
    let mut out = WordExp::default();
    assert_eq!(run_with(b"x\x00one\x00two\x00", 0, &mut out), 0);
    assert_eq!(out.words, vec![b"one".to_vec(), b"two".to_vec()]);
    let mut recorder = Recorder {
        output: b"x\x00a\x00".to_vec(),
        seen: Vec::new(),
        fail: false,
    };
    assert_eq!(wordexp(&mut recorder, b"hello", 0, &mut out), 0);
    let (script, arg, showerr) = &recorder.seen[0];
    assert_eq!(script, crate::consts::WORDEXP_SCRIPT);
    assert_eq!(arg, b"hello");
    assert!(!showerr);
}

#[test]
fn flags_append_reuse_dooffs_and_showerr() {
    let mut out = WordExp::default();
    assert_eq!(run_with(b"x\x00a\x00", 0, &mut out), 0);
    assert_eq!(run_with(b"x\x00b\x00", WRDE_APPEND, &mut out), 0);
    assert_eq!(out.words, vec![b"a".to_vec(), b"b".to_vec()]);
    assert_eq!(
        run_with(b"x\x00c\x00", WRDE_REUSE | WRDE_APPEND, &mut out),
        0
    );
    assert_eq!(out.words, vec![b"c".to_vec()]);
    let mut recorder = Recorder {
        output: b"x\x00a\x00".to_vec(),
        seen: Vec::new(),
        fail: false,
    };
    out.offs = 4;
    assert_eq!(
        wordexp(&mut recorder, b"a", WRDE_DOOFFS | WRDE_SHOWERR, &mut out),
        0
    );
    assert_eq!(out.offs, 4);
    assert!(recorder.seen[0].2);
    assert_eq!(wordexp(&mut recorder, b"a", 0, &mut out), 0);
    assert_eq!(out.offs, 0);
}

#[test]
fn prescan_rejects_before_running_the_shell() {
    let mut recorder = Recorder {
        output: b"x\x00a\x00".to_vec(),
        seen: Vec::new(),
        fail: false,
    };
    let mut out = WordExp::default();
    assert_eq!(
        wordexp(&mut recorder, b"$(x)", NOCMD, &mut out),
        WRDE_CMDSUB
    );
    assert!(recorder.seen.is_empty());
}

#[test]
fn shell_failure_is_nospace() {
    let mut recorder = Recorder {
        output: Vec::new(),
        seen: Vec::new(),
        fail: true,
    };
    let mut out = WordExp {
        words: vec![b"old".to_vec()],
        offs: 0,
    };
    assert_eq!(wordexp(&mut recorder, b"a", 0, &mut out), WRDE_NOSPACE);
    assert!(out.words.is_empty());
}
