// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `MALLOC_DEBUG` comma-list parser, without allocation.

#![allow(clippy::indexing_slicing)] // Token positions are checked against the slice length.

use crate::debug::consts::{DEFAULT_AUDIT, MAX_AUDIT};

/// The debug-mode options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugOptions {
    /// Check 16-byte redzones.
    pub redzone: bool,
    /// Fill allocations and frees.
    pub fill: bool,
    /// Quarantine length.
    pub audit: usize,
    /// Guard threshold in bytes (`0` = off).
    pub guard: usize,
    /// Keep live buffers for the leak report.
    pub leak: bool,
    /// Fail every n-th allocation (`0` = never).
    pub fail_every: usize,
}

impl Default for DebugOptions {
    fn default() -> Self {
        Self {
            redzone: false,
            fill: false,
            audit: DEFAULT_AUDIT,
            guard: 0,
            leak: false,
            fail_every: 0,
        }
    }
}

/// Parse a comma-separated list (`redzone,fill,audit=128,guard=4096,leak,fail_every=3`).
#[must_use]
pub fn parse(text: &[u8]) -> DebugOptions {
    let mut options = DebugOptions::default();
    let mut start = 0usize;
    let mut index = 0usize;
    while index <= text.len() {
        if index == text.len() || text[index] == b',' {
            parse_token(&mut options, &text[start..index]);
            start = index.wrapping_add(1);
        }
        index = index.wrapping_add(1);
    }
    options
}

fn parse_token(options: &mut DebugOptions, token: &[u8]) {
    let (name, value) = split_equal(token);
    if name == b"redzone" {
        options.redzone = true;
    } else if name == b"fill" {
        options.fill = true;
    } else if name == b"leak" {
        options.leak = true;
    } else if name == b"audit" {
        options.audit = number(value).clamp(1, MAX_AUDIT);
    } else if name == b"guard" {
        options.guard = number(value);
    } else if name == b"fail_every" {
        options.fail_every = number(value);
    }
}

fn split_equal(token: &[u8]) -> (&[u8], &[u8]) {
    let mut index = 0usize;
    while index < token.len() {
        if token[index] == b'=' {
            return (&token[..index], &token[index.wrapping_add(1)..]);
        }
        index = index.wrapping_add(1);
    }
    (token, &[])
}

fn number(text: &[u8]) -> usize {
    let mut value = 0usize;
    let mut index = 0usize;
    while index < text.len() {
        let digit = text[index];
        if !digit.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'));
        index = index.wrapping_add(1);
    }
    value
}

#[cfg(test)]
#[path = "env_ut.rs"]
mod tests;
