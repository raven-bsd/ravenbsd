// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c unistd/IO exports that are ready for the next frozen-ABI round.
//!
//! These are thin C-ABI forwarders to the existing `raven-libc-platform` functions. They are
//! exported now but intentionally absent from `Symbol.map`/the frozen list until the ABI window
//! opens; the round-1/2 wrappers in the platform crate are the bodies.

#![allow(dead_code, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_long, c_uint, c_void};

use raven_libc_abi::structs::{FdSet, Iovec, Pollfd, Rlimit, SigsetT, Statvfs, Timespec, Timeval};

use crate::ffi::platform::global;
use crate::ffi::ret;

fn map_error(error: raven_libc_platform::Errno) -> raven_libc_abi::errno::Errno {
    // Both crates use the same positive Linux/BSD error numbers (`raven-libc-platform` re-exports
    // `raven_abi::errno::Errno`); the C shell only changes the newtype.
    raven_libc_abi::errno::Errno(error.0)
}

fn unit(result: Result<(), raven_libc_platform::Errno>) -> c_int {
    ret::unit(result.map_err(map_error))
}

fn int(result: Result<c_int, raven_libc_platform::Errno>) -> c_int {
    ret::int(result.map_err(map_error))
}

fn ssize(result: Result<usize, raven_libc_platform::Errno>) -> isize {
    ret::platform_ssize(result)
}

/// Read packed directory entries.
///
/// # Safety
/// `dirp` has `count` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getdents(fd: c_int, dirp: *mut c_void, count: usize) -> c_int {
    // SAFETY: the caller's buffer contract; NULL with zero is the empty slice.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(dirp.cast(), count) };
    int(raven_libc_platform::compat::getdents(global(), fd, buffer)
        .map(|written| c_int::try_from(written).unwrap_or(c_int::MAX)))
}

/// Advise the kernel about a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fadvise(fd: c_int, offset: i64, len: i64, advice: c_int) -> c_int {
    unit(global().posix_fadvise(fd, offset, len, advice))
}

/// Allocate file space.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fallocate(fd: c_int, offset: i64, len: i64) -> c_int {
    unit(global().posix_fallocate(fd, offset, len))
}

/// Poll with an atomic signal mask.
///
/// # Safety
/// `fds` has `nfds` writable records; `timeout`/`sigmask` are NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ppoll(
    fds: *mut Pollfd,
    nfds: u64,
    timeout: *const Timespec,
    sigmask: *const SigsetT,
) -> c_int {
    let count = usize::try_from(nfds).unwrap_or(usize::MAX);
    let records = if fds.is_null() || count == 0 {
        &mut []
    } else {
        // SAFETY: the caller supplied `count` writable pollfd records.
        unsafe { core::slice::from_raw_parts_mut(fds, count) }
    };
    // SAFETY: NULL is `None`; otherwise the caller supplied a valid Timespec/SigsetT.
    let timeout = unsafe { timeout.as_ref() };
    // SAFETY: same contract as `timeout`.
    let mask = unsafe { sigmask.as_ref() }.map(|value| &value.bytes[..]);
    ret::platform_int(
        raven_libc_platform::poll::ppoll(global(), records, timeout, mask)
            .map(|ready| c_int::try_from(ready).unwrap_or(c_int::MAX)),
    )
}

/// Give advice about a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn madvise(addr: *mut c_void, len: usize, advice: c_int) -> c_int {
    unit(raven_libc_platform::mem::madvise(
        global(),
        addr as usize,
        len,
        advice,
    ))
}

/// Lock a memory range with flags.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlock2(addr: *const c_void, len: usize, flags: c_uint) -> c_int {
    unit(raven_libc_platform::compat::mlock2(
        global(),
        addr as usize,
        len,
        flags as i32,
    ))
}

/// Lock all mapped pages.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlockall(flags: c_int) -> c_int {
    unit(raven_libc_platform::mem::mlockall(global(), flags))
}

/// Lock a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlock(addr: *const c_void, len: usize) -> c_int {
    unit(raven_libc_platform::mem::mlock(
        global(),
        addr as usize,
        len,
    ))
}

/// Synchronize a mapping with its backing store.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn msync(addr: *mut c_void, len: usize, flags: c_int) -> c_int {
    unit(raven_libc_platform::mem::msync(
        global(),
        addr as usize,
        len,
        flags,
    ))
}

/// Unlock all mapped pages.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn munlockall() -> c_int {
    unit(raven_libc_platform::mem::munlockall(global()))
}

/// Unlock a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn munlock(addr: *const c_void, len: usize) -> c_int {
    unit(raven_libc_platform::mem::munlock(
        global(),
        addr as usize,
        len,
    ))
}

/// POSIX memory advice.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_madvise(addr: *mut c_void, len: usize, advice: c_int) -> c_int {
    unit(raven_libc_platform::compat::posix_madvise(
        global(),
        addr as usize,
        len,
        advice,
    ))
}

/// Read a 64-bit resource limit.
///
/// # Safety
/// `limit` points to a writable `struct rlimit`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getrlimit64(resource: c_int, limit: *mut Rlimit) -> c_int {
    if limit.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match raven_libc_platform::compat::getrlimit64(global(), resource) {
        Ok((soft, hard)) => {
            // SAFETY: the caller supplied a writable Rlimit.
            unsafe {
                *limit = Rlimit {
                    rlim_cur: soft,
                    rlim_max: hard,
                }
            };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}

/// `select` over file descriptor sets.
///
/// # Safety
/// The three sets are NULL or writable; `timeout` is NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn select(
    nfds: c_int,
    readfds: *mut FdSet,
    writefds: *mut FdSet,
    exceptfds: *mut FdSet,
    timeout: *const Timeval,
) -> c_int {
    // SAFETY: each NULL set is an empty slice; otherwise it is 16 writable words.
    let read = unsafe { fd_set(readfds) };
    // SAFETY: same contract as `read`.
    let write = unsafe { fd_set(writefds) };
    // SAFETY: same contract as `read`.
    let except = unsafe { fd_set(exceptfds) };
    // SAFETY: NULL is `None`.
    let timeout = unsafe { timeout.as_ref() };
    int(raven_libc_platform::poll::select(
        global(),
        nfds,
        read,
        write,
        except,
        timeout,
    ))
}

/// `pselect` over file descriptor sets with an atomic signal mask.
///
/// # Safety
/// The sets are NULL or writable; `timeout`/`sigmask` are NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pselect(
    nfds: c_int,
    readfds: *mut FdSet,
    writefds: *mut FdSet,
    exceptfds: *mut FdSet,
    timeout: *const Timespec,
    sigmask: *const SigsetT,
) -> c_int {
    // SAFETY: each NULL set is an empty slice; otherwise it is 16 writable words.
    let read = unsafe { fd_set(readfds) };
    // SAFETY: same contract as `read`.
    let write = unsafe { fd_set(writefds) };
    // SAFETY: same contract as `read`.
    let except = unsafe { fd_set(exceptfds) };
    // SAFETY: NULL is `None`.
    let timeout = unsafe { timeout.as_ref() };
    // SAFETY: NULL is `None`; otherwise the caller supplied a valid SigsetT.
    let mask = unsafe { sigmask.as_ref() }.map(|value| &value.bytes[..]);
    int(raven_libc_platform::poll::pselect(
        global(),
        nfds,
        read,
        write,
        except,
        timeout,
        mask,
    ))
}

/// Change a symlink's mode (unsupported on Raven).
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lchmod(path: *const c_char, mode: u32) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::lchmod(global(), path, mode))
}

/// Lock a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lockf(fd: c_int, function: c_int, size: i64) -> c_int {
    unit(global().lockf(fd, function, size))
}

/// Create a directory relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mkdirat(dirfd: c_int, path: *const c_char, mode: u32) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::mkdirat(
        global(),
        dirfd,
        path,
        mode,
    ))
}

/// Read file-system statistics for an open descriptor.
///
/// # Safety
/// `buf` points to a writable `struct statvfs`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fstatvfs(fd: c_int, buf: *mut Statvfs) -> c_int {
    if buf.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().fstatvfs(fd) {
        Ok(value) => {
            // SAFETY: the caller supplied a writable Statvfs.
            unsafe { *buf = value };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}

/// Vector read at an explicit offset.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn preadv(fd: c_int, iovs: *const Iovec, count: c_int, offset: i64) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let mut bufs = unsafe { iovec_mut(iovs, count) };
    ssize(raven_libc_platform::compat::preadv(
        global(),
        fd,
        &mut bufs,
        offset,
    ))
}

/// Vector write at an explicit offset.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pwritev(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let bufs = unsafe { iovec_ref(iovs, count) };
    ssize(raven_libc_platform::compat::pwritev(
        global(),
        fd,
        &bufs,
        offset,
    ))
}

/// Vector read with flags.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn preadv2(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
    flags: c_int,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let mut bufs = unsafe { iovec_mut(iovs, count) };
    ssize(raven_libc_platform::compat::preadv2(
        global(),
        fd,
        &mut bufs,
        offset,
        flags,
    ))
}

/// Vector write with flags.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pwritev2(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
    flags: c_int,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let bufs = unsafe { iovec_ref(iovs, count) };
    ssize(raven_libc_platform::compat::pwritev2(
        global(),
        fd,
        &bufs,
        offset,
        flags,
    ))
}

/// Set the program break (class B: ENOSYS).
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn brk(addr: *mut c_void) -> c_int {
    unit(raven_libc_platform::mem::brk(global(), addr as usize).map(|_| ()))
}

/// Duplicate a descriptor with flags.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn dup3(old: c_int, new: c_int, flags: c_int) -> c_int {
    int(raven_libc_platform::compat::dup3(global(), old, new, flags))
}

/// Effective-access check.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn eaccess(path: *const c_char, mode: c_int) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::eaccess(global(), path, mode))
}

/// Effective-uid access check.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn euidaccess(path: *const c_char, mode: c_int) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::euidaccess(
        global(),
        path,
        mode,
    ))
}

/// Access check relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn faccessat(
    dirfd: c_int,
    path: *const c_char,
    mode: c_int,
    flags: c_int,
) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::faccessat(
        global(),
        dirfd,
        path,
        mode,
        flags,
    ))
}

/// Change the working directory by descriptor.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fchdir(fd: c_int) -> c_int {
    unit(global().fchdir(fd))
}

/// Flush file data.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fdatasync(fd: c_int) -> c_int {
    unit(raven_libc_platform::compat::fdatasync(global(), fd))
}

/// Read a path configuration value.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fpathconf(fd: c_int, name: c_int) -> c_long {
    match global().fpathconf(fd, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}

/// Truncate a descriptor to a 64-bit size.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ftruncate64(fd: c_int, size: i64) -> c_int {
    unit(raven_libc_platform::compat::ftruncate64(global(), fd, size))
}

/// Fill a buffer with entropy.
///
/// # Safety
/// `buffer` has `len` writable bytes and `len <= 256`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getentropy(buffer: *mut c_void, len: usize) -> c_int {
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), len) };
    unit(global().getentropy(buffer))
}

/// Return the page size.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getpagesize() -> c_int {
    raven_libc_platform::mem::getpagesize()
}

/// Read a path configuration value.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pathconf(path: *const c_char, name: c_int) -> c_long {
    if path.is_null() {
        return ret::long(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match global().pathconf(path, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}

/// Create a pipe.
///
/// # Safety
/// `fds` points to two writable `int` values.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pipe(fds: *mut c_int) -> c_int {
    if fds.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().pipe() {
        Ok((read, write)) => {
            // SAFETY: the caller supplied two writable ints.
            unsafe {
                core::ptr::write(fds, read);
                core::ptr::write(fds.add(1), write);
            }
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}

/// Close a descriptor with `posix_close` semantics.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_close(fd: c_int, flags: c_int) -> c_int {
    if flags != 0 {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    unit(raven_libc_platform::compat::posix_close(global(), fd))
}

/// Read a symbolic link relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated; `buf` has `len` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn readlinkat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut c_char,
    len: usize,
) -> isize {
    if path.is_null() {
        return ret::ssize(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    // SAFETY: the caller's writable buffer contract.
    let buf = unsafe { crate::ffi::cstr::slice_mut(buf, len) };
    ssize(raven_libc_platform::compat::readlinkat(
        global(),
        dirfd,
        path,
        buf,
    ))
}

/// Set the host name.
///
/// # Safety
/// `name` has `len` readable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn sethostname(name: *const c_char, len: usize) -> c_int {
    // SAFETY: the caller's readable buffer contract.
    let name = unsafe { crate::ffi::cstr::bytes_n(name, len) };
    unit(global().sethostname(name))
}

/// Sleep for whole seconds.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn sleep(seconds: c_uint) -> c_uint {
    raven_libc_platform::time::sleep(global(), seconds).unwrap_or(seconds)
}

/// Synchronize a file system.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn syncfs(fd: c_int) -> c_int {
    unit(raven_libc_platform::compat::syncfs(global(), fd))
}

/// Truncate a path to a 64-bit size.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn truncate64(path: *const c_char, size: i64) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::truncate64(
        global(),
        path,
        size,
    ))
}

/// Truncate a path.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn truncate(path: *const c_char, size: i64) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(global().truncate(path, size))
}

/// Return the terminal name for a descriptor.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ttyname(fd: c_int) -> *mut c_char {
    static mut BUFFER: [u8; 256] = [0; 256];
    match raven_libc_platform::compat::ttyname(global(), fd) {
        Ok(name) => {
            // `addr_of_mut!` avoids forming a reference to the mutable static.
            let buffer = core::ptr::addr_of_mut!(BUFFER).cast::<u8>();
            let count = name.len().min(255);
            // SAFETY: `count <= 255` fits the static buffer.
            unsafe {
                core::ptr::copy_nonoverlapping(name.as_ptr(), buffer, count);
                *buffer.add(count) = 0;
            }
            buffer.cast()
        }
        Err(error) => ret::null(map_error(error)),
    }
}

/// Sleep for microseconds.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn usleep(microseconds: c_uint) -> c_int {
    unit(raven_libc_platform::time::usleep(global(), microseconds))
}

unsafe fn fd_set<'a>(set: *mut FdSet) -> &'a mut [u64] {
    if set.is_null() {
        &mut []
    } else {
        // SAFETY: the caller supplied a writable FdSet (16 words).
        unsafe { &mut (*set).bytes }
    }
}

unsafe fn iovec_mut<'a>(iovs: *const Iovec, count: c_int) -> Vec<&'a mut [u8]> {
    let count = usize::try_from(count).unwrap_or(0);
    let mut bufs = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: the caller supplied `count` readable iovec records.
        let iov = unsafe { &*iovs.add(index) };
        // SAFETY: `iov_base`/`iov_len` obey the caller's iovec contract.
        bufs.push(unsafe {
            crate::ffi::cstr::slice_mut(iov.iov_base.cast(), iov.iov_len as usize)
        });
    }
    bufs
}

unsafe fn iovec_ref<'a>(iovs: *const Iovec, count: c_int) -> Vec<&'a [u8]> {
    let count = usize::try_from(count).unwrap_or(0);
    let mut bufs = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: the caller supplied `count` readable iovec records.
        let iov = unsafe { &*iovs.add(index) };
        // SAFETY: `iov_base`/`iov_len` obey the caller's iovec contract.
        bufs.push(unsafe { crate::ffi::cstr::bytes_n(iov.iov_base.cast(), iov.iov_len as usize) });
    }
    bufs
}

#[cfg(test)]
#[path = "p1c_exports_ut.rs"]
mod tests;
