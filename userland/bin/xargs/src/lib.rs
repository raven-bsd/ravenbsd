// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `xargs`: read items, batch them and run a command.
//!
//! The library owns option parsing, input splitting, batching, replacement and status aggregation.
//! Command execution is supplied by a [`Runner`], so the host binary uses `std::process` and the
//! target binary uses the landed `raven_rt::spawn` path.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Default maximum command-line size in bytes (`-s`).
///
/// This is a bounded value, not the host `ARG_MAX`; the ticket requires a bounded utility.
pub const DEFAULT_MAX_SIZE: usize = 4096;

/// Why an xargs invocation or input was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XargsError {
    /// The command line is invalid.
    Usage,
    /// An input quote was not closed.
    UnmatchedQuote,
    /// An input backslash had no following character.
    TrailingBackslash,
    /// A command line would exceed `-s` with `-x`.
    SizeExceeded,
    /// The child command could not be found.
    CommandNotFound,
    /// The child command was found but could not be started.
    CommandNotInvokable,
}

impl core::fmt::Display for XargsError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid usage"),
            Self::UnmatchedQuote => formatter.write_str("unmatched quote in input"),
            Self::TrailingBackslash => formatter.write_str("trailing backslash in input"),
            Self::SizeExceeded => formatter.write_str("command line too long"),
            Self::CommandNotFound => formatter.write_str("command not found"),
            Self::CommandNotInvokable => formatter.write_str("command could not be invoked"),
        }
    }
}

/// Parsed options and command template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Split input on NUL bytes (`-0`).
    pub null: bool,
    /// Maximum items per invocation (`-n`).
    pub max_args: Option<usize>,
    /// Replace this string in the command (`-I`).
    pub replace: Option<String>,
    /// Do not run the command for empty input (`-r`).
    pub no_run_if_empty: bool,
    /// Write each command to standard error (`-t`).
    pub trace: bool,
    /// Treat an oversized command as an error (`-x` or implied by `-I`).
    pub exit_if_size: bool,
    /// Maximum command-line size in bytes (`-s`).
    pub max_size: usize,
    /// Command and initial arguments.
    pub command: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            null: false,
            max_args: None,
            replace: None,
            no_run_if_empty: false,
            trace: false,
            exit_if_size: false,
            max_size: DEFAULT_MAX_SIZE,
            command: vec!["echo".to_string()],
        }
    }
}

/// Parse `-0`, `-n`, `-I`, `-r`, `-t`, `-x`, `-s`, `--` and the command.
pub fn parse_args<I, S>(arguments: I) -> Result<Options, XargsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect();
    let mut options = Options::default();
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument == "--" {
            index = index.saturating_add(1);
            break;
        }
        if !argument.starts_with('-') || argument == "-" {
            break;
        }
        match argument.as_str() {
            "-0" => options.null = true,
            "-r" => options.no_run_if_empty = true,
            "-t" => options.trace = true,
            "-x" => options.exit_if_size = true,
            _ => {
                if let Some(value) = argument.strip_prefix("-n") {
                    let value = option_value(value, &arguments, &mut index)?;
                    options.max_args = Some(parse_positive(value)?);
                } else if let Some(value) = argument.strip_prefix("-s") {
                    let value = option_value(value, &arguments, &mut index)?;
                    options.max_size = parse_positive(value)?;
                } else if let Some(value) = argument.strip_prefix("-I") {
                    let value = option_value(value, &arguments, &mut index)?;
                    if value.is_empty() {
                        return Err(XargsError::Usage);
                    }
                    options.replace = Some(value.to_string());
                    options.exit_if_size = true;
                } else {
                    return Err(XargsError::Usage);
                }
            }
        }
        index = index.saturating_add(1);
    }
    if index < arguments.len() {
        options.command = arguments.get(index..).unwrap_or(&[]).to_vec();
    }
    if options.command.is_empty() {
        options.command = vec!["echo".to_string()];
    }
    Ok(options)
}

fn option_value<'a>(
    attached: &'a str,
    arguments: &'a [String],
    index: &mut usize,
) -> Result<&'a str, XargsError> {
    if attached.is_empty() {
        *index = (*index).saturating_add(1);
        arguments
            .get(*index)
            .map(String::as_str)
            .ok_or(XargsError::Usage)
    } else {
        Ok(attached)
    }
}

fn parse_positive(value: &str) -> Result<usize, XargsError> {
    let parsed = value.parse::<usize>().map_err(|_| XargsError::Usage)?;
    if parsed == 0 {
        return Err(XargsError::Usage);
    }
    Ok(parsed)
}

/// What a child process did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildStatus {
    /// The child exited with this code.
    Code(i32),
    /// The child was terminated by a signal.
    Signal(i32),
}

/// Why a child could not be started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    /// The command was not found.
    NotFound,
    /// The command was found but could not be invoked.
    CannotInvoke,
}

/// Command execution supplied by the host or target binary.
pub trait Runner {
    /// Write one command to standard error for `-t`.
    fn trace(&mut self, command: &[String]);
    /// Run one command and wait for it.
    fn run(&mut self, command: &[String]) -> Result<ChildStatus, RunError>;
}

/// Aggregate result of one xargs invocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// POSIX xargs exit status.
    pub exit_code: i32,
    /// Commands actually run.
    pub invocations: usize,
}

/// Read items from `input`, batch them and run `options.command` through `runner`.
pub fn run(input: &str, options: &Options, runner: &mut dyn Runner) -> Result<Outcome, XargsError> {
    let mut outcome = Outcome::default();
    if let Some(replacement) = &options.replace {
        let items = if options.null {
            split_nul(input)
        } else {
            split_lines(input)
        };
        for item in items {
            let mut command = Vec::with_capacity(options.command.len());
            for part in &options.command {
                command.push(part.replace(replacement, &item));
            }
            if command_size(&command) > options.max_size {
                return Err(XargsError::SizeExceeded);
            }
            invoke(&command, options, runner, &mut outcome)?;
        }
        return Ok(outcome);
    }
    let items = if options.null {
        split_nul(input)
    } else {
        split_items(input)?
    };
    if items.is_empty() {
        if options.no_run_if_empty {
            return Ok(outcome);
        }
        invoke(&options.command, options, runner, &mut outcome)?;
        return Ok(outcome);
    }
    let mut batch: Vec<String> = Vec::new();
    let mut size = command_size(&options.command);
    for item in items {
        let item_size = item.len().saturating_add(1);
        let full = options
            .max_args
            .is_some_and(|maximum| batch.len() >= maximum);
        if options.exit_if_size
            && options.max_args.is_some()
            && !batch.is_empty()
            && size.saturating_add(item_size) > options.max_size
        {
            return Err(XargsError::SizeExceeded);
        }
        if full || (!batch.is_empty() && size.saturating_add(item_size) > options.max_size) {
            flush_batch(&mut batch, options, runner, &mut outcome)?;
            size = command_size(&options.command);
        }
        if size.saturating_add(item_size) > options.max_size {
            return Err(XargsError::SizeExceeded);
        }
        size = size.saturating_add(item_size);
        batch.push(item);
        if options
            .max_args
            .is_some_and(|maximum| batch.len() >= maximum)
        {
            flush_batch(&mut batch, options, runner, &mut outcome)?;
            size = command_size(&options.command);
        }
    }
    flush_batch(&mut batch, options, runner, &mut outcome)?;
    Ok(outcome)
}

fn flush_batch(
    batch: &mut Vec<String>,
    options: &Options,
    runner: &mut dyn Runner,
    outcome: &mut Outcome,
) -> Result<(), XargsError> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut command = options.command.clone();
    command.append(batch);
    invoke(&command, options, runner, outcome)
}

fn invoke(
    command: &[String],
    options: &Options,
    runner: &mut dyn Runner,
    outcome: &mut Outcome,
) -> Result<(), XargsError> {
    if options.trace {
        runner.trace(command);
    }
    let status = runner.run(command).map_err(|error| match error {
        RunError::NotFound => XargsError::CommandNotFound,
        RunError::CannotInvoke => XargsError::CommandNotInvokable,
    })?;
    outcome.invocations = outcome.invocations.saturating_add(1);
    let code = match status {
        ChildStatus::Code(0) => 0,
        ChildStatus::Code(255) => 124,
        ChildStatus::Code(_) => 123,
        ChildStatus::Signal(_) => 125,
    };
    outcome.exit_code = merge_status(outcome.exit_code, code);
    Ok(())
}

fn merge_status(current: i32, next: i32) -> i32 {
    if current == 0 {
        next
    } else if next == 0 {
        current
    } else {
        current.max(next)
    }
}

fn command_size(command: &[String]) -> usize {
    command.iter().fold(0usize, |total, argument| {
        total.saturating_add(argument.len() + 1)
    })
}

fn split_nul(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut items: Vec<String> = input.split('\0').map(ToString::to_string).collect();
    if input.ends_with('\0') {
        items.pop();
    }
    items
}

fn split_lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let trailing = input.ends_with('\n');
    let mut lines: Vec<String> = input
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect();
    if trailing {
        lines.pop();
    }
    lines
}

fn split_items(input: &str) -> Result<Vec<String>, XargsError> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut characters = input.chars();
    while let Some(character) = characters.next() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else if character == '\\' && delimiter == '"' {
                let Some(next) = characters.next() else {
                    return Err(XargsError::TrailingBackslash);
                };
                current.push(next);
                started = true;
            } else {
                current.push(character);
                started = true;
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                started = true;
            }
            '\\' => {
                let Some(next) = characters.next() else {
                    return Err(XargsError::TrailingBackslash);
                };
                current.push(next);
                started = true;
            }
            ' ' | '\t' | '\n' | '\r' => {
                if started {
                    items.push(core::mem::take(&mut current));
                    started = false;
                }
            }
            other => {
                current.push(other);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(XargsError::UnmatchedQuote);
    }
    if started {
        items.push(current);
    }
    Ok(items)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
