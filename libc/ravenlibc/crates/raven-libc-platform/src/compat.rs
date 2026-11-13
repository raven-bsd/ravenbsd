// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compatibility aliases and the thin `*at`/`*64`/BSD spellings.
//!
//! The C shell exports these names; the bodies are one call to the canonical operation. A name
//! whose primitive is missing stays a registered stub in [`crate::stubs`] and is not repeated
//! here as a fake success.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;
use crate::lock::Lock;
use crate::platform::Platform;

/// `posix_close`.
pub fn posix_close<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<(), Errno> {
    platform.close(fd)
}

/// `closefrom`.
pub fn closefrom<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    first: u32,
) -> Result<(), Errno> {
    platform.close_from(first)
}

/// `close_range`.
pub fn close_range<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    first: u32,
    last: u32,
) -> Result<(), Errno> {
    platform.close_range(first, last)
}

/// `open64`.
pub fn open64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    flags: u32,
    mode: u32,
) -> Result<i32, Errno> {
    platform.open(path, flags, mode)
}

/// `openat64`.
pub fn openat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    flags: u32,
    mode: u32,
) -> Result<i32, Errno> {
    platform.openat(dirfd, path, flags, mode)
}

/// `creat64`.
pub fn creat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    mode: u32,
) -> Result<i32, Errno> {
    platform.creat(path, mode)
}

/// `dup3`.
pub fn dup3<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    old: i32,
    new: i32,
    flags: i32,
) -> Result<i32, Errno> {
    if flags & !fcntl::O_CLOEXEC != 0 {
        return Err(Errno::EINVAL);
    }
    if old == new {
        return Err(Errno::EINVAL);
    }
    platform.dup2(old, new, flags & fcntl::O_CLOEXEC != 0)
}

/// `fcntl(F_DUPFD, F_DUPFD_CLOEXEC)` is a method; this alias helps the 64-bit spelling table.
pub fn lseek64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    offset: i64,
    whence: i32,
) -> Result<i64, Errno> {
    platform.lseek(fd, offset, whence)
}

/// `ftruncate64`.
pub fn ftruncate64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    size: i64,
) -> Result<(), Errno> {
    platform.ftruncate(fd, size)
}

/// `truncate64`.
pub fn truncate64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    size: i64,
) -> Result<(), Errno> {
    platform.truncate(path, size)
}

/// `pread64`: `ENOSYS` until PG-22.
pub fn pread64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    buf: &mut [u8],
    offset: i64,
) -> Result<usize, Errno> {
    platform.pread(fd, buf, offset)
}

/// `pwrite64`: `ENOSYS` until PG-22.
pub fn pwrite64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    buf: &[u8],
    offset: i64,
) -> Result<usize, Errno> {
    platform.pwrite(fd, buf, offset)
}

/// `preadv`: requires PG-22.
pub fn preadv<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _bufs: &mut [&mut [u8]],
    _offset: i64,
) -> Result<usize, Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `pwritev`: requires PG-22.
pub fn pwritev<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    _bufs: &[&[u8]],
    _offset: i64,
) -> Result<usize, Errno> {
    let _ = platform.get(fd)?;
    Err(Errno::ENOSYS)
}

/// `preadv2`: requires PG-22.
pub fn preadv2<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    bufs: &mut [&mut [u8]],
    offset: i64,
    _flags: i32,
) -> Result<usize, Errno> {
    preadv(platform, fd, bufs, offset)
}

/// `pwritev2`: requires PG-22.
pub fn pwritev2<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    bufs: &[&[u8]],
    offset: i64,
    _flags: i32,
) -> Result<usize, Errno> {
    pwritev(platform, fd, bufs, offset)
}

/// `fdatasync` is `fsync` with a data-only flag until PG-21.
pub fn fdatasync<B: SysBackend, L: Lock>(platform: &Platform<B, L>, fd: i32) -> Result<(), Errno> {
    platform.fsync(fd)
}

/// `syncfs`.
pub fn syncfs<B: SysBackend, L: Lock>(platform: &Platform<B, L>, _fd: i32) -> Result<(), Errno> {
    platform.sync()
}

/// `posix_fadvise64`.
pub fn posix_fadvise64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    offset: i64,
    len: i64,
    advice: i32,
) -> Result<(), Errno> {
    platform.posix_fadvise(fd, offset, len, advice)
}

/// `stat64`.
pub fn stat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.stat(path)
}

/// `lstat64`.
pub fn lstat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.lstat(path)
}

/// `fstat64`.
pub fn fstat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.fstat(fd)
}

/// `fstatat`.
pub fn fstatat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    flags: i32,
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.stat_at(dirfd, path, flags)
}

/// `newfstatat` (Linux spelling).
pub fn newfstatat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    flags: i32,
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.stat_at(dirfd, path, flags)
}

/// `fstatat64`.
pub fn fstatat64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    flags: i32,
) -> Result<raven_libc_abi::structs::Stat, Errno> {
    platform.stat_at(dirfd, path, flags)
}

/// `faccessat`.
pub fn faccessat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    mode: i32,
    flags: i32,
) -> Result<(), Errno> {
    if flags & !(fcntl::AT_EACCESS | fcntl::AT_SYMLINK_NOFOLLOW) != 0 {
        return Err(Errno::EINVAL);
    }
    if dirfd == fcntl::AT_FDCWD {
        return platform.access(path, mode);
    }
    let base = platform.base_for(dirfd)?;
    let resolved = crate::path::resolve(&base, path, false, &crate::path::DEFAULT_LIMITS)
        .map_err(crate::path::path_errno)?;
    platform.access(&resolved.abs, mode)
}

/// `eaccess`.
pub fn eaccess<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    mode: i32,
) -> Result<(), Errno> {
    platform.access(path, mode)
}

/// `euidaccess`.
pub fn euidaccess<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    mode: i32,
) -> Result<(), Errno> {
    platform.access(path, mode)
}

/// `fchmodat`: `AT_SYMLINK_NOFOLLOW` is `EOPNOTSUPP`; the cwd form works.
pub fn fchmodat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    mode: u32,
    flags: i32,
) -> Result<(), Errno> {
    if flags & fcntl::AT_SYMLINK_NOFOLLOW != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    if dirfd == fcntl::AT_FDCWD {
        return platform.chmod(path, mode);
    }
    let base = platform.base_for(dirfd)?;
    let resolved = crate::path::resolve(&base, path, false, &crate::path::DEFAULT_LIMITS)
        .map_err(crate::path::path_errno)?;
    platform.chmod(&resolved.abs, mode)
}

/// `lchmod`: `EOPNOTSUPP` (Linux behaviour).
pub fn lchmod<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
    _mode: u32,
) -> Result<(), Errno> {
    let _ = platform.lstat(path)?;
    Err(Errno::EOPNOTSUPP)
}

/// `symlinkat`.
pub fn symlinkat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    target: &[u8],
    dirfd: i32,
    link: &[u8],
) -> Result<(), Errno> {
    if dirfd == fcntl::AT_FDCWD {
        return platform.symlink(target, link);
    }
    let base = platform.base_for(dirfd)?;
    let resolved = crate::path::resolve(&base, link, false, &crate::path::DEFAULT_LIMITS)
        .map_err(crate::path::path_errno)?;
    platform.root_call(raven_proto_fs::Call::Symlink {
        path: &resolved.abs,
        target,
    })
}

/// `readlinkat`.
pub fn readlinkat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    buf: &mut [u8],
) -> Result<usize, Errno> {
    if dirfd == fcntl::AT_FDCWD {
        return platform.readlink(path, buf);
    }
    let base = platform.base_for(dirfd)?;
    let resolved = crate::path::resolve(&base, path, false, &crate::path::DEFAULT_LIMITS)
        .map_err(crate::path::path_errno)?;
    platform.readlink(&resolved.abs, buf)
}

/// `renameat`.
pub fn renameat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    olddir: i32,
    from: &[u8],
    newdir: i32,
    to: &[u8],
) -> Result<(), Errno> {
    if olddir != fcntl::AT_FDCWD || newdir != fcntl::AT_FDCWD {
        return Err(Errno::ENOSYS);
    }
    platform.rename(from, to)
}

/// `mkdirat`.
pub fn mkdirat<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    dirfd: i32,
    path: &[u8],
    mode: u32,
) -> Result<(), Errno> {
    if dirfd != fcntl::AT_FDCWD {
        return Err(Errno::ENOSYS);
    }
    platform.mkdir(path, mode)
}

/// `umask`.
pub fn umask<B: SysBackend, L: Lock>(platform: &Platform<B, L>, mask: u32) -> u32 {
    platform.set_umask(mask)
}

/// `fstatfs`.
pub fn fstatfs<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<raven_proto_fs::StatFs, Errno> {
    platform.fstatfs(fd)
}

/// `statfs`.
pub fn statfs<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
) -> Result<raven_proto_fs::StatFs, Errno> {
    platform.statfs(path)
}

/// `statfs64`.
pub fn statfs64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
) -> Result<raven_proto_fs::StatFs, Errno> {
    platform.statfs(path)
}

/// `fstatfs64`.
pub fn fstatfs64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<raven_proto_fs::StatFs, Errno> {
    platform.fstatfs(fd)
}

/// `fstatvfs64`.
pub fn fstatvfs64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<raven_libc_abi::structs::Statvfs, Errno> {
    platform.fstatvfs(fd)
}

/// `statvfs64`.
pub fn statvfs64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    path: &[u8],
) -> Result<raven_libc_abi::structs::Statvfs, Errno> {
    platform.statvfs(path)
}

/// `mmap64`.
pub fn mmap64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    protection: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> Result<usize, Errno> {
    crate::mem::mmap(platform, addr, len, protection, flags, fd, offset)
}

/// `epoll_create` (the size argument is ignored since Linux 2.6.8).
pub fn epoll_create<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    _size: i32,
) -> Result<i32, Errno> {
    crate::poll::epoll_create1(platform, 0)
}

/// `ttyname`; the shell copies the returned bytes into its per-thread buffer.
pub fn ttyname<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<Vec<u8>, Errno> {
    let reference = platform.get(fd)?;
    if reference.kind() != OfdKind::Tty {
        return Err(Errno::ENOTTY);
    }
    reference
        .path()
        .map(|path| path.to_vec())
        .ok_or(Errno::ENOTTY)
}

/// `ttyname_r`.
pub fn ttyname_r<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    buf: &mut [u8],
) -> Result<usize, Errno> {
    let name = ttyname(platform, fd)?;
    let needed = name.len().saturating_add(1);
    if buf.len() < needed {
        return Err(Errno::ERANGE);
    }
    buf.get_mut(..name.len())
        .ok_or(Errno::ERANGE)?
        .copy_from_slice(&name);
    if let Some(slot) = buf.get_mut(name.len()) {
        *slot = 0;
    }
    Ok(needed)
}

/// `lockf64`.
pub fn lockf64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    function: i32,
    size: i64,
) -> Result<(), Errno> {
    platform.lockf(fd, function, size)
}

/// `mlock2`.
pub fn mlock2<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    _flags: i32,
) -> Result<(), Errno> {
    crate::mem::mlock(platform, addr, len)
}

/// `posix_madvise`.
pub fn posix_madvise<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    advice: i32,
) -> Result<(), Errno> {
    crate::mem::madvise(platform, addr, len, advice)
}

/// `get_current_dir_name`.
pub fn get_current_dir_name<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
) -> Result<Vec<u8>, Errno> {
    let mut cwd = platform.cwd();
    cwd.push(0);
    Ok(cwd)
}

/// `getrlimit`.
pub fn getrlimit<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    resource: i32,
) -> Result<(u64, u64), Errno> {
    if resource != 7 {
        return Err(Errno::EINVAL);
    }
    let limits = platform.rlimits();
    Ok((u64::from(limits.nofile_soft), u64::from(limits.nofile_hard)))
}

/// `setrlimit`.
pub fn setrlimit<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    resource: i32,
    soft: u64,
    hard: u64,
) -> Result<(), Errno> {
    if resource != 7 || soft > hard || hard > u64::from(u32::MAX) {
        return Err(Errno::EINVAL);
    }
    platform.set_nofile_limits(soft as u32, hard as u32);
    Ok(())
}

/// `prlimit`.
pub fn prlimit<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    _pid: i32,
    resource: i32,
    new: Option<(u64, u64)>,
) -> Result<(u64, u64), Errno> {
    if let Some((soft, hard)) = new {
        setrlimit(platform, resource, soft, hard)?;
    }
    getrlimit(platform, resource)
}

/// `getrlimit64`.
pub fn getrlimit64<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    resource: i32,
) -> Result<(u64, u64), Errno> {
    getrlimit(platform, resource)
}

/// `getpriority`: `ENOSYS` until PG-28.
pub fn getpriority<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _which: i32,
    _who: i32,
) -> Result<i32, Errno> {
    Err(Errno::ENOSYS)
}

/// `setpriority`: `ENOSYS` until PG-28.
pub fn setpriority<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _which: i32,
    _who: i32,
    _priority: i32,
) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// `nice` with the O-6 default: non-negative is a no-op success, negative is `EPERM`.
pub fn nice<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    increment: i32,
) -> Result<i32, Errno> {
    crate::sched::nice(increment, None, |_| Ok(()))
}

/// `time`.
pub fn time<B: SysBackend, L: Lock>(platform: &Platform<B, L>) -> Result<i64, Errno> {
    Ok(crate::time::clock_gettime(platform, 0)?.tv_sec)
}

/// `getdents`: a raw `READ` on a directory handle.
pub fn getdents<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    buf: &mut [u8],
) -> Result<usize, Errno> {
    platform.read(fd, buf)
}

/// `getdirentries` (BSD spelling of `getdents`).
pub fn getdirentries<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    buf: &mut [u8],
) -> Result<usize, Errno> {
    platform.read(fd, buf)
}

/// `umount2`.
pub fn umount2<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    target: &[u8],
    _flags: i32,
) -> Result<(), Errno> {
    platform.umount(target)
}

/// `unmount` (BSD spelling).
pub fn unmount<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    target: &[u8],
    flags: i32,
) -> Result<(), Errno> {
    umount2(platform, target, flags)
}

#[cfg(test)]
#[path = "compat_ut.rs"]
mod tests;
