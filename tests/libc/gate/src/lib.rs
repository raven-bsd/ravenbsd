// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD libc gate evaluator.
//!
//! The six-row gate of `tests/libc/GATE.md` is evaluated from `rows.toml`; every check writes an
//! evidence file under `build/libc-gate/`. The man-page candidate selection and coverage check are
//! independent of the verdict. Nothing here reads a donor or foreign manual text.

#![forbid(unsafe_code)]

pub mod eval;
pub mod man;
pub mod model;
pub mod report;
pub mod rows;
pub mod soak;
pub mod status;
pub mod testutil;

pub use model::{CheckResult, Status, aggregate, verdict};
