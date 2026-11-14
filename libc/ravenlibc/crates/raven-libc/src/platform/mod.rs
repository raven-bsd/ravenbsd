// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The platform I/O family of the C-ABI shell (C60c): `stat`, `mmap`, `poll`, `sysconf`,
//! `uname`, `getcwd`, `access`, `unlink` and the variadic `open`/`openat`/`ioctl` entries of
//! C55's `raven-libc-platform`.
//!
//! Every body is a pointer check plus one platform call; the platform crate owns path
//! resolution, the fd table, the scheme clients and the error values. The shell owns the C
//! `struct stat` copy, the `MAP_FAILED` sentinel and the `va_list` reads that C59's trampolines
//! hand over. `ShellDirSys` is the descriptor seam of `raven-libc-fts` (C46) and is reused by
//! `glob`; no directory parser or path algorithm lives here.

pub(crate) mod __raven_va_ioctl;
pub(crate) mod __raven_va_open;
pub(crate) mod __raven_va_openat;
pub(crate) mod access;
pub(crate) mod creat;
pub(crate) mod creat64;
pub(crate) mod fstat;
pub(crate) mod getcwd;
pub(crate) mod ioctl;
pub(crate) mod lstat;
pub(crate) mod mmap;
pub(crate) mod mprotect;
pub(crate) mod munmap;
pub(crate) mod open;
pub(crate) mod open64;
pub(crate) mod openat;
pub(crate) mod openat64;
pub(crate) mod poll;
pub(crate) mod stat;
pub(crate) mod sysconf;
pub(crate) mod uname;
pub(crate) mod unlink;

use raven_libc_abi::fcntl::{AT_SYMLINK_NOFOLLOW, F_GETFL};
use raven_libc_fts::{DirSys, Fd, Whence};

/// The platform error as the ABI crate's value (both are positive Linux numbers).
pub(crate) fn map_error(error: raven_libc_platform::Errno) -> raven_libc_abi::errno::Errno {
    raven_libc_abi::errno::Errno(error.0)
}

/// Allocate a NUL-terminated C copy of `bytes`.
///
/// # Safety
/// The returned pointer comes from `malloc`; the caller owns it.
pub(crate) unsafe fn copy_c_path(bytes: &[u8]) -> *mut core::ffi::c_char {
    let needed = bytes.len().saturating_add(1);
    // SAFETY: the shell allocator returns `needed` writable bytes or NULL.
    let raw = unsafe { crate::malloc::abi_alloc(needed) };
    if raw.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOMEM as u16,
        ));
        return core::ptr::null_mut();
    }
    // SAFETY: `raw` has `needed` bytes; the source has `bytes.len()`.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), raw, bytes.len());
        *raw.add(bytes.len()) = 0;
    }
    raw.cast::<core::ffi::c_char>()
}

/// The one `DirSys` instance of the shell: every call goes to the process platform object.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ShellDirSys;

impl DirSys for ShellDirSys {
    fn openat(
        &mut self,
        dirfd: i32,
        name: &[u8],
        flags: u32,
    ) -> Result<Fd, raven_libc_abi::errno::Errno> {
        crate::ffi::platform::global()
            .openat(dirfd, name, flags, 0)
            .map(Fd)
            .map_err(map_error)
    }

    fn fstatat(
        &mut self,
        dirfd: i32,
        name: &[u8],
        nofollow: bool,
    ) -> Result<raven_libc_abi::structs::Stat, raven_libc_abi::errno::Errno> {
        let flags = if nofollow { AT_SYMLINK_NOFOLLOW } else { 0 };
        crate::ffi::platform::global()
            .stat_at(dirfd, name, flags)
            .map_err(map_error)
    }

    fn fstat(
        &mut self,
        fd: Fd,
    ) -> Result<raven_libc_abi::structs::Stat, raven_libc_abi::errno::Errno> {
        crate::ffi::platform::global()
            .fstat(fd.0)
            .map_err(map_error)
    }

    fn getdents(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, raven_libc_abi::errno::Errno> {
        raven_libc_platform::compat::getdents(crate::ffi::platform::global(), fd.0, buf)
            .map_err(map_error)
    }

    fn lseek(
        &mut self,
        fd: Fd,
        off: i64,
        whence: Whence,
    ) -> Result<i64, raven_libc_abi::errno::Errno> {
        let value = match whence {
            Whence::Set => raven_libc_abi::fcntl::SEEK_SET,
            Whence::Cur => raven_libc_abi::fcntl::SEEK_CUR,
            Whence::End => raven_libc_abi::fcntl::SEEK_END,
        };
        crate::ffi::platform::global()
            .lseek(fd.0, off, value)
            .map_err(map_error)
    }

    fn close(&mut self, fd: Fd) -> Result<(), raven_libc_abi::errno::Errno> {
        crate::ffi::platform::global()
            .close(fd.0)
            .map_err(map_error)
    }

    fn fcntl_getfl(&mut self, fd: Fd) -> Result<i32, raven_libc_abi::errno::Errno> {
        let value = crate::ffi::platform::global()
            .fcntl(fd.0, F_GETFL, 0)
            .map_err(map_error)?;
        i32::try_from(value)
            .map_err(|_| raven_libc_abi::errno::Errno(raven_libc_abi::errno::EOVERFLOW as u16))
    }
}
