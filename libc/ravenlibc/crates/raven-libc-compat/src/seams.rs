// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The platform seams of `raven-libc-compat` (doc 15 3.B; ticket C65 section 4.2).
//!
//! The crate never calls the kernel, the clock, the environment or a random device. Every such
//! access is a trait method implemented by the C-ABI shell over `raven-libc-platform`,
//! `raven-libc-rand` and the network/tty servers. The fakes used by the unit tests live in
//! the test-only `testutil` module.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

/// Whole-file read seam for `getcap`, `getttyent`, `getusershell` and `mntent`.
///
/// `out` is cleared first; the implementation appends at most [`READ_FILE_CAP`] bytes and returns
/// `ERANGE` when the file is longer. A missing file is an `Err` (the caller decides whether that
/// is "not found" or "system error").
pub trait ReadFile {
    /// Read the whole file named `path`.
    fn read_all(&mut self, path: &[u8], out: &mut Vec<u8>) -> Result<(), Errno>;
}

/// The cap of [`ReadFile`] (doc 15 4.6, ticket 4.1): one MiB.
pub const READ_FILE_CAP: usize = 1024 * 1024;

/// Line seam for `fparseln`: the `fgetln` equivalent.
///
/// A line is returned **without** a trailing NUL and **with** its newline when the source had
/// one. `None` is end of file.
pub trait LineSource {
    /// The next physical line, or `None` at end of file.
    fn next_line(&mut self) -> Option<Vec<u8>>;
}

/// Entropy seam: `uuid_create` and `bcrypt_gensalt` only.
///
/// The shell passes `arc4random_buf`; the crate never reads `AT_RANDOM`, the clock or `randd`
/// directly (A-hard-12/13, PG-52).
pub trait Entropy {
    /// Fill `buf` with random bytes.
    fn fill(&mut self, buf: &mut [u8]);
}

/// The operations `pidfile()` needs (open/write, unlink, pid, `atexit`).
pub trait PidfileOps {
    /// Create `path` (truncate or exclusive-create as the caller decided) and write `contents`.
    fn create_write(&mut self, path: &[u8], contents: &[u8]) -> Result<(), Errno>;
    /// Remove `path`; errors are ignored like the donor's `(void)unlink`.
    fn unlink(&mut self, path: &[u8]);
    /// The current process id.
    fn getpid(&self) -> i32;
    /// Register `f` for process exit.
    fn atexit(&mut self, f: fn()) -> Result<(), Errno>;
}

/// The terminal operations `readpassphrase` needs.
pub trait TtyOps {
    /// Open `/dev/tty` read/write and return the descriptor.
    fn open_tty(&mut self) -> Result<i32, Errno>;
    /// `tcgetattr`.
    fn tcgetattr(&mut self, fd: i32) -> Result<crate::seams::Termios, Errno>;
    /// `tcsetattr` with `TCSAFLUSH`.
    fn tcsetattr(&mut self, fd: i32, t: &crate::seams::Termios) -> Result<(), Errno>;
    /// `write` the whole buffer; returns the byte count.
    fn write(&mut self, fd: i32, b: &[u8]) -> Result<usize, Errno>;
    /// Read one byte; `Ok(None)` is end of file.
    fn read_byte(&mut self, fd: i32) -> Result<Option<u8>, Errno>;
    /// `close`.
    fn close(&mut self, fd: i32);
    /// The signal that arrived since the last call, if any (the shell's handler sets a flag).
    fn pending_signal(&mut self) -> Option<i32>;
}

/// The operations the `openpty` family needs.
pub trait PtyOps {
    /// `posix_openpt(flags)`.
    fn posix_openpt(&mut self, flags: i32) -> Result<i32, Errno>;
    /// `grantpt`.
    fn grantpt(&mut self, fd: i32) -> Result<(), Errno>;
    /// `unlockpt`.
    fn unlockpt(&mut self, fd: i32) -> Result<(), Errno>;
    /// `ptsname_r` into `buf`; returns the length without the NUL.
    fn ptsname_r(&mut self, fd: i32, buf: &mut [u8]) -> Result<usize, Errno>;
    /// `open(path, flags)`.
    fn open(&mut self, path: &[u8], flags: i32) -> Result<i32, Errno>;
    /// `close`.
    fn close(&mut self, fd: i32);
    /// `tcsetattr` with `TCSAFLUSH`.
    fn tcsetattr(&mut self, fd: i32, t: &crate::seams::Termios) -> Result<(), Errno>;
    /// `ioctl(TIOCSWINSZ)`.
    fn set_winsize(&mut self, fd: i32, ws: &crate::seams::Winsize) -> Result<(), Errno>;
    /// `setsid`.
    fn setsid(&mut self) -> Result<i32, Errno>;
    /// `ioctl(TIOCSCTTY)`.
    fn set_ctty(&mut self, fd: i32) -> Result<(), Errno>;
    /// `dup2(a, b)`.
    fn dup2(&mut self, a: i32, b: i32) -> Result<i32, Errno>;
    /// `fork`; `Err(ENOSYS)` until PG-45 exists (A-bsd-8).
    fn fork(&mut self) -> Result<ForkResult, Errno>;
}

/// The outcome of `PtyOps::fork`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForkResult {
    /// The parent, with the child's pid.
    Parent(i32),
    /// The child.
    Child,
}

pub use raven_libc_abi::generated::structs::Termios;

/// The Linux `struct winsize`.
///
/// `raven-libc-abi` (C36) does not define it; the C-ABI shell declares the matching C record.
/// `size_of::<Winsize>() == 8`, `align_of == 2` (probed by a unit test).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Winsize {
    /// Rows.
    pub ws_row: u16,
    /// Columns.
    pub ws_col: u16,
    /// Pixel width.
    pub ws_xpixel: u16,
    /// Pixel height.
    pub ws_ypixel: u16,
}

const _: () = assert!(core::mem::size_of::<Winsize>() == 8);
const _: () = assert!(core::mem::align_of::<Winsize>() == 2);

#[cfg(test)]
#[path = "seams_ut.rs"]
mod tests;
