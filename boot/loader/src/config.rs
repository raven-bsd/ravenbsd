// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for `\raven\boot.toml` (CONTRACTS C3).
//!
//! This is deliberately not a TOML implementation: the file is a small line-based sequence of
//! `key = "value"` entries. It is pure and free of UEFI types so the host tests can exercise every
//! error path.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Longest accepted value, in bytes. Values of one MiB or more are rejected.
pub const MAX_VALUE_LEN: usize = 1024 * 1024;

/// Loader log verbosity from `log_level`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogLevel {
    /// Errors only.
    Error,
    /// Errors and warnings.
    Warn,
    /// Errors, warnings and informational messages.
    #[default]
    Info,
    /// Everything, including debug messages.
    Debug,
}

impl LogLevel {
    /// Parse a configured log level.
    pub fn parse(value: &str) -> Option<LogLevel> {
        match value {
            "error" => Some(LogLevel::Error),
            "warn" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            _ => None,
        }
    }

    /// The canonical spelling used in `boot.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The parsed boot configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Kernel command line copied to `BootInfo.cmdline`.
    pub cmdline: String,
    /// Loader log verbosity.
    pub log_level: LogLevel,
}

/// A non-fatal parse result: an unknown key was ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// 1-based line number.
    pub line: usize,
    /// The unknown key.
    pub key: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: unknown key '{}' ignored", self.line, self.key)
    }
}

/// Why `boot.toml` was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The file is not valid UTF-8.
    NonUtf8,
    /// A non-comment line has no `=`.
    MalformedLine(usize),
    /// The key part of a line is empty.
    EmptyKey(usize),
    /// A value does not start and end with a double quote.
    UnterminatedString(usize),
    /// A value has at least [`MAX_VALUE_LEN`] bytes.
    ValueTooLong(usize),
    /// `log_level` is not one of `error`, `warn`, `info`, `debug`.
    InvalidLogLevel(usize),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NonUtf8 => f.write_str("boot.toml is not valid UTF-8"),
            ConfigError::MalformedLine(line) => {
                write!(f, "line {line}: expected key = \"value\"")
            }
            ConfigError::EmptyKey(line) => write!(f, "line {line}: empty key"),
            ConfigError::UnterminatedString(line) => {
                write!(f, "line {line}: value is not a quoted string")
            }
            ConfigError::ValueTooLong(line) => {
                write!(f, "line {line}: value is longer than one MiB")
            }
            ConfigError::InvalidLogLevel(line) => write!(
                f,
                "line {line}: log_level must be error, warn, info or debug"
            ),
        }
    }
}

/// Parse `boot.toml`, discarding unknown-key warnings.
pub fn parse(data: &[u8]) -> Result<Config, ConfigError> {
    parse_with_warnings(data).map(|(config, _warnings)| config)
}

/// Parse `boot.toml` and return the ignored unknown keys as warnings.
pub fn parse_with_warnings(data: &[u8]) -> Result<(Config, Vec<Warning>), ConfigError> {
    let text = core::str::from_utf8(data).map_err(|_| ConfigError::NonUtf8)?;
    let mut config = Config::default();
    let mut warnings = Vec::new();

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .ok_or(ConfigError::MalformedLine(line_number))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(ConfigError::EmptyKey(line_number));
        }
        let value = value.trim();
        if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
            return Err(ConfigError::UnterminatedString(line_number));
        }
        let value = &value[1..value.len() - 1];
        if value.len() >= MAX_VALUE_LEN {
            return Err(ConfigError::ValueTooLong(line_number));
        }

        match key {
            "cmdline" => config.cmdline = String::from(value),
            "log_level" => {
                config.log_level =
                    LogLevel::parse(value).ok_or(ConfigError::InvalidLogLevel(line_number))?;
            }
            _ => warnings.push(Warning {
                line: line_number,
                key: String::from(key),
            }),
        }
    }

    Ok((config, warnings))
}

#[cfg(test)]
mod tests {
    use super::{
        Config, ConfigError, LogLevel, MAX_VALUE_LEN, Warning, parse, parse_with_warnings,
    };
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
        let (config, warnings) =
            parse_with_warnings(b"mystery = \"x\"\ncmdline = \"ok\"\n").unwrap();
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
}
