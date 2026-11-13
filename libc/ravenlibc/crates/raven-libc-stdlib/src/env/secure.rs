// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `secure_getenv`.
//!
//! CLEANROOM: no donor code; the rule is `if at_secure { None } else { getenv(n) }`. The shell
//! reads `AT_SECURE` from the auxiliary vector and passes it in.

use super::table::EnvTable;

/// `secure_getenv(name, at_secure)`.
#[must_use]
pub fn secure_getenv(env: &EnvTable, name: &[u8], at_secure: bool) -> Option<&'static [u8]> {
    if at_secure { None } else { env.getenv(name) }
}

#[cfg(test)]
#[path = "secure_ut.rs"]
mod tests;
