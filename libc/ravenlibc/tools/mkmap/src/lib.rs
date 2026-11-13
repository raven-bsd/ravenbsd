// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-mkmap`: the host half of the symbol-version pipeline.
//!
//! * [`merge`] turns `abi/Versions.def` plus every `Symbol.map` into `build/<arch>/libc.map`
//!   and `abi/Symbols.list`;
//! * [`check`] enforces rules K1-K7 of `donor-map/14` 3.A.7 against a built `libc.a`;
//! * [`freeze`] records a released DSO node as `frozen/RAVEN_1.N.txt`.
//!
//! The crate is a host tool: `std` is allowed, `unsafe` is forbidden, and the two external
//! helpers (`nm`, `readelf`) are found through `RL_NM`/`RL_READELF` so a test can inject a
//! fixture reader. Files are read and written as UTF-8; a symbol name is never interpreted.

#![forbid(unsafe_code)]

pub mod check;
pub mod freeze;
pub mod list;
pub mod merge;
pub mod tools;
pub mod versions;

#[cfg(test)]
mod testutil;

/// One finding of a rule, rendered as `<rule>: <message>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The rule id (`K1`..`K7`).
    pub rule: &'static str,
    /// The human-readable message.
    pub message: String,
}

impl Finding {
    /// Build a finding.
    #[must_use]
    pub fn new(rule: &'static str, message: impl Into<String>) -> Self {
        Self {
            rule,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.rule, self.message)
    }
}
