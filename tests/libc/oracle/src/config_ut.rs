// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

#[test]
fn defaults_are_documented() {
    let config = Config::defaults();
    assert_eq!(config.seeds, 20);
    assert_eq!(config.cases, 10_000);
    assert_eq!(config.seed_base, 1);
    assert!(!config.exhaustive);
    assert_eq!(config.filter, None);
}

#[test]
fn each_variable_overrides_the_default() {
    let (config, warnings) = Config::from_lookup(&lookup(&[
        ("RL_DIFF_SEEDS", "3"),
        ("RL_DIFF_CASES", "50"),
        ("RL_DIFF_SEED_BASE", "9"),
        ("RL_DIFF_EXHAUSTIVE", "1"),
        ("RL_DIFF_FILTER", "memchr"),
    ]));
    assert_eq!(config.seeds, 3);
    assert_eq!(config.cases, 50);
    assert_eq!(config.seed_base, 9);
    assert!(config.exhaustive);
    assert_eq!(config.filter.as_deref(), Some("memchr"));
    assert!(warnings.is_empty());
}

#[test]
fn garbage_values_fall_back_with_a_warning() {
    let (config, warnings) = Config::from_lookup(&lookup(&[
        ("RL_DIFF_SEEDS", "many"),
        ("RL_DIFF_CASES", "-3"),
        ("RL_DIFF_SEED_BASE", "x"),
        ("RL_DIFF_EXHAUSTIVE", "perhaps"),
    ]));
    assert_eq!(config, Config::defaults());
    assert_eq!(warnings.len(), 4);
    assert!(warnings[0].contains("RL_DIFF_SEEDS"));
    assert!(warnings[1].contains("RL_DIFF_CASES"));
    assert!(warnings[2].contains("RL_DIFF_SEED_BASE"));
    assert!(warnings[3].contains("RL_DIFF_EXHAUSTIVE"));
}

#[test]
fn zero_is_not_a_useful_count() {
    let (config, warnings) =
        Config::from_lookup(&lookup(&[("RL_DIFF_SEEDS", "0"), ("RL_DIFF_CASES", "0")]));
    assert_eq!(config.seeds, 20);
    assert_eq!(config.cases, 10_000);
    assert_eq!(warnings.len(), 2);
}

#[test]
fn an_empty_filter_is_unset() {
    let (config, warnings) = Config::from_lookup(&lookup(&[("RL_DIFF_FILTER", "")]));
    assert_eq!(config.filter, None);
    assert!(warnings.is_empty());
}
