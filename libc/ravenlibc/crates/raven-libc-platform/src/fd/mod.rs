// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-local fd table and open-file descriptions (doc 11 section 3.5).
//!
//! An fd is an index into [`table::FdTable`]; it points at an [`ofd::Ofd`] slot owned by
//! [`ofd::OfdArena`]. The OFD owns exactly one kernel handle, so `dup` and `fork` share the
//! handle (and therefore the server-side offset) without another kernel call, and the handle is
//! closed only when the last reference goes away.
//!
//! The table is not the fdsan tag array: the tag lives in the [`table::FdEntry`] and the
//! [`fdsan`] module owns its encoding and error policy.

pub mod fdsan;
pub mod inherit;
pub mod ofd;
pub mod table;

pub use fdsan::{ErrorLevel, FdsanState, OwnerType, tag, tag_type, tag_value};
pub use inherit::{FDMAP_MAGIC, FDMAP_VERSION, FdMap, FdMapEntry};
pub use ofd::{Ofd, OfdArena, OfdKind, OfdRef};
pub use table::{FD_CLOEXEC, FdEntry, FdTable};

/// The default soft `RLIMIT_NOFILE`.
pub const DEFAULT_SOFT_NOFILE: u32 = 1024;
/// The default hard `RLIMIT_NOFILE`.
pub const DEFAULT_HARD_NOFILE: u32 = 32768;
