// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host test for the aarch64 `sendsig` frame layout (C19/S160, NetBSD donor).
//!
//! The test compiles the kernel's `kernel/src/arch/aarch64/signal.rs` directly, so the pinned
//! offsets and sigcode are the same code the aarch64 vectors will call.

#[path = "../../../../../kernel/src/arch/aarch64/signal.rs"]
mod signal;

pub use signal::*;
