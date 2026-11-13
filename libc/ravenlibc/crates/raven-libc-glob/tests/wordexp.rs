// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wordexp` through the host `/bin/sh` (the donor's integration contract).
//!
//! `ShellRun` is implemented with `std::process::Command` here; the shipped implementation is
//! the process crate's `posix_spawn` version. The FreeBSD `wordexp_test.c` file is absent from
//! the pinned sparse checkout, so these cases are the POSIX behaviours the ticket lists plus
//! the donor's pre-scan table.

use std::process::Command;

use raven_libc_abi::errno::Errno;
use raven_libc_glob::consts::*;
use raven_libc_glob::{ShellRun, WordExp, wordexp};

struct HostShell {
    script: Vec<u8>,
    arg: Vec<u8>,
    showerr: bool,
    env: Vec<(String, String)>,
}

impl HostShell {
    fn new() -> Self {
        Self {
            script: Vec::new(),
            arg: Vec::new(),
            showerr: false,
            env: Vec::new(),
        }
    }

    fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }
}

impl ShellRun for HostShell {
    fn run(&mut self, script: &[u8], arg: &[u8], show_stderr: bool) -> Result<Vec<u8>, Errno> {
        self.script = script.to_vec();
        self.arg = arg.to_vec();
        self.showerr = show_stderr;
        let script = String::from_utf8_lossy(script).into_owned();
        let arg = String::from_utf8_lossy(arg).into_owned();
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(&script).arg("sh").arg(&arg);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if !show_stderr {
            command.stderr(std::process::Stdio::null());
        }
        let output = command.output().map_err(|_| Errno(2))?;
        Ok(output.stdout)
    }
}

fn words(input: &str) -> (i32, Vec<String>) {
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    let rv = wordexp(&mut shell, input.as_bytes(), 0, &mut out);
    let words: Vec<String> = out
        .words
        .iter()
        .map(|w| String::from_utf8_lossy(w).into_owned())
        .collect();
    (rv, words)
}

fn expected(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

#[test]
fn simple_words_and_quotes() {
    assert_eq!(words("a b c"), (0, expected(&["a", "b", "c"])));
    assert_eq!(words("'a b' c"), (0, expected(&["a b", "c"])));
    assert_eq!(words("\"a b\" c"), (0, expected(&["a b", "c"])));
    assert_eq!(words("a\\ b"), (0, expected(&["a b"])));
    assert_eq!(words(""), (0, expected(&[])));
}

#[test]
fn variable_and_arithmetic_expansion() {
    let mut shell = HostShell::new().with_env("RAVEN_TEST", "value");
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"$RAVEN_TEST", 0, &mut out), 0);
    assert_eq!(out.words, vec![b"value".to_vec()]);
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"$((2+3))", 0, &mut out), 0);
    assert_eq!(out.words, vec![b"5".to_vec()]);
}

#[test]
fn home_expansion() {
    let mut shell = HostShell::new().with_env("HOME", "/home/raven");
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"~/x", 0, &mut out), 0);
    assert_eq!(out.words, vec![b"/home/raven/x".to_vec()]);
}

#[test]
fn the_runner_receives_the_donor_script() {
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"a b", 0, &mut out), 0);
    assert_eq!(shell.script, WORDEXP_SCRIPT);
    assert_eq!(shell.arg, b"a b");
    assert!(!shell.showerr);
    assert_eq!(WORDEXP_SCRIPT, b"eval \"set --;printf %s\\\\\\\\0 x $1\"");
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"a", WRDE_SHOWERR, &mut out), 0);
    assert!(shell.showerr);
}

#[test]
fn nocmd_rejects_command_substitution() {
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    assert_eq!(
        wordexp(&mut shell, b"$(echo hi)", WRDE_NOCMD, &mut out),
        WRDE_CMDSUB
    );
    assert!(shell.script.is_empty());
    assert_eq!(
        wordexp(&mut shell, b"`echo hi`", WRDE_NOCMD, &mut out),
        WRDE_CMDSUB
    );
    assert_eq!(
        wordexp(&mut shell, b"a|b", WRDE_NOCMD, &mut out),
        WRDE_BADCHAR
    );
    assert_eq!(
        wordexp(&mut shell, b"a;b", WRDE_NOCMD, &mut out),
        WRDE_BADCHAR
    );
    assert_eq!(wordexp(&mut shell, b"'a|b'", WRDE_NOCMD, &mut out), 0);
}

#[test]
fn shell_syntax_error_is_reported() {
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    assert_eq!(
        wordexp(&mut shell, b"'unterminated", 0, &mut out),
        WRDE_SYNTAX
    );
}

#[test]
fn append_and_reuse() {
    let mut shell = HostShell::new();
    let mut out = WordExp::default();
    assert_eq!(wordexp(&mut shell, b"a", 0, &mut out), 0);
    assert_eq!(wordexp(&mut shell, b"b", WRDE_APPEND, &mut out), 0);
    assert_eq!(out.words, vec![b"a".to_vec(), b"b".to_vec()]);
    assert_eq!(wordexp(&mut shell, b"c", WRDE_REUSE, &mut out), 0);
    assert_eq!(out.words, vec![b"c".to_vec()]);
}
