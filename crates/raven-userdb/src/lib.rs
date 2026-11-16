// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The files-backed RavenBSD user database.
//!
//! `/etc/passwd`, `/etc/group` and `/etc/shadow` are the authoritative 0.2.0 account database. This
//! crate parses, validates, renders and looks up those files and provides the locked temporary-file
//! transaction the administration tools use. It deliberately never hashes or verifies a password:
//! hashes come from `raven-auth` and the tools pass the finished string in.
//!
//! The crate is `no_std` plus `alloc` on the target and uses the host `std` only in tests. The
//! target-only [`rt`] module adapts the `raven-rt` file API to the [`store::DbIo`] seam.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod db;
pub mod error;
pub mod group;
pub mod passwd;
pub mod shadow;
pub mod store;
mod validate;

#[cfg(target_os = "none")]
pub mod rt;

pub use crate::db::{AddGroup, AddUser, AddUserOutcome, Credential, Database};
pub use crate::error::{DbError, DbFile};
pub use crate::group::GroupEntry;
pub use crate::passwd::PasswdEntry;
pub use crate::shadow::ShadowEntry;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
