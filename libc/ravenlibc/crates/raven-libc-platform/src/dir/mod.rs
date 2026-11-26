// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory helpers that are pure libc (the `opendir` family is C46).

pub mod getcwd;

pub use getcwd::getcwd;

/// Offset of `d_name` in the public FreeBSD `struct dirent`.
pub const DIRENT_NAME_OFFSET: usize = 24;
/// Length of the shortest public record (a one-byte name).
pub const DIRENT_MIN_LEN: usize = 32;
/// Length of the longest public record (a 255-byte name).
pub const DIRENT_MAX_LEN: usize = 280;

/// The FreeBSD `_GENERIC_DIRLEN`: header, name, NUL, rounded up to eight bytes.
pub const fn dirent_reclen(namlen: usize) -> usize {
    (DIRENT_NAME_OFFSET + namlen + 1 + 7) & !7
}

#[cfg(test)]
#[path = "dir_ut.rs"]
mod tests;
