// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure signal-name and command-line handling for `kill`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The synopsis printed after `usage: kill`.
pub const USAGE: &str = "[-s signal | -signal] pid ... | -l [name | number]";

/// One named signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal {
    /// Signal number.
    pub number: u32,
    /// Uppercase name without the `SIG` prefix.
    pub name: &'static str,
}

/// The standard signals this implementation names.
pub const SIGNALS: &[Signal] = &[
    Signal {
        number: 1,
        name: "HUP",
    },
    Signal {
        number: 2,
        name: "INT",
    },
    Signal {
        number: 3,
        name: "QUIT",
    },
    Signal {
        number: 4,
        name: "ILL",
    },
    Signal {
        number: 5,
        name: "TRAP",
    },
    Signal {
        number: 6,
        name: "ABRT",
    },
    Signal {
        number: 7,
        name: "BUS",
    },
    Signal {
        number: 8,
        name: "FPE",
    },
    Signal {
        number: 9,
        name: "KILL",
    },
    Signal {
        number: 10,
        name: "USR1",
    },
    Signal {
        number: 11,
        name: "SEGV",
    },
    Signal {
        number: 12,
        name: "USR2",
    },
    Signal {
        number: 13,
        name: "PIPE",
    },
    Signal {
        number: 14,
        name: "ALRM",
    },
    Signal {
        number: 15,
        name: "TERM",
    },
    Signal {
        number: 16,
        name: "STKFLT",
    },
    Signal {
        number: 17,
        name: "CHLD",
    },
    Signal {
        number: 18,
        name: "CONT",
    },
    Signal {
        number: 19,
        name: "STOP",
    },
    Signal {
        number: 20,
        name: "TSTP",
    },
    Signal {
        number: 21,
        name: "TTIN",
    },
    Signal {
        number: 22,
        name: "TTOU",
    },
    Signal {
        number: 23,
        name: "URG",
    },
    Signal {
        number: 24,
        name: "XCPU",
    },
    Signal {
        number: 25,
        name: "XFSZ",
    },
    Signal {
        number: 26,
        name: "VTALRM",
    },
    Signal {
        number: 27,
        name: "PROF",
    },
    Signal {
        number: 28,
        name: "WINCH",
    },
    Signal {
        number: 29,
        name: "IO",
    },
    Signal {
        number: 30,
        name: "PWR",
    },
    Signal {
        number: 31,
        name: "SYS",
    },
];

/// A parsed `kill` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Print names or numbers.
    List {
        /// A signal name or number, or `None` for the full list.
        query: Option<String>,
    },
    /// Send `signal` to each target; negative targets address process groups.
    Send {
        /// Signal number, including 0 for the existence check.
        signal: u32,
        /// Pid or process-group targets.
        targets: Vec<i32>,
    },
}

/// Why a `kill` command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option or malformed signal.
    BadSignal,
    /// No target was supplied.
    MissingTarget,
    /// A target is not a number.
    BadTarget,
}

/// Look up a signal name.
#[must_use]
pub fn by_name(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    let bare = upper.strip_prefix("SIG").unwrap_or(upper.as_str());
    SIGNALS
        .iter()
        .find(|signal| signal.name == bare)
        .map(|signal| signal.number)
}

/// Look up a signal number.
#[must_use]
pub fn by_number(number: u32) -> Option<&'static str> {
    SIGNALS
        .iter()
        .find(|signal| signal.number == number)
        .map(|signal| signal.name)
}

/// Parse a signal name or number, including signal 0.
pub fn parse_signal(text: &str) -> Option<u32> {
    if text == "0" {
        return Some(0);
    }
    if let Ok(number) = text.parse::<u32>() {
        if number <= 64 {
            return Some(number);
        }
        return None;
    }
    by_name(text)
}

/// Parse `kill [-s signal | -signal] pid ... | -l [name | number]`.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Action, UsageError> {
    let mut args = args.into_iter().peekable();
    let mut signal = 15u32;
    let mut targets = Vec::new();
    let mut parsing = true;
    while let Some(argument) = args.next() {
        if parsing && argument == "--" {
            parsing = false;
            continue;
        }
        if parsing && (argument == "-l" || argument == "--list") {
            let query = args.next().map(str::to_string);
            return Ok(Action::List { query });
        }
        if parsing && argument == "-s" {
            let Some(value) = args.next() else {
                return Err(UsageError::BadSignal);
            };
            signal = parse_signal(value).ok_or(UsageError::BadSignal)?;
            continue;
        }
        if parsing && argument.starts_with('-') && argument != "-" {
            let text = argument.strip_prefix('-').ok_or(UsageError::BadSignal)?;
            signal = parse_signal(text).ok_or(UsageError::BadSignal)?;
            continue;
        }
        let target = argument.parse::<i32>().map_err(|_| UsageError::BadTarget)?;
        targets.push(target);
    }
    if targets.is_empty() {
        return Err(UsageError::MissingTarget);
    }
    Ok(Action::Send { signal, targets })
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
