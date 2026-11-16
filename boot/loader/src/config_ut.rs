// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `boot.toml` parser and A/B transitions.

use super::{
    Config, ConfigError, LogLevel, MAX_ATTEMPTS, MAX_VALUE_LEN, Slot, Warning, parse,
    parse_with_warnings,
};
use alloc::format;
use alloc::string::String;
use alloc::vec;

#[test]
fn empty_file_uses_defaults() {
    assert_eq!(parse(b"").unwrap(), Config::default());
    assert_eq!(parse(b"\n\n").unwrap(), Config::default());
}

#[test]
fn only_comments_use_defaults() {
    let text = b"# comment\n  # indented comment\n\n";
    assert_eq!(parse(text).unwrap(), Config::default());
}

#[test]
fn cmdline_keeps_spaces_and_equals() {
    let config = parse(b"cmdline = \"console=ttyS0 test-exit=1 foo=bar baz\"\n").unwrap();
    assert_eq!(config.cmdline, "console=ttyS0 test-exit=1 foo=bar baz");
}

#[test]
fn unknown_key_is_a_warning() {
    let (config, warnings) = parse_with_warnings(b"mystery = \"x\"\ncmdline = \"ok\"\n").unwrap();
    assert_eq!(config.cmdline, "ok");
    assert_eq!(
        warnings,
        vec![Warning {
            line: 1,
            key: String::from("mystery"),
        }]
    );
}

#[test]
fn missing_quote_is_an_error() {
    assert_eq!(
        parse(b"cmdline = \"unterminated\n").unwrap_err(),
        ConfigError::UnterminatedString(1)
    );
    assert_eq!(
        parse(b"cmdline = unquoted\n").unwrap_err(),
        ConfigError::UnterminatedString(1)
    );
}

#[test]
fn non_utf8_is_an_error() {
    assert_eq!(
        parse(b"cmdline = \"\xff\"\n").unwrap_err(),
        ConfigError::NonUtf8
    );
}

#[test]
fn invalid_log_level_is_an_error() {
    assert_eq!(
        parse(b"log_level = \"loud\"\n").unwrap_err(),
        ConfigError::InvalidLogLevel(1)
    );
}

#[test]
fn every_log_level_is_accepted() {
    for (text, expected) in [
        ("error", LogLevel::Error),
        ("warn", LogLevel::Warn),
        ("info", LogLevel::Info),
        ("debug", LogLevel::Debug),
    ] {
        let config = parse(format!("log_level = \"{text}\"\n").as_bytes()).unwrap();
        assert_eq!(config.log_level, expected);
    }
}

#[test]
fn duplicate_keys_last_wins() {
    let config = parse(
        b"cmdline = \"first\"\nlog_level = \"error\"\ncmdline = \"second\"\nlog_level = \"debug\"\n",
    )
    .unwrap();
    assert_eq!(config.cmdline, "second");
    assert_eq!(config.log_level, LogLevel::Debug);
}

#[test]
fn crlf_line_endings_are_accepted() {
    let config = parse(b"cmdline = \"hello\"\r\nlog_level = \"warn\"\r\n").unwrap();
    assert_eq!(config.cmdline, "hello");
    assert_eq!(config.log_level, LogLevel::Warn);
}

#[test]
fn one_mib_value_is_rejected() {
    let mut text = String::from("cmdline = \"");
    text.push_str(&"a".repeat(MAX_VALUE_LEN));
    text.push_str("\"\n");
    assert_eq!(
        parse(text.as_bytes()).unwrap_err(),
        ConfigError::ValueTooLong(1)
    );
}

#[test]
fn malformed_lines_are_rejected() {
    assert_eq!(
        parse(b"cmdline\n").unwrap_err(),
        ConfigError::MalformedLine(1)
    );
    assert_eq!(
        parse(b" = \"value\"\n").unwrap_err(),
        ConfigError::EmptyKey(1)
    );
}

#[test]
fn slot_keys_are_parsed_and_rendered() {
    let config =
        parse(b"slot = \"a\"\ntrial = \"b\"\ntrial_token = \"deadbeef\"\nattempts = \"2\"\n")
            .unwrap();
    assert_eq!(config.slot, Slot::A);
    assert_eq!(config.trial, Some(Slot::B));
    assert_eq!(config.trial_token, Some(0xdead_beef));
    assert_eq!(config.attempts, 2);
    assert!(config.has_slot_config);
    assert_eq!(config.boot_slot(), Slot::B);

    let rendered = config.render();
    assert!(rendered.contains("slot = \"a\"\n"));
    assert!(rendered.contains("trial = \"b\"\n"));
    assert!(rendered.contains("trial_token = \"deadbeef\"\n"));
    assert!(rendered.contains("attempts = \"2\"\n"));
    assert_eq!(parse(rendered.as_bytes()).unwrap(), config);
}

#[test]
fn legacy_file_has_no_slot_config() {
    let config = parse(b"cmdline = \"x\"\nlog_level = \"warn\"\n").unwrap();
    assert!(!config.has_slot_config);
    assert_eq!(config.slot, Slot::A);
    assert_eq!(config.trial, None);
    assert_eq!(config.attempts, 0);
}

#[test]
fn empty_trial_is_none() {
    let config = parse(b"slot = \"b\"\ntrial = \"\"\nattempts = \"0\"\n").unwrap();
    assert_eq!(config.slot, Slot::B);
    assert_eq!(config.trial, None);
    assert_eq!(config.boot_slot(), Slot::B);
}

#[test]
fn invalid_slot_and_attempts_are_errors() {
    assert_eq!(
        parse(b"slot = \"c\"\n").unwrap_err(),
        ConfigError::InvalidSlot(1)
    );
    assert_eq!(
        parse(b"trial = \"z\"\n").unwrap_err(),
        ConfigError::InvalidSlot(1)
    );
    assert_eq!(
        parse(b"attempts = \"x\"\n").unwrap_err(),
        ConfigError::InvalidAttempts(1)
    );
    assert_eq!(
        parse(format!("attempts = \"{}\"\n", MAX_ATTEMPTS + 1).as_bytes()).unwrap_err(),
        ConfigError::InvalidAttempts(1)
    );
    assert_eq!(
        parse(b"trial_token = \"not-hex\"\n").unwrap_err(),
        ConfigError::InvalidTrialToken(1)
    );
}

#[test]
fn healthy_trial_becomes_confirmed() {
    let mut config = Config {
        slot: Slot::A,
        trial: Some(Slot::B),
        attempts: 2,
        has_slot_config: true,
        ..Config::default()
    };
    config.mark_healthy(Slot::B);
    assert_eq!(config.slot, Slot::B);
    assert_eq!(config.trial, None);
    assert_eq!(config.attempts, 0);
}

#[test]
fn failed_attempts_then_rollback_keep_the_confirmed_slot() {
    let mut config = Config {
        slot: Slot::A,
        trial: Some(Slot::B),
        attempts: 2,
        has_slot_config: true,
        ..Config::default()
    };
    config.fail_attempt();
    assert_eq!(config.attempts, 1);
    config.fail_attempt();
    assert_eq!(config.attempts, 0);
    config.rollback();
    assert_eq!(config.slot, Slot::A);
    assert_eq!(config.trial, None);
    assert_eq!(config.attempts, 0);
    assert_eq!(config.boot_slot(), Slot::A);
}
