// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the startup stack parser.

#![allow(clippy::indexing_slicing)] // fixed test vectors
#![allow(clippy::arithmetic_side_effects)] // test vector construction

use super::*;
use crate::consts::{
    AT_CLKTCK, AT_EXECFN, AT_HWCAP, AT_HWCAP2, AT_NULL, AT_PAGESZ, AT_RANDOM, AT_RAVEN_ABI,
    AT_RAVEN_CWD, AT_RAVEN_FDMAP, AT_RAVEN_FDMAP_N, AT_RAVEN_PROCD, AT_RAVEN_PROCINFO,
    AT_RAVEN_TIMEPAGE, AT_SECURE,
};

struct Words {
    words: Vec<usize>,
}

impl Words {
    fn new(words: Vec<usize>) -> Self {
        Self { words }
    }
}

impl WordSource for Words {
    fn word(&self, index: usize) -> Option<usize> {
        self.words.get(index).copied()
    }
}

struct StrictWords {
    words: Vec<usize>,
}

impl WordSource for StrictWords {
    fn word(&self, index: usize) -> Option<usize> {
        if index >= self.words.len() {
            panic!("read outside the mapped stack at {index}");
        }
        Some(self.words[index])
    }
}

fn empty_stack() -> Vec<usize> {
    alloc::vec![0, 0, 0, AT_NULL, 0]
}

#[test]
fn an_empty_stack_parses() {
    let info = parse_stack(&Words::new(empty_stack())).expect("stack");
    assert_eq!(info.argc, 0);
    assert!(info.argv.is_empty());
    assert!(info.envp.is_empty());
    assert_eq!(info.page_size, 4096);
    assert_eq!(info.clk_tck, 100);
    assert!(!info.secure);
    assert_eq!(info.random, None);
}

#[test]
fn a_missing_argv_terminator_is_bad_argv() {
    let words = alloc::vec![1, 0x100, 0x200, 0, AT_NULL, 0];
    assert_eq!(parse_stack(&Words::new(words)), Err(StartError::BadArgv));
}

#[test]
fn a_missing_environment_terminator_is_reported() {
    let words = alloc::vec![0, 0, 0x300, 0x400];
    assert_eq!(
        parse_stack(&Words::new(words)),
        Err(StartError::NoEnvTerminator)
    );
}

#[test]
fn a_missing_at_null_is_reported() {
    let words = alloc::vec![0, 0, 0, AT_PAGESZ, 4096];
    assert_eq!(parse_stack(&Words::new(words)), Err(StartError::NoAtNull));
}

#[test]
fn ten_thousand_environment_entries_parse() {
    let mut words = alloc::vec![0, 0];
    for index in 0..10_000usize {
        words.push(0x1000 + index);
    }
    words.push(0);
    words.extend_from_slice(&[AT_NULL, 0]);
    let info = parse_stack(&Words::new(words)).expect("stack");
    assert_eq!(info.envp.len(), 10_000);
}

#[test]
fn a_too_large_argc_is_rejected() {
    let words = alloc::vec![crate::limits::MAX_ENV_WORDS + 1, 0, 0];
    assert_eq!(
        parse_stack(&Words::new(words)),
        Err(StartError::TooManyWords)
    );
}

#[test]
fn auxiliary_values_are_decoded() {
    let words = alloc::vec![
        0, 0, 0, AT_PAGESZ, 8192, AT_CLKTCK, 250, AT_SECURE, 1, AT_RANDOM, 0x5000, AT_EXECFN,
        0x6000, AT_HWCAP, 3, AT_HWCAP2, 4, AT_NULL, 0,
    ];
    let info = parse_stack(&Words::new(words)).expect("stack");
    assert_eq!(info.page_size, 8192);
    assert_eq!(info.clk_tck, 250);
    assert!(info.secure);
    assert_eq!(info.random, Some(0x5000));
    assert_eq!(info.execfn, Some(0x6000));
    assert_eq!(info.hwcap, 3);
    assert_eq!(info.hwcap2, 4);
}

#[test]
fn raven_keys_are_decoded() {
    let words = alloc::vec![
        0,
        0,
        0,
        AT_RAVEN_PROCD,
        0x42,
        AT_RAVEN_PROCINFO,
        0x7000,
        AT_RAVEN_FDMAP,
        0x8000,
        AT_RAVEN_FDMAP_N,
        3,
        AT_RAVEN_CWD,
        0x9000,
        AT_RAVEN_ABI,
        7,
        AT_RAVEN_TIMEPAGE,
        0xa000,
        AT_NULL,
        0,
    ];
    let info = parse_stack(&Words::new(words)).expect("stack");
    assert_eq!(info.raven.procd, Some(0x42));
    assert_eq!(info.raven.procinfo, Some(0x7000));
    assert_eq!(info.raven.fdmap, Some(0x8000));
    assert_eq!(info.raven.fdmap_n, 3);
    assert_eq!(info.raven.cwd, Some(0x9000));
    assert_eq!(info.raven.abi, Some(7));
    assert_eq!(info.raven.timepage, Some(0xa000));
}

#[test]
fn duplicate_keys_keep_the_first_value() {
    let words = alloc::vec![0, 0, 0, AT_PAGESZ, 8192, AT_PAGESZ, 4096, AT_NULL, 0];
    let info = parse_stack(&Words::new(words)).expect("stack");
    assert_eq!(info.page_size, 8192);
    assert_eq!(info.auxv.get(AT_PAGESZ), Some(8192));
}

#[test]
fn unknown_keys_are_kept() {
    let words = alloc::vec![0, 0, 0, 0x1234, 0x5678, AT_NULL, 0];
    let info = parse_stack(&Words::new(words)).expect("stack");
    assert_eq!(info.auxv.get(0x1234), Some(0x5678));
}

#[test]
fn the_parser_never_reads_outside_a_valid_stack() {
    let words = alloc::vec![1, 0x100, 0, 0, AT_NULL, 0];
    let info = parse_stack(&StrictWords { words }).expect("stack");
    assert_eq!(info.argc, 1);
    assert_eq!(info.argv, alloc::vec![0x100]);
}

#[test]
fn split_progname_handles_edges() {
    assert_eq!(split_progname(b""), 0);
    assert_eq!(split_progname(b"/"), 1);
    assert_eq!(split_progname(b"a/"), 2);
    assert_eq!(split_progname(b"//x"), 2);
    assert_eq!(split_progname(b"x/y/z"), 4);
    assert_eq!(split_progname(b"plain"), 0);
}

#[test]
fn split_progname_accepts_non_utf8() {
    assert_eq!(split_progname(&[0xff, b'/', 0xfe]), 2);
}

#[test]
fn progname_prefers_argv_zero() {
    assert_eq!(
        progname_sources(Some(b"/bin/sh"), Some(b"/bin/bash")),
        b"/bin/sh"
    );
}

#[test]
fn progname_falls_back_to_execfn() {
    assert_eq!(progname_sources(None, Some(b"/bin/sh")), b"/bin/sh");
}

#[test]
fn progname_defaults_to_empty() {
    assert!(progname_sources(None, None).is_empty());
}

#[test]
fn too_many_auxv_pairs_is_no_at_null() {
    let mut words = alloc::vec![0, 0, 0];
    for index in 0..=crate::limits::MAX_AUXV_PAIRS {
        words.push(0x2000 + index);
        words.push(index);
    }
    assert_eq!(parse_stack(&Words::new(words)), Err(StartError::NoAtNull));
}
