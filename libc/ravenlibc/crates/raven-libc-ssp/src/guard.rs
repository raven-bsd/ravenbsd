// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! Canary storage and seeding.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The global guard storage; the shell exports it as `__stack_chk_guard`.
pub static GUARD: AtomicUsize = AtomicUsize::new(0);
/// Set when the canary came from the terminator fallback.
pub static SSP_WEAK: AtomicBool = AtomicBool::new(false);

/// Where the canary value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardSource {
    /// The supplied random bytes.
    Kernel,
    /// The terminator canary.
    Terminator,
}

/// A sink for the two copies of the canary (global and TLS).
pub trait CanarySink {
    /// Write the process-global copy.
    fn set_global(&self, value: usize);
    /// Write the thread-control-block copy (x86_64 `%fs:0x28`).
    fn set_tls(&self, value: usize);
}

/// Compute the guard from `random`; `None` or all-zero input uses the terminator fallback.
#[must_use]
pub fn compute_guard(random: Option<&[u8]>) -> (usize, GuardSource) {
    compute_guard_width(random, core::mem::size_of::<usize>())
}

/// Width-parameterised form, so 32-bit behaviour is testable on a 64-bit host.
#[must_use]
pub fn compute_guard_width(random: Option<&[u8]>, width: usize) -> (usize, GuardSource) {
    let terminator = terminator_value(width);
    let Some(random) = random else {
        return (terminator, GuardSource::Terminator);
    };
    if random.len() < width {
        return (terminator, GuardSource::Terminator);
    }
    let mut bytes = [0u8; core::mem::size_of::<usize>()];
    let limit = width.min(bytes.len());
    if let (Some(target), Some(source)) = (bytes.get_mut(..limit), random.get(..limit)) {
        target.copy_from_slice(source);
    }
    if width >= 8 {
        bytes[1] = 0;
    }
    let value = usize::from_le_bytes(bytes);
    if value == 0 {
        (terminator, GuardSource::Terminator)
    } else {
        (value, GuardSource::Kernel)
    }
}

/// Seed both canary copies; the terminator fallback marks [`SSP_WEAK`].
pub fn init_guard<S: CanarySink>(random: Option<&[u8]>, sink: &S) -> GuardSource {
    let (value, source) = compute_guard(random);
    GUARD.store(value, Ordering::SeqCst);
    sink.set_global(value);
    sink.set_tls(value);
    if source == GuardSource::Terminator {
        SSP_WEAK.store(true, Ordering::SeqCst);
    }
    source
}

fn terminator_value(width: usize) -> usize {
    let mut bytes = [0u8; core::mem::size_of::<usize>()];
    let limit = width.min(bytes.len());
    if limit > 0 {
        bytes[0] = 0;
    }
    if limit > 1 {
        bytes[1] = 0;
    }
    if limit > 2 {
        bytes[2] = b'\n';
    }
    if limit > 3 {
        bytes[3] = 0xff;
    }
    usize::from_le_bytes(bytes)
}

#[cfg(test)]
#[path = "guard_ut.rs"]
mod tests;
