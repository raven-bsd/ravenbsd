// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn civil_dates_are_known_days() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    assert_eq!(civil_from_days(20_686), (2026, 8, 21));
}

#[test]
fn today_is_an_iso_date() {
    let today = today();
    assert_eq!(today.len(), 10);
    assert_eq!(&today[4..5], "-");
    assert_eq!(&today[7..8], "-");
}

#[test]
fn split_var_needs_an_equals_sign() {
    assert_eq!(
        split_var("a=b").expect("split"),
        ("a".to_string(), "b".to_string())
    );
    assert!(split_var("ab").is_err());
}

#[test]
fn parse_u64_rejects_negative_values() {
    assert_eq!(parse_u64("5", "--jobs").expect("parse"), 5);
    assert!(parse_u64("-1", "--jobs").is_err());
}

#[test]
fn default_jobs_is_between_one_and_eight() {
    let jobs = default_jobs();
    assert!((1..=8).contains(&jobs));
}
