// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `seed-assignments`.

use std::fs;

use super::*;
use crate::testutil::{TempDir, fixture};

const TABLE: &str = "\
| id | header | symbols | ring | pers | doc | crate | status | notes |
|---|---|---|---|---|---|---|---|---|
| X01 | string.h | `strlcpy strlcat` | 1 | - | 01 | str | EXISTS-donor | note |
| X02 | string.h | `memmem memrchr` | 1 | - | 01 | str | EXISTS-donor | note |
| L01 | sys/epoll.h | `epoll_create epoll_create1` | 2 | linux | 11/15 | p-linux | TO-BE-DESIGNED | note |
| Z01 | (private) | `__raven_libc_start __libc_start_main` | private | - | 10 | shell | TO-BE-DESIGNED | note |
| P54 | ucontext.h | `getcontext makecontext setcontext swapcontext` | 1 | - | 09 | signal | TO-BE-DESIGNED | note |
| X35 | unistd.h | `pread64 pwrite64 ... lseek64` | 2 | linux | 11 | p-linux | EXISTS-donor | note |
| Z03 | (data) | `environ __environ _environ` | 0/1 | - | 03 | stdlib | EXISTS-donor | note |
| X99 | thing.h | `pthread_attr_* <stdbit.h> a/b foo...` | 1 | - | 01 | str | EXISTS-donor | note |
";

#[test]
fn backtick_groups_split_into_assignments() {
    let result = seed_from_text(TABLE);
    let names = names(&result);
    assert!(names.contains("strlcpy"));
    assert!(names.contains("strlcat"));
    assert!(names.contains("epoll_create"));
    assert!(names.contains("getcontext"));
    assert_eq!(result.rows, 8);
}

#[test]
fn pattern_tokens_are_skipped_and_counted() {
    let result = seed_from_text(TABLE);
    assert_eq!(result.skipped_patterns, 5);
    assert!(!names(&result).contains("pthread_attr_"));
    assert!(!names(&result).contains("a/b"));
}

#[test]
fn mixed_rows_get_the_first_ring_and_an_override() {
    let result = seed_from_text(TABLE);
    let environ = result
        .assignments
        .assign
        .iter()
        .find(|assignment| assignment.name == "environ")
        .unwrap();
    assert_eq!(environ.ring, Some(Ring::Number(0)));
    assert_eq!(environ.ring_override.as_deref(), Some("mixed row Z03"));
}

#[test]
fn a_private_row_uses_the_private_ring() {
    let result = seed_from_text(TABLE);
    let start = result
        .assignments
        .assign
        .iter()
        .find(|assignment| assignment.name == "__raven_libc_start")
        .unwrap();
    assert_eq!(start.ring, Some(Ring::private()));
}

#[test]
fn personality_crate_area_and_status_are_copied() {
    let result = seed_from_text(TABLE);
    let epoll = result
        .assignments
        .assign
        .iter()
        .find(|assignment| assignment.name == "epoll_create")
        .unwrap();
    assert_eq!(epoll.personality.as_deref(), Some("linux"));
    assert_eq!(epoll.crate_name.as_deref(), Some("p-linux"));
    assert_eq!(epoll.area, Some(11));
    assert_eq!(epoll.status.as_deref(), Some("TO-BE-DESIGNED"));
}

#[test]
fn assignments_are_sorted_by_name() {
    let result = seed_from_text(TABLE);
    let names: Vec<&str> = result
        .assignments
        .assign
        .iter()
        .map(|assignment| assignment.name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn a_duplicate_name_keeps_the_first_row_and_logs_a_conflict() {
    let text =
        format!("{TABLE}| X77 | string.h | `strlcpy` | 1 | - | 01 | str | EXISTS-donor | dup |\n");
    let result = seed_from_text(&text);
    assert_eq!(result.conflicts.len(), 1);
    assert!(result.conflicts[0].contains("strlcpy"));
    assert!(result.conflicts[0].contains("X01"));
    assert_eq!(
        result
            .assignments
            .assign
            .iter()
            .filter(|assignment| assignment.name == "strlcpy")
            .count(),
        1
    );
}

#[test]
fn the_excerpt_fixture_is_parsed() {
    let text = fs::read_to_string(fixture("api-surface-excerpt.md")).unwrap();
    let result = seed_from_text(&text);
    assert!(result.rows >= 2);
    assert!(!result.assignments.assign.is_empty());
}

#[test]
fn seed_writes_a_provenance_header_and_round_trips() {
    let temp = TempDir::new("seed-write");
    temp.write("api.md", TABLE);
    let out = temp.path().join("assignments.toml");
    let result = seed(&temp.path().join("api.md"), &out).unwrap();
    let text = fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(text.contains("# SEEDED by libc-census seed-assignments"));
    let parsed: Assignments = toml::from_str(&text).unwrap();
    assert_eq!(parsed.assign.len(), result.assignments.assign.len());
    assert_eq!(parsed.assign, result.assignments.assign);
}

#[test]
fn a_missing_api_surface_file_is_an_error() {
    let temp = TempDir::new("seed-missing");
    let result = seed(
        &temp.path().join("no-such-file.md"),
        &temp.path().join("out/assignments.toml"),
    );
    assert!(result.is_err());
}

#[test]
fn rows_without_a_section_id_are_ignored() {
    let text = "\
| id | header | symbols | ring | pers | doc | crate | status | notes |
|---|---|---|---|---|---|---|---|---|
| not-an-id | string.h | `strlcpy` | 1 | - | 01 | str | EXISTS-donor | x |
";
    let result = seed_from_text(text);
    assert_eq!(result.rows, 0);
    assert!(result.assignments.assign.is_empty());
}
