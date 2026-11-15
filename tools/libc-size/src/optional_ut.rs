// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the optional-personality allowlist.

use super::*;

fn table(names: &[(&str, &str)]) -> OptionalSymbols {
    OptionalSymbols {
        schema: 1,
        symbols: names
            .iter()
            .map(|(name, reason)| OptionalSymbol {
                name: (*name).to_string(),
                reason: (*reason).to_string(),
            })
            .collect(),
    }
}

#[test]
fn allowlisted_server_only_names_are_removed() {
    let optional = table(&[("thr_self", "Solaris thread alias")]);
    let filtered = optional.filter_server_only(vec!["thr_self".to_string()]);
    assert!(filtered.is_empty());
    assert!(!set_symbols_fail(&[], &filtered));
    let filtered = optional.filter_server_only(vec![
        "thr_self".to_string(),
        "unknown_server_name".to_string(),
    ]);
    assert_eq!(filtered, vec!["unknown_server_name".to_string()]);
    assert!(set_symbols_fail(&[], &filtered));
}

#[test]
fn a_tiny_only_name_still_fails_even_with_an_allowlist() {
    let optional = table(&[("thr_self", "Solaris thread alias")]);
    let server_only = optional.filter_server_only(Vec::new());
    assert!(set_symbols_fail(
        &["tiny_only_name".to_string()],
        &server_only
    ));
}

#[test]
fn an_unlisted_server_only_name_fails() {
    let optional = table(&[("thr_self", "Solaris thread alias")]);
    let filtered = optional.filter_server_only(vec!["not_listed".to_string()]);
    assert!(set_symbols_fail(&[], &filtered));
}

#[test]
fn a_missing_file_allows_nothing() {
    let path = std::env::temp_dir().join("libc-size-no-optional-symbols.toml");
    let _ = std::fs::remove_file(&path);
    let optional = OptionalSymbols::load(&path).unwrap();
    assert!(optional.names().is_empty());
    assert!(set_symbols_fail(
        &[],
        &optional.filter_server_only(vec!["thr_self".to_string()])
    ));
}

#[test]
fn a_symbol_without_a_reason_is_an_error() {
    let path = std::env::temp_dir().join("libc-size-bad-optional-symbols.toml");
    let _ = std::fs::remove_file(&path);
    std::fs::write(
        &path,
        "schema = 1\n\n[[symbol]]\nname = \"thr_self\"\nreason = \"  \"\n",
    )
    .unwrap();
    let error = OptionalSymbols::load(&path).unwrap_err();
    assert!(error.to_string().contains("every optional symbol"));
    let _ = std::fs::remove_file(&path);
}
