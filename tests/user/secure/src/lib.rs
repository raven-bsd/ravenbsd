// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side adversarial credential-transition tests for `su` and `passwd`.
//!
//! The crate carries no production code. The tests attack the authentication decisions that
//! precede a credential transition, the environment and group plan, and the locked update
//! transaction. The real setuid transition and the target shell are exercised by the
//! `user-secure` qtest.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
