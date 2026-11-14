// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `*64` alias seed (`14` 3.A.10, `01` A25, ticket C66 5.F).
//!
//! On Raven `off_t`, `time_t` and `ino_t` are 64-bit on every architecture (DR-13), so an alias
//! is the same code as its base and the shell emits one symbol alias, not a second body. The
//! census (`abi/census/symbols.toml`, `aliases` and names ending in `64`) is the authority for
//! later additions; a base that does not exist yet is listed in [`PENDING`] and is not exported
//! until its owner lands it.

/// The seed rows `(alias, base)` of ticket C66 5.F, in table order.
pub const ALIASES: &[(&str, &str)] = &[
    ("stat64", "stat"),
    ("fstat64", "fstat"),
    ("lstat64", "lstat"),
    ("fstatat64", "fstatat"),
    ("statfs64", "statfs"),
    ("fstatfs64", "fstatfs"),
    ("open64", "open"),
    ("openat64", "openat"),
    ("creat64", "creat"),
    ("truncate64", "truncate"),
    ("ftruncate64", "ftruncate"),
    ("pread64", "pread"),
    ("pwrite64", "pwrite"),
    ("lseek64", "lseek"),
    ("mmap64", "mmap"),
    ("sendfile64", "sendfile"),
    ("lockf64", "lockf"),
    ("posix_fadvise64", "posix_fadvise"),
    ("posix_fallocate64", "posix_fallocate"),
    ("fopen64", "fopen"),
    ("fseeko64", "fseeko"),
    ("readdir64", "readdir"),
    ("getdents64", "getdents"),
];

/// Alias rows whose base symbol does not exist on the C66 branch; they are not exported.
///
/// `fstatat`/`truncate`/`ftruncate`/`sendfile` are owned by C55/C64, `statfs`/`fstatfs` by
/// C55, `lockf`/`posix_fadvise`/`posix_fallocate` by C55/C41, `fopen`/`fseeko` by C56/C64 and
/// `getdents` by C55.
pub const EXISTING: &[&str] = &["open64", "openat64", "creat64"];

/// Alias rows whose base symbol does not exist on the C66 branch; they are not exported.
pub const PENDING: &[&str] = &[
    "fstatat64",
    "statfs64",
    "fstatfs64",
    "truncate64",
    "ftruncate64",
    "lockf64",
    "posix_fadvise64",
    "posix_fallocate64",
    "fopen64",
    "fseeko64",
    "getdents64",
];

/// Whether `alias` is exported on this branch.
#[must_use]
pub fn active(alias: &str) -> bool {
    ALIASES.iter().any(|(name, _)| *name == alias)
        && !PENDING.contains(&alias)
        && !EXISTING.contains(&alias)
}

/// The base of `alias`.
#[must_use]
pub fn base(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(name, _)| *name == alias)
        .map(|(_, base)| *base)
}

#[cfg(test)]
#[path = "alias64_ut.rs"]
mod tests;
