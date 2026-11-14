// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::PathBuf;

use super::*;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-exclude-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

#[test]
fn a_missing_file_is_an_empty_table() {
    let table =
        ExcludeFile::load(std::path::Path::new("/nonexistent/excluded.toml")).expect("load");
    assert!(table.entries.is_empty());
}

#[test]
fn an_empty_reason_is_an_error() {
    let root = temp("reason");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='netbsd-libc'\nprogram='sys/t_ptrace_*'\nkind='manual'\nreason=''\n",
    )
    .expect("write");
    assert!(ExcludeFile::load(&path).is_err());
}

#[test]
fn an_unknown_kind_is_an_error() {
    let root = temp("kind");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='s'\nprogram='p'\nkind='wish'\nreason='why'\n",
    )
    .expect("write");
    assert!(ExcludeFile::load(&path).is_err());
}

#[test]
fn a_program_glob_matches() {
    let root = temp("glob");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='netbsd-libc'\nprogram='sys/t_ptrace_*'\nkind='manual'\nreason='NetBSD ptrace'\n",
    )
    .expect("write");
    let table = ExcludeFile::load(&path).expect("load");
    assert!(
        table
            .matched("netbsd-libc", "sys/t_ptrace_wait", None)
            .is_some()
    );
    assert!(table.matched("netbsd-libc", "sys/t_fork", None).is_none());
}

#[test]
fn a_case_glob_only_matches_that_case() {
    let root = temp("case");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='s'\nprogram='p'\ncase='one*'\nkind='manual'\nreason='why'\n",
    )
    .expect("write");
    let table = ExcludeFile::load(&path).expect("load");
    assert!(table.matched("s", "p", Some("one")).is_some());
    assert!(table.matched("s", "p", Some("two")).is_none());
}

#[test]
fn manual_count_ignores_oracle_exclusions() {
    let root = temp("count");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='s'\nprogram='a'\nkind='manual'\nreason='one'\n\n[[exclude]]\nsuite='s'\nprogram='b'\nkind='oracle'\nreason='two'\n",
    )
    .expect("write");
    assert_eq!(ExcludeFile::load(&path).expect("load").manual_count(), 1);
}

#[test]
fn the_reason_is_preserved() {
    let root = temp("preserve");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='s'\nprogram='p'\nkind='manual'\nreason='NetBSD ptrace(2)'\n",
    )
    .expect("write");
    let table = ExcludeFile::load(&path).expect("load");
    assert_eq!(
        table.matched("s", "p", None).expect("match").reason,
        "NetBSD ptrace(2)"
    );
}

#[test]
fn a_wrong_suite_does_not_match() {
    let root = temp("suite");
    let path = root.join("excluded.toml");
    fs::write(
        &path,
        "[[exclude]]\nsuite='a'\nprogram='p'\nkind='manual'\nreason='why'\n",
    )
    .expect("write");
    let table = ExcludeFile::load(&path).expect("load");
    assert!(table.matched("b", "p", None).is_none());
}
