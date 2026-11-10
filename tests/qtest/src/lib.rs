// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD QEMU test harness.
//!
//! Loads the case files defined by contract C6, runs the image under QEMU (or an arbitrary
//! `--runner` command for tests), captures serial output, matches regexes in order or
//! unordered, enforces a timeout, maps the guest exit protocol from C5, and writes a JSON
//! result plus the full serial log.
//!
//! The binary is `raven-qtest`; the library exposes the case parser and runner so `xtask` can
//! call it without duplicating the rules.

#![forbid(unsafe_code)]

mod case;
mod execute;
mod exitmap;
mod matcher;
mod oracle;
mod qemu;
mod report;
mod runner;

#[cfg(test)]
mod testutil;

pub use case::{Case, CaseSpec, Expect, ExpectExit, Forbid, is_valid_arch, list_case_files};
pub use execute::{ExecuteOptions, Executed, execute, execute_with_oracles};
pub use oracle::{
    OracleCheck, OracleReport, OracleSpec, Wav, analyze as analyze_oracle, parse_wav, write_reports,
};
pub use report::{CaseOutcome, CaseReport, ResultKind, write_outcome};
