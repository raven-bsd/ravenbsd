// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for `\raven\boot.toml` (CONTRACTS C3).
//!
//! This is deliberately not a TOML implementation: the file is a small line-based sequence of
//! `key = "value"` entries. It is pure and free of UEFI types so the host tests can exercise every
//! error path. K27 adds the optional A/B keys `slot`, `trial` and `attempts`; a file without them
//! keeps the legacy single-slot behaviour.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write as _;

/// Longest accepted value, in bytes. Values of one MiB or more are rejected.
pub const MAX_VALUE_LEN: usize = 1024 * 1024;

/// Largest accepted trial-boot budget.
pub const MAX_ATTEMPTS: u32 = 255;

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

/// One root slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    /// First root slot.
    #[default]
    A,
    /// Second root slot.
    B,
}

impl Slot {
    /// Parse a slot name.
    pub fn parse(value: &str) -> Option<Slot> {
        match value {
            "a" => Some(Slot::A),
            "b" => Some(Slot::B),
            _ => None,
        }
    }

    /// The canonical spelling used in `boot.toml` and slot paths.
    pub const fn as_str(self) -> &'static str {
        match self {
            Slot::A => "a",
            Slot::B => "b",
        }
    }

    /// The other slot.
    pub const fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

impl fmt::Display for Slot {
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
    /// Confirmed (known-good) root slot.
    pub slot: Slot,
    /// Slot being trialled, if any.
    pub trial: Option<Slot>,
    /// Token that the userland health marker must repeat.
    pub trial_token: Option<u32>,
    /// Remaining trial boots before rollback.
    pub attempts: u32,
    /// True when `slot`, `trial` or `attempts` appeared in the file.
    pub has_slot_config: bool,
}

impl Config {
    /// The slot selected for this boot.
    pub fn boot_slot(&self) -> Slot {
        match self.trial {
            Some(trial) if self.attempts > 0 => trial,
            _ => self.slot,
        }
    }

    /// Mark a trial slot as healthy: it becomes the confirmed slot.
    pub fn mark_healthy(&mut self, slot: Slot) {
        self.slot = slot;
        self.trial = None;
        self.trial_token = None;
        self.attempts = 0;
        self.has_slot_config = true;
    }

    /// Drop the trial and keep the confirmed slot.
    pub fn rollback(&mut self) {
        self.trial = None;
        self.trial_token = None;
        self.attempts = 0;
        self.has_slot_config = true;
    }

    /// Consume one failed trial attempt.
    pub fn fail_attempt(&mut self) {
        self.attempts = self.attempts.saturating_sub(1);
        self.has_slot_config = true;
    }

    /// Render the loader's line-based format.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "cmdline = \"{}\"", escape(&self.cmdline));
        let _ = writeln!(out, "log_level = \"{}\"", self.log_level.as_str());
        if self.has_slot_config {
            let _ = writeln!(out, "slot = \"{}\"", self.slot.as_str());
            let _ = writeln!(out, "trial = \"{}\"", self.trial.map_or("", Slot::as_str));
            let _ = writeln!(
                out,
                "trial_token = \"{}\"",
                self.trial_token
                    .map_or_else(String::new, |token| format!("{token:08x}"))
            );
            let _ = writeln!(out, "attempts = \"{}\"", self.attempts);
        }
        out
    }
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
    /// `slot` or `trial` is not `a` or `b`.
    InvalidSlot(usize),
    /// `attempts` is not an integer in `0..=MAX_ATTEMPTS`.
    InvalidAttempts(usize),
    /// `trial_token` is not a 32-bit hexadecimal token.
    InvalidTrialToken(usize),
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
            ConfigError::InvalidSlot(line) => {
                write!(f, "line {line}: slot must be a or b")
            }
            ConfigError::InvalidAttempts(line) => write!(
                f,
                "line {line}: attempts must be an integer in 0..={MAX_ATTEMPTS}"
            ),
            ConfigError::InvalidTrialToken(line) => {
                write!(f, "line {line}: trial_token must be 1-8 hex digits")
            }
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
            "slot" => {
                config.slot = Slot::parse(value).ok_or(ConfigError::InvalidSlot(line_number))?;
                config.has_slot_config = true;
            }
            "trial" => {
                config.trial = if value.is_empty() {
                    None
                } else {
                    Some(Slot::parse(value).ok_or(ConfigError::InvalidSlot(line_number))?)
                };
                config.has_slot_config = true;
            }
            "attempts" => {
                let attempts = value
                    .parse::<u32>()
                    .map_err(|_| ConfigError::InvalidAttempts(line_number))?;
                if attempts > MAX_ATTEMPTS {
                    return Err(ConfigError::InvalidAttempts(line_number));
                }
                config.attempts = attempts;
                config.has_slot_config = true;
            }
            "trial_token" => {
                config.trial_token = if value.is_empty() {
                    None
                } else {
                    Some(
                        u32::from_str_radix(value, 16)
                            .map_err(|_| ConfigError::InvalidTrialToken(line_number))?,
                    )
                };
                config.has_slot_config = true;
            }
            _ => warnings.push(Warning {
                line: line_number,
                key: String::from(key),
            }),
        }
    }

    Ok((config, warnings))
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
