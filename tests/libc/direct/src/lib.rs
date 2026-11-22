// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Direct SSH-critical libc tests for the donor port register.

/// The K519 source-inclusion test of the unreferenced `raw/strcoll.rs` calls
/// `crate::cstr::strlcpy`; expose the live implementation under that path.
#[cfg(test)]
pub(crate) mod cstr {
    pub use raven_libc_str::cstr::strlcpy;
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
