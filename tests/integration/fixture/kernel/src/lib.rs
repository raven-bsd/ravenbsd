// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel fixture: depends on the core library and includes the mirror's implementation.

#[path = "../../crates/mirror-lib/src/imp.rs"]
pub mod imported;

use fixture_core::add;

/// Entry point stand-in.
pub fn run() -> u32 {
    add(1, imported::double(2))
}
