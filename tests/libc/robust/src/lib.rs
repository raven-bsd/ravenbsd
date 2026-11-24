// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host adversarial-argument tests for the ravenlibc login/server path.
//!
//! The C-ABI exports are target-only because their names collide with the host libc (see
//! `libc/ravenlibc/crates/raven-libc/src/lib.rs`). The host tests therefore drive the same
//! platform and pure-logic functions each shell wrapper forwards to, with the deterministic
//! `FakeSys` kernel. The wrapper-to-function map and the expected errno are in
//! `docs/libc-robustness.md`.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
