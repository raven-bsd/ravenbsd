// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn owners() -> Owners {
    Owners {
        entries: vec![
            Owner {
                prefix: "netbsd-libc:".to_string(),
                ticket: "C50".to_string(),
                area: "default".to_string(),
            },
            Owner {
                prefix: "netbsd-libc:string/".to_string(),
                ticket: "C40".to_string(),
                area: "string".to_string(),
            },
            Owner {
                prefix: "netbsd-libc:string/t_strchr".to_string(),
                ticket: "C40x".to_string(),
                area: "exact".to_string(),
            },
            Owner {
                prefix: "other:".to_string(),
                ticket: "C99".to_string(),
                area: "other".to_string(),
            },
        ],
    }
}

#[test]
fn the_longest_prefix_wins() {
    assert_eq!(owners().ticket("netbsd-libc", "string/t_strchr"), "C40x");
    assert_eq!(owners().ticket("netbsd-libc", "string/t_other"), "C40");
    assert_eq!(owners().ticket("netbsd-libc", "sys/t_fork"), "C50");
}

#[test]
fn a_suite_default_catches_unknown_directories() {
    assert_eq!(owners().ticket("netbsd-libc", "brand/new/dir"), "C50");
}

#[test]
fn an_unmapped_suite_gets_a_dash() {
    assert_eq!(owners().ticket("none", "string/t_strchr"), "-");
    assert!(fail_without_owner("-"));
}

#[test]
fn unmapped_directories_are_reported() {
    let table = Owners {
        entries: vec![
            Owner {
                prefix: "netbsd-libc:string/".to_string(),
                ticket: "C40".to_string(),
                area: "string".to_string(),
            },
            Owner {
                prefix: "other:".to_string(),
                ticket: "C99".to_string(),
                area: "other".to_string(),
            },
        ],
    };
    let programs = vec!["string/t_one".to_string(), "newdir/t_three".to_string()];
    assert_eq!(table.unmapped("netbsd-libc", &programs), ["newdir"]);
}

#[test]
fn all_covered_directories_are_not_reported() {
    let programs = vec!["string/t_one".to_string(), "sys/t_two".to_string()];
    assert!(owners().unmapped("netbsd-libc", &programs).is_empty());
}

#[test]
fn a_ticket_must_start_with_c() {
    let mut table = owners();
    table.entries[0].ticket = "T50".to_string();
    assert!(table.validate().is_err());
}

#[test]
fn a_prefix_needs_a_colon() {
    let mut table = owners();
    table.entries[0].prefix = "netbsd-libc".to_string();
    assert!(table.validate().is_err());
}

#[test]
fn an_empty_area_is_an_error() {
    let mut table = owners();
    table.entries[0].area.clear();
    assert!(table.validate().is_err());
}

#[test]
fn a_fail_row_without_owner_is_an_error() {
    assert!(fail_without_owner(""));
    assert!(fail_without_owner("-"));
    assert!(!fail_without_owner("C40"));
}

#[test]
fn every_directory_of_the_vendored_trees_has_an_owner() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(repo) = manifest.ancestors().nth(3) else {
        eprintln!("SKIP: repository root not found");
        return;
    };
    let oracle = repo.join("tests/libc-oracle");
    if !oracle.join("vendor").is_dir() {
        eprintln!("SKIP: tests/libc-oracle/vendor is absent");
        return;
    }
    let table = Owners::load(&oracle.join("owners.toml")).expect("owners.toml");
    let suites = std::fs::read_dir(oracle.join("suites")).expect("suites");
    for entry in suites {
        let path = entry.expect("suite entry").path();
        if path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        let suite = crate::suite::load(&path).expect("suite file");
        if suite.programs.is_empty() || suite.build.as_deref() == Some("external") {
            continue;
        }
        let programs: Vec<String> = suite
            .programs
            .iter()
            .map(|program| program.name.clone())
            .collect();
        let unmapped = table.unmapped(&suite.suite, &programs);
        assert!(
            unmapped.is_empty(),
            "{}: directories without an owner: {unmapped:?}",
            suite.suite
        );
    }
}
