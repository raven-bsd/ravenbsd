// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-test surface for the `raven-login` acceptance package.
//!
//! The account policy lives in the `login` crate used by `/bin/login` and `/bin/getty`. This
//! package names the login side of the acceptance gate and tests the retry status getty loops on,
//! without adding a second implementation.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub use login::{LOGIN_OK, LOGIN_RETRY, Profile, is_root_user, login_status, normalize_username};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
