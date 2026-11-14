// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const SAFE_TEXT: &str = include_str!("../data/async-signal-safe.txt");
const NOT_SAFE_TEXT: &str = include_str!("../data/not-async-signal-safe.txt");

fn safe_rows() -> Vec<(&'static str, &'static str)> {
    SAFE_TEXT
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, mark) = line.split_once('\t').expect("name<TAB>mark");
            (name, mark)
        })
        .collect()
}

#[test]
fn every_line_carries_a_verification_mark() {
    let rows = safe_rows();
    assert_eq!(rows.len(), ASYNC_SAFE.len());
    let mut verified = 0;
    let mut unverified = 0;
    for (name, mark) in &rows {
        assert!(
            mark.starts_with("VERIFIED:") || mark.starts_with("UNVERIFIED:"),
            "{name}: {mark}"
        );
        if mark.starts_with("VERIFIED:") {
            verified += 1;
        } else {
            unverified += 1;
        }
    }
    assert_eq!(verified + unverified, rows.len());
    assert!(verified > 180, "verified {verified}");
    println!("async-signal-safe: {verified} VERIFIED, {unverified} UNVERIFIED");
}

#[test]
fn the_const_table_and_the_data_file_agree() {
    let rows = safe_rows();
    for name in ASYNC_SAFE {
        assert!(
            rows.iter().any(|(candidate, _)| candidate == name),
            "{name} missing from data/async-signal-safe.txt"
        );
        assert!(is_async_signal_safe(name), "{name}");
    }
    for (name, _) in &rows {
        assert!(ASYNC_SAFE.contains(name), "{name} missing from ASYNC_SAFE");
    }
}

#[test]
fn lookup_is_exact_and_case_sensitive() {
    assert!(is_async_signal_safe("write"));
    assert!(!is_async_signal_safe("Write"));
    assert!(!is_async_signal_safe(""));
    assert!(!is_async_signal_safe("not_a_function"));
}

#[test]
fn crate_functions_are_on_the_list() {
    for name in CRATE_FUNCTIONS {
        assert!(is_async_signal_safe(name), "{name}");
    }
}

#[test]
fn not_safe_names_are_absent_from_the_safe_table() {
    for line in NOT_SAFE_TEXT.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        assert!(!is_async_signal_safe(name), "{name} must not be safe");
    }
}

#[test]
fn every_crate_export_is_a_real_abi_symbol_when_the_table_exists() {
    let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../abi/symbols.toml");
    if !candidate.is_file() {
        println!(
            "skipped: {} does not exist yet (C33 output)",
            candidate.display()
        );
        return;
    }
    let text = std::fs::read_to_string(&candidate).expect("symbols.toml");
    for name in CRATE_FUNCTIONS {
        assert!(
            text.contains(&format!("\"{name}\"")) || text.contains(&format!("{name} ")),
            "{name} is not in abi/symbols.toml"
        );
    }
}
