// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The two seams of the crate: [`SysFile`] (every system-call-shaped operation) and [`Lock`]
//! (every blocking wait), plus the combined [`Env`] bound and the small `fstat` view.
//!
//! `state.rs`, `ofstream` and every backend take these traits; `raven-libc-platform` (C55)
//! implements `SysFile`, `raven-libc-thread` (C57) implements `Lock`, and the shell wires the
//! two into one [`Env`]. The crate has no other dependency on the platform.

use core::sync::atomic::AtomicI32;

use raven_libc_abi::errno::Errno;

/// The few `struct stat` fields the FILE layer needs (`donor-map/04` 3.5 rule 3).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct StatLite {
    /// `st_mode` bits; only the `S_IFMT` field is inspected.
    pub mode: u32,
    /// `st_blksize`, used as the buffer size hint.
    pub blksize: u32,
    /// True when `S_ISCHR(mode)` (a terminal can only be a character device).
    pub is_char_dev: bool,
}

/// The OS seam.
///
/// All calls take the raw POSIX fd; `errno` is the return channel. `write2` is one gathering
/// write of two buffers (musl `__stdio_write`); a short result is valid and the caller loops.
pub trait SysFile {
    /// `read(2)`; `Ok(0)` is end of file and a short count is valid.
    ///
    /// # Errors
    /// The platform errno.
    fn read(&self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno>;

    /// `write(2)`; a short count is valid.
    ///
    /// # Errors
    /// The platform errno.
    fn write(&self, fd: i32, buf: &[u8]) -> Result<usize, Errno>;

    /// One `writev(2)` of two iovecs, returning the total written.
    ///
    /// # Errors
    /// The platform errno.
    fn write2(&self, fd: i32, a: &[u8], b: &[u8]) -> Result<usize, Errno>;

    /// `lseek(2)`; returns the new absolute offset.
    ///
    /// # Errors
    /// The platform errno (`ESPIPE` for a pipe).
    fn seek(&self, fd: i32, off: i64, whence: i32) -> Result<i64, Errno>;

    /// `close(2)`.
    ///
    /// # Errors
    /// The platform errno.
    fn close(&self, fd: i32) -> Result<(), Errno>;

    /// `open(2)`; `path` has no NUL terminator.
    ///
    /// # Errors
    /// The platform errno.
    fn open(&self, path: &[u8], flags: i32, mode: u32) -> Result<i32, Errno>;

    /// `ioctl(fd, TCGETS) == 0`: the fd is a terminal.
    ///
    /// This must not allocate and must be safe on the very first write of the process.
    fn isatty(&self, fd: i32) -> bool;

    /// `fstat(2)`, reduced to [`StatLite`].
    ///
    /// # Errors
    /// The platform errno.
    fn fstat(&self, fd: i32) -> Result<StatLite, Errno>;

    /// `fcntl(fd, F_GETFL)`.
    ///
    /// # Errors
    /// The platform errno.
    fn getfl(&self, fd: i32) -> Result<i32, Errno>;

    /// `fcntl(fd, F_SETFL, flags)`.
    ///
    /// # Errors
    /// The platform errno.
    fn setfl(&self, fd: i32, flags: i32) -> Result<(), Errno>;

    /// `unlink(2)`; `path` has no NUL terminator.
    ///
    /// # Errors
    /// The platform errno.
    fn unlink(&self, path: &[u8]) -> Result<(), Errno>;

    /// `rmdir(2)`; `path` has no NUL terminator.
    ///
    /// # Errors
    /// The platform errno.
    fn rmdir(&self, path: &[u8]) -> Result<(), Errno>;

    /// `rename(2)`; both paths have no NUL terminator.
    ///
    /// # Errors
    /// The platform errno.
    fn rename(&self, old: &[u8], new: &[u8]) -> Result<(), Errno>;

    /// `pipe2(2)`.
    ///
    /// # Errors
    /// The platform errno.
    fn pipe2(&self, flags: i32) -> Result<(i32, i32), Errno>;

    /// Fill six characters of a temporary-file suffix (`__randname`).
    fn random_suffix(&self, out: &mut [u8; 6]);
}

/// The lock seam.
///
/// `tid` is the 30-bit kernel thread id of `H-19`; `wait(word, expected)` returns when the
/// word no longer holds `expected` or after a spurious wakeup (the caller loops); `wake`
/// releases up to `n` waiters. `threaded()` may only go from `false` to `true`.
pub trait Lock {
    /// The calling thread's id, never zero.
    fn tid(&self) -> i32;

    /// Block until `word` no longer holds `expected`.
    fn wait(&self, word: &AtomicI32, expected: i32);

    /// Wake up to `n` waiters of `word`.
    fn wake(&self, word: &AtomicI32, n: i32);

    /// Whether more than one thread exists (the single-thread fast path).
    fn threaded(&self) -> bool;
}

/// Everything a [`crate::State`] needs from its host.
pub trait Env: SysFile + Lock + Sync + 'static {}

impl<T: SysFile + Lock + Sync + 'static> Env for T {}

#[cfg(test)]
#[path = "traits_ut.rs"]
mod tests;
