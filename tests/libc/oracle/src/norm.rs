// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host normalisation.
//!
//! The worked functions of this ticket do not set `errno`, but later families do. `with_errno`
//! clears `errno`, runs the closure and returns the value and the resulting `errno`. It is
//! Linux-only in effect (glibc and musl both export `__errno_location`); on another host the errno
//! slot is reported as 0 and the differential tests are unsupported.

/// Run `f` with `errno` cleared and return `(value, errno)`.
pub fn with_errno<R>(f: impl FnOnce() -> R) -> (R, i32) {
    crate::call::errno_set(0);
    let value = f();
    (value, crate::call::errno_get())
}

#[cfg(test)]
#[path = "norm_ut.rs"]
mod tests;
