// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared gate vocabulary and record types.

use serde::{Deserialize, Serialize};

/// The exact status vocabulary of `tests/libc/GATE.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Status {
    /// The command ran, the pass rule holds and the evidence file exists.
    #[serde(rename = "MET")]
    Met,
    /// The command ran and the pass rule failed.
    #[serde(rename = "NOT MET")]
    NotImplemented,
    /// A prerequisite (tool, ticket, PG item, input) is absent.
    #[serde(rename = "BLOCKED")]
    Blocked,
    /// Recorded, never gates.
    #[serde(rename = "INFO")]
    Info,
}

impl Status {
    /// The exact table spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Met => "MET",
            Self::NotImplemented => "NOT MET",
            Self::Blocked => "BLOCKED",
            Self::Info => "INFO",
        }
    }
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One evaluated check (or an expanded sub-check).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// Check id (`1.1`, `5.1.libc-hello`, ...).
    pub id: String,
    /// Row number, 1 through 6.
    pub row: u8,
    /// Human title.
    pub title: String,
    /// Observed status.
    pub status: Status,
    /// Process exit code, when a command ran.
    pub exit: Option<i32>,
    /// Why the status was chosen.
    pub reason: String,
    /// Evidence file, relative to the repository.
    pub evidence: String,
}

/// The six row names in table order.
pub const ROW_TITLES: [&str; 6] = [
    "1 correctness",
    "2 portability",
    "3 size",
    "4 hardening",
    "5 on-target",
    "6 documentation",
];

/// Aggregate checks into a row status.
///
/// A row is `MET` iff every required check is `MET`; `NOT MET` if any required check is `NOT MET`;
/// otherwise `BLOCKED`. `INFO` checks never gate. No required checks means `BLOCKED`.
#[must_use]
pub fn aggregate(required: impl IntoIterator<Item = Status>) -> Status {
    let mut any_required = false;
    let mut any_not_met = false;
    let mut all_met = true;
    for status in required {
        if status == Status::Info {
            continue;
        }
        any_required = true;
        match status {
            Status::Met => {}
            Status::NotImplemented => {
                any_not_met = true;
                all_met = false;
            }
            Status::Blocked => all_met = false,
            Status::Info => {}
        }
    }
    if !any_required {
        return Status::Blocked;
    }
    if any_not_met {
        Status::NotImplemented
    } else if all_met {
        Status::Met
    } else {
        Status::Blocked
    }
}

/// Aggregate the six rows into the gate verdict.
///
/// `MET` iff every row is `MET`; `NOT MET` if any row is `NOT MET`; otherwise `BLOCKED`.
#[must_use]
pub fn verdict(rows: [Status; 6]) -> Status {
    if rows.iter().all(|status| *status == Status::Met) {
        Status::Met
    } else if rows.contains(&Status::NotImplemented) {
        Status::NotImplemented
    } else {
        Status::Blocked
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
