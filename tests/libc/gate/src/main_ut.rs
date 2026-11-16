// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI parsing tests.

use super::*;

#[test]
fn help_exits_zero() {
    assert_eq!(run(vec!["help".to_string()]).unwrap(), 0);
}

#[test]
fn parses_all_options() {
    let parsed = parse(&[
        "--rows".to_string(),
        "r.toml".to_string(),
        "--out".to_string(),
        "out".to_string(),
        "--only".to_string(),
        "1".to_string(),
        "--timeout-secs".to_string(),
        "5".to_string(),
        "--repo".to_string(),
        "repo".to_string(),
        "--symbols".to_string(),
        "s.toml".to_string(),
        "--pages".to_string(),
        "p.toml".to_string(),
        "--man".to_string(),
        "man".to_string(),
    ])
    .unwrap();
    assert_eq!(parsed.rows.unwrap(), PathBuf::from("r.toml"));
    assert_eq!(parsed.out.unwrap(), PathBuf::from("out"));
    assert_eq!(parsed.only.as_deref(), Some("1"));
    assert_eq!(parsed.timeout, Some(5));
    assert_eq!(parsed.repo.unwrap(), PathBuf::from("repo"));
    assert_eq!(parsed.symbols.unwrap(), PathBuf::from("s.toml"));
    assert_eq!(parsed.pages.unwrap(), PathBuf::from("p.toml"));
    assert_eq!(parsed.man.unwrap(), PathBuf::from("man"));
}

#[test]
fn an_unknown_flag_is_an_error() {
    assert!(parse(&["--magic".to_string()]).is_err());
}

#[test]
fn a_flag_without_a_value_is_an_error() {
    assert!(parse(&["--rows".to_string()]).is_err());
}

#[test]
fn an_unknown_command_is_an_error() {
    assert!(run(vec!["magic".to_string()]).is_err());
}
