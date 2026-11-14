// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable bodies, replay and regression harness for the ravenlibc fuzz targets.
//!
//! The bodies are pure functions of their input bytes: no clock, no environment and no
//! randomness beyond a value decoded from the input. The `cargo-fuzz` targets in
//! `tests/libc/fuzz/targets` are one-line wrappers around [`run`]; every corpus and regression
//! file is also an ordinary stable `cargo test` case.

#![deny(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// The harness indexes fixed-size buffers whose lengths are checked by construction; the
// arithmetic is the C size arithmetic of the decoders under test. A panic is the finding
// signal, so `clippy::panic` is not denied here.
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic
)]

#[allow(unsafe_code)] // the ticket's single counting-allocator module
pub mod alloc_probe;
pub mod cursor;
pub mod env;
pub mod harness;
pub mod model;
pub mod rng;
pub mod targets;

use std::fmt;

pub use cursor::Cursor;

/// A fuzz target body: a total function of arbitrary bytes.
pub type Body = fn(&[u8]);

/// The largest input libFuzzer is asked to generate (`-max_len=4096`).
pub const MAX_INPUT: usize = 4096;
/// Per-input watchdog limit.
pub const TIMEOUT_SECS: u64 = 10;
/// Peak-allocation cap per input, except for `regex_compile_exec`.
pub const PEAK_CAP: usize = 64 << 20;
/// Peak-allocation cap of `regex_compile_exec` (the C45 `exhaust_seven_shapes` figure).
pub const REGEX_PEAK_CAP: usize = 256 << 20;

/// Every implemented target, sorted by name.
///
/// `iconv_stream` is DEFERRED: C48 (`raven-libc-iconv`) is not done.
pub const TARGETS: &[(&str, Body)] = &[
    ("elf_dynamic", targets::elf_dynamic::body),
    ("getopt_args", targets::getopt_args::body),
    ("glob_pattern", targets::glob_pattern::body),
    ("inet_text", targets::inet_text::body),
    ("printf_format", targets::printf_format::body),
    ("regex_compile_exec", targets::regex_compile_exec::body),
    ("resolver_conf", targets::resolver_conf::body),
    ("resolver_message", targets::resolver_message::body),
    ("scanf_format", targets::scanf_format::body),
    ("strtod_input", targets::strtod_input::body),
    ("strtol_input", targets::strtol_input::body),
    ("time_format", targets::time_format::body),
    ("tz_parse", targets::tz_parse::body),
    ("utf8_decode", targets::utf8_decode::body),
];

/// Dispatch one body. An unknown name is a harness bug, so it panics with that name.
pub fn run(target: &str, data: &[u8]) {
    let limited = data.get(..MAX_INPUT).unwrap_or(data);
    for (name, body) in TARGETS {
        if *name == target {
            body(limited);
            return;
        }
    }
    panic!("unknown fuzz target {target}");
}

/// The peak-allocation cap of `target`.
#[must_use]
pub fn peak_cap(target: &str) -> usize {
    if target.as_bytes() == b"regex_compile_exec" {
        REGEX_PEAK_CAP
    } else {
        PEAK_CAP
    }
}

/// Identify a body by name without running it; `None` for an unknown target.
#[must_use]
pub fn body_of(target: &str) -> Option<Body> {
    TARGETS
        .iter()
        .find(|(name, _)| *name == target)
        .map(|(_, body)| *body)
}

/// A replay failure.
#[derive(Debug)]
pub enum ReplayError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The body panicked; the message is the panic payload rendered as text.
    Panicked {
        /// The target name.
        target: &'static str,
        /// The panic message.
        message: String,
    },
    /// The body exceeded [`TIMEOUT_SECS`].
    Timeout {
        /// The target name.
        target: &'static str,
        /// The limit that fired.
        secs: u64,
    },
    /// The body exceeded the target's peak-allocation cap.
    MemoryCap {
        /// The target name.
        target: &'static str,
        /// Observed peak since the body started.
        peak: usize,
        /// The cap.
        cap: usize,
    },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io: {error}"),
            Self::Panicked { target, message } => write!(f, "{target}: panic: {message}"),
            Self::Timeout { target, secs } => write!(f, "{target}: timeout after {secs}s"),
            Self::MemoryCap { target, peak, cap } => {
                write!(f, "{target}: peak {peak} exceeds cap {cap}")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// Read and replay one file through the named target under the watchdog and memory cap.
pub fn replay_file(target: &str, path: &std::path::Path) -> Result<(), ReplayError> {
    let data = std::fs::read(path).map_err(ReplayError::Io)?;
    let (name, body) = TARGETS
        .iter()
        .find(|(name, _)| *name == target)
        .copied()
        .unwrap_or_else(|| panic!("unknown fuzz target {target}"));
    harness::execute(name, body, &data, peak_cap(name))
}
