// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Status helpers shared by the evaluator and the report.

pub use crate::model::{Status, aggregate, verdict};

/// Classify a process exit with the row's pass and blocked rules.
#[must_use]
pub fn from_exit(exit: Option<i32>, pass: i32, blocked: &[i32]) -> Status {
    match exit {
        None => Status::Blocked,
        Some(code) if blocked.contains(&code) => Status::Blocked,
        Some(code) if code == pass => Status::Met,
        Some(_) => Status::NotImplemented,
    }
}

#[cfg(test)]
#[path = "status_ut.rs"]
mod tests;
