// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fts`, `ftw`, `nftw` and the `DIR` iteration helpers for ravenlibc
//! (`architecture/raven-libc/donor-map/07` sections 3.5-3.6).
//!
//! The crate is `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`), has no global state and
//! no clock or randomness. Everything reaches the filesystem through [`DirSys`], a descriptor
//! based seam shaped like `openat`/`fstatat`/`fstat`/`getdents`/`lseek`/`close`; the platform
//! crate (C55) implements it and the C-ABI shell owns `DIR`, `FTS`, `FTSENT` and `struct dirent`.
//!
//! Donor files, all COPIED with their notices: `musl:src/dirent/*` (MIT via `musl/COPYRIGHT`)
//! for the stream and `scandir`, `freebsd:lib/libc/gen/fts.c` (BSD-3-Clause), and
//! `freebsd:lib/libc/gen/{ftw,nftw}.c` (ISC). Hardening on top of the donors: H-FTS-1
//! (`FTS_NOCHDIR` is forced), H-FTS-2 (`O_NOFOLLOW` on physical walks), H-FTS-3 (dev/ino
//! identity re-check), H-FTS-4 (fd budget with a full-path fallback), H-FTS-5 (`PATH_MAX` cap)
//! and the malformed-`getdents`-record `EIO` rule. [`dirent::parse_dirent`] never trusts
//! `d_reclen`.
//!
//! The `testutil` feature exports the std-free fake directory tree of doc 07 4.5 (`fake`).

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
pub mod dirent;
#[cfg(any(test, feature = "testutil"))]
pub mod fake;
pub mod fts;
pub mod ftw;
pub mod nftw;
pub mod scandir;
mod sort;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::structs::Stat;

pub use consts::*;
pub use dirent::{DirEntry, DirStream, parse_dirent};
pub use fts::{Compar, Fts, FtsEnt, NodeId, fts_open};
pub use ftw::{FTW_D, FTW_DNR, FTW_DP, FTW_F, FTW_NS, FTW_SL, FTW_SLN, Ftw, ftw};
pub use nftw::{FTW_CHDIR, FTW_DEPTH, FTW_MOUNT, FTW_PHYS, nftw};
pub use scandir::{alphasort, versionsort};

/// `AT_FDCWD` (Linux numbering, RD-69).
pub const AT_FDCWD: i32 = -100;
/// A file descriptor, owned by the caller or the stream that opened it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fd(pub i32);

/// `lseek` whence values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whence {
    /// `SEEK_SET`.
    Set,
    /// `SEEK_CUR`.
    Cur,
    /// `SEEK_END`.
    End,
}

/// The filesystem seam: six descriptor operations, all owned by the platform crate.
///
/// This is the descriptor-based trait of doc 07 4.2; `raven-libc-glob` has a different, path-based
/// `DirSys` (the `glob_t` hooks) and the two are deliberately separate (`RD-new`).
pub trait DirSys {
    /// `openat(dirfd, name, flags)`; the caller closes `Fd`.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn openat(&mut self, dirfd: i32, name: &[u8], flags: u32) -> Result<Fd, Errno>;
    /// `fstatat(dirfd, name, AT_SYMLINK_NOFOLLOW)` when `nofollow` is set.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn fstatat(&mut self, dirfd: i32, name: &[u8], nofollow: bool) -> Result<Stat, Errno>;
    /// `fstat(fd)`.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn fstat(&mut self, fd: Fd) -> Result<Stat, Errno>;
    /// `getdents(fd, buf)`: `0` is end of stream, records are Linux `dirent64` (doc 07 4.3).
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn getdents(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, Errno>;
    /// `lseek(fd, off, whence)`.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn lseek(&mut self, fd: Fd, off: i64, whence: Whence) -> Result<i64, Errno>;
    /// `close(fd)`.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn close(&mut self, fd: Fd) -> Result<(), Errno>;

    /// `fcntl(fd, F_GETFL)` for the `O_PATH` rejection of [`dirent::DirStream::fdopendir`].
    ///
    /// The six required methods above are the contract; this provided method is an additive
    /// extension point (`RD-new: fdopendir O_PATH check`). The platform crate overrides it; the
    /// default reports "no O_PATH" so a minimal backend still works.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value.
    fn fcntl_getfl(&mut self, _fd: Fd) -> Result<i32, Errno> {
        Ok(0)
    }
}

/// How a `getdents` cookie failed the OI-2 contract.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookieViolation {
    /// The directory could not be opened.
    Open(Errno),
    /// A `getdents` read failed.
    Read(Errno),
    /// `lseek` did not accept a `d_off` value.
    Seek {
        /// The entry index whose cookie failed.
        index: usize,
        /// The rejected cookie.
        off: i64,
    },
    /// Resuming at a cookie did not return the following entry.
    Sequence {
        /// The entry index whose cookie failed.
        index: usize,
        /// The cookie.
        off: i64,
    },
}

/// The number of entries the contract check samples.
#[doc(hidden)]
pub const COOKIE_CHECK_MAX: usize = 64;

/// Verify OI-2 on a real backend: every `d_off` is an `lseek` cookie that resumes after its entry.
///
/// The platform ticket (C55/PG-30) runs this against the servers; the fake satisfies it by
/// construction. Only the first [`COOKIE_CHECK_MAX`] entries are sampled.
///
/// # Errors
///
/// The [`CookieViolation`] of the first failing cookie.
#[doc(hidden)]
pub fn check_cookie_contract<S: DirSys>(sys: &mut S, dir: &[u8]) -> Result<(), CookieViolation> {
    let mut stream = DirStream::opendir(sys, dir).map_err(CookieViolation::Open)?;
    let mut entries = alloc::vec::Vec::new();
    loop {
        match stream.readdir(sys) {
            Ok(Some(entry)) => {
                if entries.len() >= COOKIE_CHECK_MAX {
                    break;
                }
                entries.push(entry);
            }
            Ok(None) => break,
            Err(error) => {
                let _ = stream.closedir(sys);
                return Err(CookieViolation::Read(error));
            }
        }
    }
    let _ = stream.closedir(sys);

    for (index, entry) in entries.iter().enumerate() {
        let mut probe = DirStream::opendir(sys, dir).map_err(CookieViolation::Open)?;
        probe.seekdir(sys, entry.off);
        if probe.telldir() != entry.off {
            let _ = probe.closedir(sys);
            return Err(CookieViolation::Seek {
                index,
                off: entry.off,
            });
        }
        let Some(expected) = entries.get(index.wrapping_add(1)) else {
            let _ = probe.closedir(sys);
            continue;
        };
        match probe.readdir(sys) {
            Ok(Some(next)) => {
                if next.name != expected.name {
                    let _ = probe.closedir(sys);
                    return Err(CookieViolation::Sequence {
                        index,
                        off: entry.off,
                    });
                }
            }
            Ok(None) | Err(_) => {
                let _ = probe.closedir(sys);
                return Err(CookieViolation::Sequence {
                    index,
                    off: entry.off,
                });
            }
        }
        let _ = probe.closedir(sys);
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
