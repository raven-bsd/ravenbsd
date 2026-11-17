// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX crontab parsing, validation and spool operations.
//!
//! The five time fields support `*`, numbers, ranges, comma lists and `/step`. Commands must be
//! non-empty. `@reboot` is a documented unsupported case; the parser returns an error so neither
//! `crontab` nor `crond` silently installs it.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Default spool directory.
pub const DEFAULT_SPOOL: &str = "/var/cron";
/// Environment override used by the host tests and the qtest fixture.
pub const SPOOL_ENV: &str = "CRON_SPOOL";
/// Maximum accepted spool size.
pub const MAX_SPOOL: usize = 256 * 1024;

/// One parsed crontab entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Minute field.
    pub minute: String,
    /// Hour field.
    pub hour: String,
    /// Day-of-month field.
    pub day: String,
    /// Month field.
    pub month: String,
    /// Day-of-week field (0 and 7 are Sunday).
    pub weekday: String,
    /// Command line.
    pub command: String,
}

/// One `crontab` action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Install from this file; `-` means standard input.
    Install(String),
    /// List the current spool.
    List,
    /// Remove the current spool.
    Remove,
}

/// Storage operations used by the utility and its host tests.
pub trait Spool {
    /// Read a spool file.
    fn read(&mut self, path: &str) -> Result<String, String>;
    /// Atomically replace a spool file.
    fn write_atomic(&mut self, path: &str, text: &str) -> Result<(), String>;
    /// Remove a spool file.
    fn remove(&mut self, path: &str) -> Result<(), String>;
    /// Whether a spool file exists.
    fn exists(&mut self, path: &str) -> bool;
}

/// One civil time used by the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    /// Calendar year.
    pub year: i64,
    /// Month, 1..=12.
    pub month: u32,
    /// Day of month, 1..=31.
    pub day: u32,
    /// Hour, 0..=23.
    pub hour: u32,
    /// Minute, 0..=59.
    pub minute: u32,
    /// Day of week, 0 = Sunday.
    pub weekday: u32,
}

/// Convert Unix seconds to a civil UTC time.
#[must_use]
pub fn civil_from_unix(seconds: i64) -> Civil {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let weekday = (days + 4).rem_euclid(7) as u32;
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    Civil {
        year,
        month,
        day,
        hour,
        minute,
        weekday,
    }
}

/// Parse a whole crontab.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    if text.len() > MAX_SPOOL {
        return Err("crontab is too large".to_string());
    }
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            return Err(format!("line {}: @reboot is unsupported", index + 1));
        }
        let mut fields = line.split_whitespace();
        let minute = fields
            .next()
            .ok_or_else(|| format!("line {}: missing minute", index + 1))?;
        let hour = fields
            .next()
            .ok_or_else(|| format!("line {}: missing hour", index + 1))?;
        let day = fields
            .next()
            .ok_or_else(|| format!("line {}: missing day", index + 1))?;
        let month = fields
            .next()
            .ok_or_else(|| format!("line {}: missing month", index + 1))?;
        let weekday = fields
            .next()
            .ok_or_else(|| format!("line {}: missing weekday", index + 1))?;
        let command = fields.collect::<Vec<_>>().join(" ");
        if command.is_empty() {
            return Err(format!("line {}: missing command", index + 1));
        }
        validate_field(minute, 0, 59)
            .map_err(|error| format!("line {}: minute {error}", index + 1))?;
        validate_field(hour, 0, 23).map_err(|error| format!("line {}: hour {error}", index + 1))?;
        validate_field(day, 1, 31).map_err(|error| format!("line {}: day {error}", index + 1))?;
        validate_field(month, 1, 12)
            .map_err(|error| format!("line {}: month {error}", index + 1))?;
        validate_field(weekday, 0, 7)
            .map_err(|error| format!("line {}: weekday {error}", index + 1))?;
        entries.push(Entry {
            minute: minute.to_string(),
            hour: hour.to_string(),
            day: day.to_string(),
            month: month.to_string(),
            weekday: weekday.to_string(),
            command,
        });
    }
    Ok(entries)
}

fn validate_field(field: &str, minimum: u32, maximum: u32) -> Result<(), String> {
    for part in field.split(',') {
        validate_part(part, minimum, maximum)?;
    }
    Ok(())
}

fn validate_part(part: &str, minimum: u32, maximum: u32) -> Result<(), String> {
    if part.is_empty() {
        return Err("empty list element".to_string());
    }
    let (range, step) = match part.split_once('/') {
        Some((range, step)) => {
            let step: u32 = step.parse().map_err(|_| "bad step".to_string())?;
            if step == 0 {
                return Err("zero step".to_string());
            }
            (range, step)
        }
        None => (part, 1),
    };
    if range == "*" {
        return Ok(());
    }
    let (start, end) = match range.split_once('-') {
        Some((start, end)) => (
            start
                .parse::<u32>()
                .map_err(|_| "bad range start".to_string())?,
            end.parse::<u32>()
                .map_err(|_| "bad range end".to_string())?,
        ),
        None => {
            let value = range.parse::<u32>().map_err(|_| "bad value".to_string())?;
            (value, value)
        }
    };
    if start > end || start < minimum || end > maximum {
        return Err("outside the field range".to_string());
    }
    if step > maximum.saturating_sub(minimum).saturating_add(1) {
        return Err("step exceeds the field range".to_string());
    }
    Ok(())
}

/// Whether one field matches `value`.
pub fn field_matches(field: &str, value: u32, minimum: u32, maximum: u32) -> bool {
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, step.parse::<u32>().unwrap_or(0)),
            None => (part, 1),
        };
        if step == 0 {
            continue;
        }
        let (start, end) = if range == "*" {
            (minimum, maximum)
        } else if let Some((start, end)) = range.split_once('-') {
            match (start.parse::<u32>(), end.parse::<u32>()) {
                (Ok(start), Ok(end)) => (start, end),
                _ => continue,
            }
        } else {
            match range.parse::<u32>() {
                Ok(value) => (value, value),
                Err(_) => continue,
            }
        };
        if value >= start && value <= end && (value - start).is_multiple_of(step) {
            return true;
        }
    }
    false
}

/// Whether an entry matches a wall-clock time.
pub fn entry_matches(
    entry: &Entry,
    minute: u32,
    hour: u32,
    day: u32,
    month: u32,
    weekday: u32,
) -> bool {
    if !field_matches(&entry.minute, minute, 0, 59)
        || !field_matches(&entry.hour, hour, 0, 23)
        || !field_matches(&entry.month, month, 1, 12)
    {
        return false;
    }
    let day_restricted = entry.day != "*";
    let weekday_restricted = entry.weekday != "*";
    let day_match = field_matches(&entry.day, day, 1, 31);
    let weekday_match = field_matches(&entry.weekday, weekday, 0, 7)
        || (weekday == 0 && field_matches(&entry.weekday, 7, 0, 7));
    if day_restricted && weekday_restricted {
        day_match || weekday_match
    } else {
        day_match && weekday_match
    }
}

/// Parse `crontab` options.
pub fn action(args: &[String]) -> Result<Action, String> {
    let mut list = false;
    let mut remove = false;
    let mut file = None;
    for arg in args {
        match arg.as_str() {
            "-l" => list = true,
            "-r" => remove = true,
            "-" => file = Some("-".to_string()),
            value if value.starts_with('-') => return Err(format!("unsupported option {value}")),
            value => {
                if file.is_some() {
                    return Err("too many file operands".to_string());
                }
                file = Some(value.to_string());
            }
        }
    }
    if list && remove {
        return Err("-l and -r are mutually exclusive".to_string());
    }
    if list {
        if file.is_some() {
            return Err("-l takes no file operand".to_string());
        }
        return Ok(Action::List);
    }
    if remove {
        if file.is_some() {
            return Err("-r takes no file operand".to_string());
        }
        return Ok(Action::Remove);
    }
    Ok(Action::Install(file.unwrap_or_else(|| "-".to_string())))
}

/// The spool directory, honouring [`SPOOL_ENV`].
pub fn spool_dir() -> String {
    #[cfg(not(target_os = "none"))]
    {
        std::env::var(SPOOL_ENV).unwrap_or_else(|_| DEFAULT_SPOOL.to_string())
    }
    #[cfg(target_os = "none")]
    {
        DEFAULT_SPOOL.to_string()
    }
}

/// The spool path for `user`.
pub fn spool_path(user: &str) -> String {
    format!("{}/{}", spool_dir(), user)
}

/// Install validated text into the user spool.
pub fn install<S: Spool>(spool: &mut S, path: &str, text: &str) -> Result<(), String> {
    parse(text)?;
    spool.write_atomic(path, text)
}

/// List the user spool.
pub fn list<S: Spool>(spool: &mut S, path: &str) -> Result<String, String> {
    spool.read(path)
}

/// Remove the user spool.
pub fn remove<S: Spool>(spool: &mut S, path: &str) -> Result<(), String> {
    if !spool.exists(path) {
        return Err("no crontab for this user".to_string());
    }
    spool.remove(path)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
