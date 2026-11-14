// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The output mode.

/// The output mode selected by the global flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Human output; colour and column alignment are allowed.
    Human,
    /// NDJSON, one object per line, no colour.
    Json,
    /// No normal output; errors still go to stderr.
    Quiet,
}
