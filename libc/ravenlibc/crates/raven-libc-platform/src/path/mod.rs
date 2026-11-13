// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX paths on top of schemes.
//!
//! Resolution happens once, lexically, inside libc (doc 11 section 3.4). The result is an
//! absolute path with no `.`, `..`, or repeated `/`, and it is then prefixed with `file:` by the
//! caller. No POSIX function parses a scheme: `open("foo:bar")` is the file named `foo:bar` in
//! the cwd.
//!
//! Lexical `..` is a documented deviation until PG-33: after a symbolic link, POSIX resolves
//! `..` from the link target's directory, while this layer removes the previous lexical
//! component. The server owns physical resolution after PG-33 and this module's contract does
//! not change.

use raven_abi::errno::Errno;

pub mod cwd;
pub mod resolve;
pub mod special;

pub use cwd::Cwd;
pub use resolve::{DEFAULT_LIMITS, Limits, Resolved, resolve};
pub use special::{SpecialName, special_fd};

/// Maximum POSIX path length, including the leading slash.
pub const PATH_MAX: usize = 4096;
/// Maximum single component length.
pub const NAME_MAX: usize = 255;
/// Maximum symlink traversals before `ELOOP` (server-side after PG-33).
pub const SYMLOOP_MAX: u32 = 40;

/// Why a path could not be resolved; the mapping keeps the errno in one place.
pub fn path_errno(reason: resolve::PathReason) -> Errno {
    match reason {
        resolve::PathReason::Empty => Errno::ENOENT,
        resolve::PathReason::TooLong => Errno::ENAMETOOLONG,
        resolve::PathReason::ComponentTooLong => Errno::ENAMETOOLONG,
        resolve::PathReason::Nul => Errno::EINVAL,
        resolve::PathReason::NotAbsolute => Errno::EINVAL,
    }
}

#[cfg(test)]
#[path = "resolve_ut.rs"]
mod resolve_tests;

#[cfg(test)]
#[path = "special_ut.rs"]
mod special_tests;

#[cfg(test)]
#[path = "cwd_ut.rs"]
mod cwd_tests;
