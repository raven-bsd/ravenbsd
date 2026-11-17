// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument handling for `renice`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// The synopsis printed after `usage: renice`.
pub const USAGE: &str = "[-n] priority [-p|-g|-u] id ...";

/// The target selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Which {
    /// A process id.
    #[default]
    Process,
    /// A process group id.
    Group,
    /// A user id.
    User,
}

impl Which {
    /// The `PRIO_*` value used by the runtime.
    #[must_use]
    pub const fn prio(self) -> u32 {
        match self {
            Self::Process => 0,
            Self::Group => 1,
            Self::User => 2,
        }
    }
}

/// A parsed `renice` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Absolute nice value.
    pub priority: i32,
    /// Target selector.
    pub which: Which,
    /// Target ids.
    pub ids: Vec<i32>,
}

/// Why a `renice` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// Unknown option.
    UnknownOption,
    /// `-n` without a value.
    MissingValue,
    /// Priority is not a number.
    BadPriority,
    /// A target id is not a number.
    BadId,
    /// No target ids.
    MissingIds,
}

/// Parse `renice [-n] priority [-p|-g|-u] id ...`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut priority = None;
    let mut which = Which::Process;
    let mut ids = Vec::new();
    let mut args = args.into_iter().peekable();
    let mut parsing = true;
    while let Some(argument) = args.next() {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && argument == "-n" {
            let value = args.next().ok_or(UsageError::MissingValue)?;
            priority = Some(value.parse::<i32>().map_err(|_| UsageError::BadPriority)?);
            continue;
        }
        if parsing && (argument == "-p" || argument == "--pid") {
            which = Which::Process;
            continue;
        }
        if parsing && (argument == "-g" || argument == "--pgrp") {
            which = Which::Group;
            continue;
        }
        if parsing && (argument == "-u" || argument == "--user") {
            which = Which::User;
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            return Err(UsageError::UnknownOption);
        }
        if priority.is_none() {
            priority = Some(
                argument
                    .parse::<i32>()
                    .map_err(|_| UsageError::BadPriority)?,
            );
        } else {
            ids.push(argument.parse::<i32>().map_err(|_| UsageError::BadId)?);
        }
    }
    let Some(priority) = priority else {
        return Err(UsageError::BadPriority);
    };
    if ids.is_empty() {
        return Err(UsageError::MissingIds);
    }
    Ok(Options {
        priority,
        which,
        ids,
    })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
