// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Integer arithmetic: `abs`/`labs` (and their `llabs`/`imaxabs` aliases) and the `div` family.
//! The donors are OpenBSD `lib/libc/stdlib/{abs,labs,div,ldiv}.c`; `llabs`/`imaxabs`/`lldiv`/
//! `imaxdiv` are aliases of the same bodies on LP64 (R-10: one core, thin wrappers).

pub mod abs;
pub mod div;
pub mod labs;
pub mod ldiv;

pub use abs::abs;
pub use div::{DivT, div};
pub use labs::labs;
pub use ldiv::{LDivT, ldiv};

/// `llabs`: `long long` is `long` on every Raven target, so it is the `labs` body.
#[must_use]
pub const fn llabs(x: i64) -> i64 {
    labs(x)
}

/// `imaxabs`: `intmax_t` is `long` on every Raven target, so it is the `labs` body.
#[must_use]
pub const fn imaxabs(x: i64) -> i64 {
    labs(x)
}

/// `lldiv`: `long long` is `long` on every Raven target, so it is the `ldiv` body.
#[must_use]
pub fn lldiv(n: i64, d: i64) -> LDivT {
    ldiv(n, d)
}

/// `imaxdiv`: `intmax_t` is `long` on every Raven target, so it is the `ldiv` body.
#[must_use]
pub fn imaxdiv(n: i64, d: i64) -> LDivT {
    ldiv(n, d)
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
