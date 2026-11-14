// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parsing of ATF result files and mapping an expectation to a verdict (ticket C71, 4.6/4.7).
//!
//! The ATF program writes the result file itself (`atf-c/tc.c`); the runner never guesses. A
//! missing or unparsable file is a failure, not a pass.

use std::fmt;

use crate::{Expect, ProcessStatus, Verdict};

/// Error returned when a result file is empty or not in the ATF grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResultError {
    /// The file is empty (or only whitespace).
    Empty,
    /// The first line is not one of the ATF result directives.
    Unknown(String),
    /// A recognised directive has a malformed argument.
    Malformed(String),
}

impl fmt::Display for ResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("empty result file"),
            Self::Unknown(line) => write!(formatter, "unknown result directive `{line}`"),
            Self::Malformed(detail) => write!(formatter, "malformed result directive: {detail}"),
        }
    }
}

impl std::error::Error for ResultError {}

/// Parse the first line of an ATF result file.
pub fn parse_result(text: &str) -> Result<Expect, ResultError> {
    let first = text.lines().next().unwrap_or("").trim_end_matches('\r');
    if first.trim().is_empty() {
        return Err(ResultError::Empty);
    }
    match first {
        "passed" => Ok(Expect::Pass),
        _ => {
            if let Some(reason) = first.strip_prefix("failed: ") {
                return Ok(Expect::Fail(reason.to_string()));
            }
            if let Some(reason) = first.strip_prefix("skipped: ") {
                return Ok(Expect::Skip(reason.to_string()));
            }
            if let Some(reason) = first.strip_prefix("expected_failure: ") {
                return Ok(Expect::XFail(reason.to_string()));
            }
            if let Some(reason) = first.strip_prefix("expected_death: ") {
                return Ok(Expect::Death(reason.to_string()));
            }
            if let Some(reason) = first.strip_prefix("expected_timeout: ") {
                return Ok(Expect::Timeout(reason.to_string()));
            }
            if let Some(rest) = first.strip_prefix("expected_exit(") {
                let (value, reason) = split_argument(rest, "expected_exit")?;
                return Ok(Expect::ExitCode(parse_number(value)?, reason));
            }
            if let Some(rest) = first.strip_prefix("expected_signal(") {
                let (value, reason) = split_argument(rest, "expected_signal")?;
                return Ok(Expect::Signal(parse_number(value)?, reason));
            }
            Err(ResultError::Unknown(truncate(first, 120)))
        }
    }
}

/// Parse the `N): reason` tail of an `expected_*` directive.
fn split_argument<'a>(rest: &'a str, directive: &str) -> Result<(&'a str, String), ResultError> {
    let Some(close) = rest.find(')') else {
        return Err(ResultError::Malformed(format!(
            "{directive} has no closing `)`"
        )));
    };
    let value = &rest[..close];
    let tail = &rest[close + 1..];
    let Some(reason) = tail.strip_prefix(": ") else {
        return Err(ResultError::Malformed(format!(
            "{directive} has no `: reason` part"
        )));
    };
    Ok((value, reason.to_string()))
}

/// Parse an ATF integer argument; `-1` is the wildcard.
fn parse_number(value: &str) -> Result<Option<i32>, ResultError> {
    let number: i32 = value
        .parse()
        .map_err(|_| ResultError::Malformed(format!("`{value}` is not an integer")))?;
    Ok((number >= 0).then_some(number))
}

/// Judge one result file against how the process actually ended.
///
/// `timeout_secs` is the wall-clock cap that was applied; it is only used in the `timeout`
/// failure reason.
#[must_use]
pub fn judge(
    expect: &Expect,
    status: ProcessStatus,
    timed_out: bool,
    timeout_secs: u64,
) -> (Verdict, String) {
    if timed_out && !matches!(expect, Expect::Timeout(_)) {
        return (Verdict::Fail, format!("timeout after {timeout_secs}s"));
    }
    match expect {
        Expect::Pass => (Verdict::Pass, String::new()),
        Expect::Fail(reason) => (Verdict::Fail, reason.clone()),
        Expect::Skip(reason) => (Verdict::Skip, reason.clone()),
        Expect::XFail(reason) => (Verdict::Pass, format!("xfail:{reason}")),
        Expect::ExitCode(wanted, _) => match status {
            ProcessStatus::Exited(code) if wanted.is_none_or(|wanted| wanted == code) => {
                (Verdict::Pass, String::new())
            }
            _ => (
                Verdict::Fail,
                format!(
                    "behaviour:expected exit {}, got {}",
                    wanted.map_or_else(|| "-1".to_string(), |code| code.to_string()),
                    status_value(status)
                ),
            ),
        },
        Expect::Signal(wanted, _) => match status {
            ProcessStatus::Signaled(signal) if wanted.is_none_or(|wanted| wanted == signal) => {
                (Verdict::Pass, String::new())
            }
            _ => (
                Verdict::Fail,
                format!(
                    "behaviour:expected signal {}, got {}",
                    wanted.map_or_else(|| "-1".to_string(), |signal| signal.to_string()),
                    status_value(status)
                ),
            ),
        },
        Expect::Death(_) => match status {
            ProcessStatus::Signaled(_) => (Verdict::Pass, String::new()),
            ProcessStatus::Exited(code) if code != 0 => (Verdict::Pass, String::new()),
            _ => (
                Verdict::Fail,
                format!("behaviour:expected death, got {}", status_value(status)),
            ),
        },
        Expect::Timeout(_) => {
            if timed_out {
                (Verdict::Pass, String::new())
            } else {
                (
                    Verdict::Fail,
                    format!("behaviour:expected timeout, got {}", status_value(status)),
                )
            }
        }
    }
}

/// Judge a case for which no result file was produced.
#[must_use]
pub fn judge_missing(
    status: ProcessStatus,
    timed_out: bool,
    timeout_secs: u64,
) -> (Verdict, String) {
    if timed_out {
        return (Verdict::Fail, format!("timeout after {timeout_secs}s"));
    }
    (
        Verdict::Fail,
        format!("no result file ({})", status.describe()),
    )
}

/// Judge an unparsable result file.
#[must_use]
pub fn judge_unparsable(bytes: &[u8], status: ProcessStatus) -> (Verdict, String) {
    let text = String::from_utf8_lossy(bytes);
    let first_line = text.lines().next().unwrap_or("");
    (
        Verdict::Fail,
        format!(
            "unparsable result: {} ({})",
            truncate(first_line, 80),
            status.describe()
        ),
    )
}

/// The status without the "exit"/"signal" prefix, for the ticket's reason wording.
fn status_value(status: ProcessStatus) -> String {
    match status {
        ProcessStatus::Exited(code) => code.to_string(),
        ProcessStatus::Signaled(signal) => format!("signal {signal}"),
    }
}

/// Truncate a string for a one-line reason.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

#[cfg(test)]
#[path = "result_ut.rs"]
mod tests;
