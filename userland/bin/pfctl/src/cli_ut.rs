// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI tests: argument forms, exit codes and dry-run behaviour.

use super::*;
use alloc::collections::BTreeMap;

struct MemoryLoader {
    files: BTreeMap<String, String>,
}

impl FileLoader for MemoryLoader {
    fn read(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| String::from("not found"))
    }
}

fn loader() -> MemoryLoader {
    let mut files = BTreeMap::new();
    files.insert(
        String::from("pf.conf"),
        String::from("pass in all\nblock out all\n"),
    );
    files.insert(
        String::from("table.txt"),
        String::from("10.0.0.1\n!10.0.0.2\n"),
    );
    files.insert(String::from("bad.conf"), String::from("pass frobnicate\n"));
    MemoryLoader { files }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|value| value.to_string()).collect()
}

#[test]
fn dry_run_prints_the_normalized_config() {
    let outcome = run(&args(&["-n", "-v", "-f", "pf.conf"]), &loader());
    assert_eq!(outcome.exit_code, 0, "{}", outcome.stderr);
    assert_eq!(
        outcome.stdout,
        "pass in all flags any keep state\nblock drop out all\n"
    );
}

#[test]
fn clustered_nv_is_accepted() {
    let outcome = run(&args(&["-nv", "-f", "pf.conf"]), &loader());
    assert_eq!(outcome.exit_code, 0, "{}", outcome.stderr);
    assert!(outcome.stdout.contains("pass in all"));
}

#[test]
fn a_parse_error_exits_one_with_the_file_and_line() {
    let outcome = run(&args(&["-nv", "-f", "bad.conf"]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.stderr.contains("bad.conf:1:"), "{}", outcome.stderr);
    assert!(
        outcome.stderr.contains("unexpected token"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn loading_without_n_is_refused_until_the_abi_exists() {
    let outcome = run(&args(&["-v", "-f", "pf.conf"]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.stderr.contains("K273"), "{}", outcome.stderr);
}

#[test]
fn a_missing_file_is_an_error() {
    let outcome = run(&args(&["-n", "-f", "absent.conf"]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.stderr.contains("absent.conf"), "{}", outcome.stderr);
}

#[test]
fn no_action_prints_usage_and_exits_one() {
    let outcome = run(&args(&[]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(
        outcome.stderr.contains("usage: pfctl"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn help_exits_zero() {
    let outcome = run(&args(&["-h"]), &loader());
    assert_eq!(outcome.exit_code, 0);
    assert!(outcome.stdout.contains("usage: pfctl"));
}

#[test]
fn table_dry_run_prints_the_table_and_negation() {
    let outcome = run(
        &args(&["-n", "-v", "-t", "web", "-T", "add", "-f", "table.txt"]),
        &loader(),
    );
    assert_eq!(outcome.exit_code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, "table <web> { 10.0.0.1 !10.0.0.2 }\n");
}

#[test]
fn table_show_needs_the_abi() {
    let outcome = run(&args(&["-t", "web", "-T", "show"]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.stderr.contains("K273"), "{}", outcome.stderr);
}

#[test]
fn macro_definitions_come_from_d() {
    let outcome = run(
        &args(&["-nv", "-D", "ext=em0", "-f", "macro.conf"]),
        &MemoryLoader {
            files: {
                let mut files = BTreeMap::new();
                files.insert(
                    String::from("macro.conf"),
                    String::from("pass in on $ext all\n"),
                );
                files
            },
        },
    );
    assert_eq!(outcome.exit_code, 0, "{}", outcome.stderr);
    assert!(
        outcome.stdout.contains("pass in on em0 all"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn an_undefined_macro_is_a_line_numbered_error() {
    let outcome = run(
        &args(&["-nv", "-f", "undefined.conf"]),
        &MemoryLoader {
            files: {
                let mut files = BTreeMap::new();
                files.insert(
                    String::from("undefined.conf"),
                    String::from("pass in on $missing all\n"),
                );
                files
            },
        },
    );
    assert_eq!(outcome.exit_code, 1);
    assert!(
        outcome.stderr.contains("undefined.conf:1:"),
        "{}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("missing"), "{}", outcome.stderr);
}

#[test]
fn unknown_options_are_refused() {
    let outcome = run(&args(&["-Q"]), &loader());
    assert_eq!(outcome.exit_code, 1);
    assert!(
        outcome.stderr.contains("unknown option"),
        "{}",
        outcome.stderr
    );
}
