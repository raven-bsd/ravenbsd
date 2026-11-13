// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `secure_getenv`, `clearenv` and the environment list.
//!
//! CLEANROOM: `secure_getenv` is the donor-free two-line rule
//! `if at_secure { None } else { getenv(n) }`; `AT_SECURE` is read from the auxv by the shell
//! into the `at_secure` parameter. `clearenv` forgets the array but never frees the strings.

pub mod getenv;
pub mod putenv;
pub mod secure;
pub mod setenv;
pub mod table;
pub mod unsetenv;

pub use getenv::{getenv, getenv_r};
pub use putenv::putenv;
pub use secure::secure_getenv;
pub use setenv::setenv;
pub use table::EnvTable;
pub use unsetenv::unsetenv;

/// `clearenv`: drop every entry; the strings themselves are not freed (D-2).
pub fn clearenv(env: &mut EnvTable) {
    env.clearenv();
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
