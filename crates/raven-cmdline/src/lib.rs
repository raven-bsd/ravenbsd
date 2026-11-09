// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! No-allocation parser for the RavenBSD kernel command line.
//!
//! The command line is a whitespace-separated sequence of `key` or `key=value` tokens. Values
//! cannot contain spaces, duplicate keys use the last occurrence, and only the first
//! [`MAX_TOKENS`] tokens are kept. The parser never allocates and never panics.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use core::fmt;

/// Maximum number of tokens retained; later tokens are ignored.
pub const MAX_TOKENS: usize = 64;

/// Kernel log verbosity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
    /// Errors only.
    Error = 0,
    /// Errors and warnings.
    Warn = 1,
    /// Errors, warnings and informational messages.
    #[default]
    Info = 2,
    /// Everything, including debug messages.
    Debug = 3,
}

impl LogLevel {
    /// Parse `error`, `warn`, `info` or `debug`.
    pub const fn parse(value: &str) -> Option<LogLevel> {
        match value.as_bytes() {
            b"error" => Some(LogLevel::Error),
            b"warn" => Some(LogLevel::Warn),
            b"info" => Some(LogLevel::Info),
            b"debug" => Some(LogLevel::Debug),
            _ => None,
        }
    }

    /// Canonical spelling.
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

/// A parsed command line borrowing the original string.
#[derive(Clone, Copy, Debug)]
pub struct CmdLine<'a> {
    tokens: [Option<(&'a str, Option<&'a str>)>; MAX_TOKENS],
    len: usize,
}

impl<'a> CmdLine<'a> {
    /// Parse a command line. Tokens beyond [`MAX_TOKENS`] are ignored; an empty key (`=value`) is
    /// ignored.
    pub fn parse(text: &'a str) -> CmdLine<'a> {
        let mut tokens = [None; MAX_TOKENS];
        let mut len = 0;
        for token in text.split_whitespace() {
            if len == MAX_TOKENS {
                break;
            }
            match token.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    tokens[len] = Some((key, Some(value)));
                    len += 1;
                }
                Some(_) => {}
                None => {
                    tokens[len] = Some((token, None));
                    len += 1;
                }
            }
        }
        CmdLine { tokens, len }
    }

    /// Value of the last occurrence of `key`; a bare key has an empty value.
    pub fn get(&self, key: &str) -> Option<&'a str> {
        for index in (0..self.len).rev() {
            if let Some((candidate, value)) = self.tokens.get(index).copied().flatten()
                && candidate == key
            {
                return Some(value.unwrap_or(""));
            }
        }
        None
    }

    /// Whether `key` is present and either bare or set to `1`, `true`, `yes` or `on`.
    pub fn flag(&self, key: &str) -> bool {
        for index in (0..self.len).rev() {
            if let Some((candidate, value)) = self.tokens.get(index).copied().flatten()
                && candidate == key
            {
                return match value {
                    None => true,
                    Some(value) => matches!(value, "1" | "true" | "yes" | "on"),
                };
            }
        }
        false
    }

    /// Configured log level, defaulting to [`LogLevel::Info`].
    pub fn loglevel(&self) -> LogLevel {
        self.get("loglevel")
            .and_then(LogLevel::parse)
            .unwrap_or(LogLevel::Info)
    }

    /// Configured init path, defaulting to `/init`.
    pub fn init_path(&self) -> &'a str {
        match self.get("init") {
            Some(path) if !path.is_empty() => path,
            _ => "/init",
        }
    }

    /// Whether `test-exit` asks the kernel to use the C5 QEMU exit protocol.
    pub fn test_exit(&self) -> bool {
        self.flag("test-exit")
    }
}

#[cfg(test)]
mod tests {
    use super::{CmdLine, LogLevel, MAX_TOKENS};

    #[test]
    fn empty_line() {
        let line = CmdLine::parse("");
        assert_eq!(line.get("anything"), None);
        assert!(!line.flag("test-exit"));
        assert_eq!(line.loglevel(), LogLevel::Info);
        assert_eq!(line.init_path(), "/init");
    }

    #[test]
    fn whitespace_runs_are_separators() {
        let line = CmdLine::parse("  a=1\t\tb=2\n c=3  ");
        assert_eq!(line.get("a"), Some("1"));
        assert_eq!(line.get("b"), Some("2"));
        assert_eq!(line.get("c"), Some("3"));
    }

    #[test]
    fn bare_key_has_empty_value_and_is_a_flag() {
        let line = CmdLine::parse("quiet test-exit");
        assert_eq!(line.get("quiet"), Some(""));
        assert!(line.flag("quiet"));
        assert!(line.test_exit());
    }

    #[test]
    fn key_value_pair() {
        let line = CmdLine::parse("console=ttyS0");
        assert_eq!(line.get("console"), Some("ttyS0"));
    }

    #[test]
    fn empty_value_is_not_true() {
        let line = CmdLine::parse("test-exit=");
        assert_eq!(line.get("test-exit"), Some(""));
        assert!(!line.flag("test-exit"));
    }

    #[test]
    fn empty_key_is_ignored() {
        let line = CmdLine::parse("=x key=value");
        assert_eq!(line.get(""), None);
        assert_eq!(line.get("key"), Some("value"));
    }

    #[test]
    fn duplicate_keys_last_wins() {
        let line = CmdLine::parse("a=1 a=2 a=3");
        assert_eq!(line.get("a"), Some("3"));
    }

    #[test]
    fn token_limit_ignores_extra() {
        let mut text = String::new();
        for index in 0..MAX_TOKENS + 5 {
            text.push_str(&format!("k{index}=v "));
        }
        let line = CmdLine::parse(&text);
        assert_eq!(line.get("k0"), Some("v"));
        assert_eq!(line.get(&format!("k{}", MAX_TOKENS - 1)), Some("v"));
        assert_eq!(line.get(&format!("k{MAX_TOKENS}")), None);
    }

    #[test]
    fn test_exit_true_variants() {
        for value in ["1", "true", "yes", "on"] {
            let text = format!("test-exit={value}");
            let line = CmdLine::parse(&text);
            assert!(line.test_exit(), "{value}");
        }
    }

    #[test]
    fn test_exit_false_variants() {
        for value in ["0", "false", "no", "off", ""] {
            let text = format!("test-exit={value}");
            let line = CmdLine::parse(&text);
            assert!(!line.test_exit(), "{value}");
        }
    }

    #[test]
    fn test_exit_last_duplicate_wins() {
        assert!(!CmdLine::parse("test-exit=1 test-exit=0").test_exit());
        assert!(CmdLine::parse("test-exit=0 test-exit=1").test_exit());
    }

    #[test]
    fn unknown_loglevel_falls_back_to_info() {
        assert_eq!(CmdLine::parse("loglevel=loud").loglevel(), LogLevel::Info);
    }

    #[test]
    fn every_loglevel_is_accepted() {
        for (text, expected) in [
            ("error", LogLevel::Error),
            ("warn", LogLevel::Warn),
            ("info", LogLevel::Info),
            ("debug", LogLevel::Debug),
        ] {
            assert_eq!(
                CmdLine::parse(&format!("loglevel={text}")).loglevel(),
                expected
            );
        }
    }

    #[test]
    fn loglevel_last_duplicate_wins() {
        assert_eq!(
            CmdLine::parse("loglevel=debug loglevel=error").loglevel(),
            LogLevel::Error
        );
    }

    #[test]
    fn init_path_default_and_custom() {
        assert_eq!(CmdLine::parse("").init_path(), "/init");
        assert_eq!(CmdLine::parse("init=/bin/sh").init_path(), "/bin/sh");
        assert_eq!(CmdLine::parse("init=").init_path(), "/init");
    }

    #[test]
    fn value_may_contain_equals() {
        let line = CmdLine::parse("console=a=b=c");
        assert_eq!(line.get("console"), Some("a=b=c"));
    }

    #[test]
    fn unicode_tokens_are_preserved() {
        let line = CmdLine::parse("name=日本語 key=value");
        assert_eq!(line.get("name"), Some("日本語"));
        assert_eq!(line.get("key"), Some("value"));
    }

    #[test]
    fn long_input_does_not_panic() {
        let text = "x".repeat(1024 * 1024);
        let line = CmdLine::parse(&text);
        assert_eq!(line.get(&text), Some(""));
        assert_eq!(line.get(&"y".repeat(1024)), None);
    }

    #[test]
    fn flags_are_exact_key_matches() {
        let line = CmdLine::parse("test-exit-extra=1 test-exit=0");
        assert!(!line.flag("test-exit"));
        assert!(line.flag("test-exit-extra"));
    }

    #[test]
    fn values_are_case_sensitive() {
        assert!(!CmdLine::parse("test-exit=TRUE").test_exit());
        assert_eq!(CmdLine::parse("loglevel=DEBUG").loglevel(), LogLevel::Info);
    }
}
