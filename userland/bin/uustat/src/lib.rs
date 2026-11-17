// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `uustat` queue listing, summary and cancellation.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use uucp::config::Config;
use uucp::job::Job;
use uucp::queue::{self, QueueError};
use uucp::spool::Spool;

/// Parsed `uustat` options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `-a`: include every user.
    pub all: bool,
    /// `-q`: print the queue summary.
    pub summary: bool,
    /// `-s system`: filter by destination system.
    pub system: Option<String>,
    /// `-u user`: filter by requesting user.
    pub user: Option<String>,
    /// `-k job`: cancel one job.
    pub cancel: Option<u64>,
}

/// Why a `uustat` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is unknown.
    UnknownOption(char),
    /// An option that takes a value has none.
    MissingValue(char),
    /// A job id is not a `u64`.
    BadJob,
    /// An unexpected operand was supplied.
    ExtraOperand,
    /// `-a` and `-u` are mutually exclusive.
    AllAndUser,
}

/// Why a `uustat` operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UustatError {
    /// The queue failed.
    Queue(QueueError),
    /// The output writer failed.
    Output,
}

impl core::fmt::Display for UustatError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Queue(error) => write!(formatter, "{error}"),
            Self::Output => formatter.write_str("output error"),
        }
    }
}

impl From<QueueError> for UustatError {
    fn from(error: QueueError) -> Self {
        Self::Queue(error)
    }
}

/// Parse `uustat [-a] [-q] [-s system] [-u user] [-k job]`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a missing value, a malformed job id, an extra
/// operand or `-a` combined with `-u`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Options, UsageError> {
    let arguments: Vec<&str> = args.into_iter().collect();
    let mut options = Options::default();
    let mut index = 0usize;
    let mut options_enabled = true;
    while index < arguments.len() {
        let argument = *arguments.get(index).ok_or(UsageError::ExtraOperand)?;
        if options_enabled && argument == "--" {
            options_enabled = false;
            index = index.saturating_add(1);
            continue;
        }
        if options_enabled && argument.starts_with('-') && argument.len() > 1 {
            let bytes = argument.as_bytes();
            let mut offset = 1usize;
            while offset < bytes.len() {
                let option = char::from(*bytes.get(offset).ok_or(UsageError::MissingValue('?'))?);
                match option {
                    'a' => options.all = true,
                    'q' => options.summary = true,
                    's' | 'u' | 'k' => {
                        let attached = argument.get(offset.saturating_add(1)..).unwrap_or("");
                        let value = if attached.is_empty() {
                            index = index.saturating_add(1);
                            *arguments
                                .get(index)
                                .ok_or(UsageError::MissingValue(option))?
                        } else {
                            attached
                        };
                        match option {
                            's' => options.system = Some(value.to_string()),
                            'u' => options.user = Some(value.to_string()),
                            'k' => {
                                options.cancel =
                                    Some(value.parse::<u64>().map_err(|_| UsageError::BadJob)?);
                            }
                            _ => {}
                        }
                        offset = bytes.len();
                        continue;
                    }
                    _ => return Err(UsageError::UnknownOption(option)),
                }
                offset = offset.saturating_add(1);
            }
            index = index.saturating_add(1);
            continue;
        }
        return Err(UsageError::ExtraOperand);
    }
    if options.all && options.user.is_some() {
        return Err(UsageError::AllAndUser);
    }
    Ok(options)
}

/// Run one `uustat` invocation, writing the result to `out`.
///
/// # Errors
///
/// Returns [`UustatError`] when the queue or the output writer fails.
pub fn run(
    options: &Options,
    config: &Config,
    spool: &mut dyn Spool,
    current_user: &str,
    out: &mut dyn core::fmt::Write,
) -> Result<(), UustatError> {
    if let Some(id) = options.cancel {
        queue::cancel(spool, config, id)?;
        writeln!(out, "job {id} cancelled").map_err(|_| UustatError::Output)?;
        return Ok(());
    }
    let all_jobs = queue::jobs(spool, config)?;
    let selected: Vec<Job> = all_jobs
        .into_iter()
        .filter(|job| {
            (options.all || job.user == options.user.as_deref().unwrap_or(current_user))
                && options
                    .system
                    .as_deref()
                    .is_none_or(|system| job.system == system)
        })
        .collect();
    if options.summary {
        let summary = queue::summarize(&selected);
        writeln!(
            out,
            "jobs {} systems {} users {}",
            summary.jobs, summary.systems, summary.users
        )
        .map_err(|_| UustatError::Output)?;
        return Ok(());
    }
    for job in selected {
        writeln!(
            out,
            "{} {} {} {} {} queued",
            job.id,
            job.user,
            job.system,
            char::from(job.grade),
            job.kind.label()
        )
        .map_err(|_| UustatError::Output)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
