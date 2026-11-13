// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `ENTROPY_WEAK` release gate (H-5, RD-56).

use core::sync::atomic::{AtomicBool, Ordering};

/// True when this build links the interim weak source.
pub const ENTROPY_WEAK_BUILD: bool = cfg!(feature = "weak-entropy-interim");

static RUNTIME_WEAK: AtomicBool = AtomicBool::new(false);

/// Mark the runtime entropy source weak (called when the platform used the interim source).
pub fn mark_entropy_weak() {
    RUNTIME_WEAK.store(true, Ordering::SeqCst);
}

/// Build flag OR runtime flag. A release image must see `false`.
#[must_use]
pub fn entropy_is_weak() -> bool {
    ENTROPY_WEAK_BUILD || RUNTIME_WEAK.load(Ordering::SeqCst)
}

#[cfg(test)]
#[path = "weak_ut.rs"]
mod tests;
