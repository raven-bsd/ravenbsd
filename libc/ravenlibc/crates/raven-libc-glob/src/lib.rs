// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fnmatch`, `glob` and `wordexp` for ravenlibc (`architecture/raven-libc/donor-map/07`
//! sections 3.2-3.4).
//!
//! The crate is `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`), has no global state and
//! no clock or randomness. `fnmatch` is the musl "Sea of Stars" port with the FreeBSD flag
//! values; `glob` is the FreeBSD `glob.c` port with byte-order sorting and the `GLOB_LIMIT`
//! arithmetic of the donor plus H-GL-1; `wordexp` is the musl pre-scan plus the `sh -c` script.
//! Filesystem, environment and process access go through the [`DirSys`], [`GlobEnv`] and
//! [`ShellRun`] seams; [`PatternLocale`] is the multibyte seam over `raven-libc-ctype`. The
//! C-ABI shell owns `glob_t`, `wordexp_t` and every exported symbol.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod consts;
pub mod fnmatch;
pub mod glob;
pub mod locale;
pub mod wordexp;

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::structs::Stat;

pub use fnmatch::fnmatch;
pub use glob::{GlobState, glob, globfree};
pub use locale::{ByteLocale, MbResult, PatternLocale, Utf8Locale, Wctype};
pub use wordexp::{ShellRun, WordExp, wordexp};

/// A directory handle of the path-based glob seam.
///
/// The handle is opaque to this crate: it is the `void *` the `GLOB_ALTDIRFUNC` hooks return
/// (`gl_opendir`), and the shell/platform backend decides what it points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirHandle(pub usize);

/// Filesystem access for `glob`, shaped like the `glob_t` `ALTDIRFUNC` hooks
/// (`gl_opendir`, `gl_readdir`, `gl_closedir`, `gl_lstat`, `gl_stat`).
///
/// The shell wraps either the five C function pointers or the platform backend. This is a
/// different trait from `raven-libc-fts::DirSys` on purpose (descriptor based there, path based
/// here): both are documented in the crate READMEs (`RD-new: two DirSys traits`).
pub trait DirSys {
    /// `gl_opendir`: open a directory for reading.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value (`ENOENT`, `ENOTDIR`, `EACCES`, ...).
    fn opendir(&mut self, path: &[u8]) -> Result<DirHandle, Errno>;
    /// `gl_readdir`: the next entry name (`None` at end of stream).
    ///
    /// # Errors
    ///
    /// The backend's `errno` value for a read failure.
    fn readdir(&mut self, d: &mut DirHandle) -> Result<Option<Vec<u8>>, Errno>;
    /// `gl_closedir`: release a handle.
    fn closedir(&mut self, d: DirHandle);
    /// `gl_lstat`: stat without following a final symbolic link.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn lstat(&mut self, path: &[u8]) -> Result<Stat, Errno>;
    /// `gl_stat`: stat following symbolic links.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno>;
}

/// Environment and user-database access for `GLOB_TILDE`.
///
/// `GLOB_TILDE` must never call plain `getenv` (trap list): the method is named
/// `secure_getenv_home` so the shell passes the `secure_getenv("HOME")` result and this crate
/// cannot reach the environment directly.
pub trait GlobEnv {
    /// `secure_getenv("HOME")`.
    fn secure_getenv_home(&self) -> Option<Vec<u8>>;
    /// `getlogin()`.
    fn getlogin(&self) -> Option<Vec<u8>>;
    /// `getpwnam(name)->pw_dir`.
    fn getpwnam_dir(&self, name: &[u8]) -> Option<Vec<u8>>;
    /// `getpwuid(getuid())->pw_dir`.
    fn getpwuid_self_dir(&self) -> Option<Vec<u8>>;
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
