// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Normative-value coverage for the libc ABI constant groups.
//!
//! The tests read `libc/ravenlibc/abi/{misc,poll,stat,time}.toml`, compare every key to an
//! explicit expected value transcribed from musl/POSIX, check the value exported by
//! `raven-libc-abi`, and check the generated C header. A key without an expected row fails.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
