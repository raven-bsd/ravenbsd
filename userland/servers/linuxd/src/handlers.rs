// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The syscall handlers of the Linux personality.
//!
//! The B20 skeleton serves `write`, `exit_group`, `brk`, anonymous `mmap`, `getpid`, `uname`,
//! `rt_sigprocmask` and `arch_prctl`; B30 adds the synthetic file calls and B25 adds the metadata
//! batch (`stat`, `lstat`, `fstat`, `newfstatat`, `statx`, `access`, `fchmodat`, `fchownat`,
//! `utimensat` and `truncate`). Every other number reaches the dispatcher's `-ENOSYS` path and is
//! counted there.
//!
//! `brk` and anonymous `mmap` are served from two arenas the loader maps once at load time, so a
//! handler never needs a kernel call and the dispatcher stays a pure function table. The arenas and
//! their bounds are recorded in the process table (`brk_limit`, `mmap_limit`).

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_linux_abi::Arch;
use raven_linux_abi::flags::at_fd::{
    AT_EMPTY_PATH, AT_NO_AUTOMOUNT, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW,
};
use raven_linux_dispatch::{Dispatcher, LinuxContext, SyscallArgs};

use crate::procfs::{self, Resource};
use crate::stat as stat_layout;
use crate::table::{FileKind, LinuxProcess};

/// `MAP_ANONYMOUS` from the Linux UAPI (the dispatcher's `raven-linux-abi` crate owns the number).
const MAP_ANONYMOUS: i32 = 0x20;

/// `PROT_EXEC` from the Linux UAPI.
const PROT_EXEC: i32 = 0x4;

/// Page size of the guest's `mmap`.
const PAGE: u64 = 4096;

/// `ARCH_SET_FS` of x86-64.
const ARCH_SET_FS: i32 = 0x1002;

/// `ARCH_GET_FS` of x86-64.
const ARCH_GET_FS: i32 = 0x1003;

/// `AT_FDCWD`.
const AT_FDCWD: i32 = -100;

/// Linux access-mode mask.
const O_ACCMODE: i32 = 0x3;

/// `O_RDONLY`.
const O_RDONLY: i32 = 0;

/// `O_DIRECTORY`.
const O_DIRECTORY: i32 = 0x1_0000;

/// `O_NONBLOCK` (accepted and ignored for synthetic files).
const O_NONBLOCK: i32 = 0x800;

/// `O_CLOEXEC`.
const O_CLOEXEC: i32 = 0x8_0000;

/// `SEEK_SET`.
const SEEK_SET: i32 = 0;
/// `SEEK_CUR`.
const SEEK_CUR: i32 = 1;
/// `SEEK_END`.
const SEEK_END: i32 = 2;

/// A dispatcher with the B20 handlers registered.
pub fn dispatcher() -> Dispatcher {
    let mut dispatcher = Dispatcher::new(Arch::X86_64);
    for (name, handler) in [
        ("write", write as _),
        ("exit_group", exit_group as _),
        ("brk", brk as _),
        ("mmap", mmap as _),
        ("getpid", getpid as _),
        ("uname", uname as _),
        ("rt_sigprocmask", rt_sigprocmask as _),
        ("arch_prctl", arch_prctl as _),
        ("open", open as _),
        ("openat", openat as _),
        ("read", read as _),
        ("close", close as _),
        ("lseek", lseek as _),
        ("readlink", readlink as _),
        ("readlinkat", readlinkat as _),
        ("getdents64", getdents64 as _),
        ("stat", stat as _),
        ("lstat", lstat as _),
        ("fstat", fstat as _),
        ("newfstatat", newfstatat as _),
        ("statx", statx as _),
        ("access", access as _),
        ("fchmodat", fchmodat as _),
        ("fchownat", fchownat as _),
        ("utimensat", utimensat as _),
        ("truncate", truncate as _),
    ] {
        // Every name is in the x86-64 table; a failure here is a build-time typo, and the
        // dispatcher reports it as an unimplemented number instead of panicking.
        let _ = dispatcher.register(name, handler);
    }
    dispatcher
}

/// Run `f` on the concrete process context; a foreign context is `EINVAL`, never a panic.
fn with<R>(ctx: &mut dyn LinuxContext, f: impl FnOnce(&mut LinuxProcess) -> R) -> Result<R, i32> {
    match ctx.as_any_mut().downcast_mut::<LinuxProcess>() {
        Some(process) => Ok(f(process)),
        None => Err(EINVAL),
    }
}

/// `EINVAL` as the dispatcher's `isize`.
const EINVAL: i32 = Errno::EINVAL.0 as i32;
/// `EBADF` for relative paths without an open directory.
const EBADF: i32 = Errno::EBADF.0 as i32;
/// `ENOTDIR` for a relative path under a non-directory descriptor.
const ENOTDIR: i32 = Errno::ENOTDIR.0 as i32;
/// `ENOENT` for an empty path without `AT_EMPTY_PATH`.
const ENOENT: i32 = Errno::ENOENT.0 as i32;

fn einval() -> isize {
    -(EINVAL as isize)
}

fn write(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let length = args.size(2);
    let result = with(ctx, |process| {
        if fd != 1 && fd != 2 {
            return -(Errno::EBADF.0 as isize);
        }
        let bytes = process.user_buffer.as_slice();
        let take = bytes.len().min(length);
        process.output.extend_from_slice(&bytes[..take]);
        take as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn exit_group(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let code = args.int(0);
    let result = with(ctx, |process| {
        process.exit = Some(code);
        0
    });
    result.unwrap_or_else(|_| einval())
}

fn brk(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let requested = args.ptr(0) as u64;
    let result = with(ctx, |process| {
        if requested != 0 && requested >= process.brk_start && requested <= process.brk_limit {
            process.brk = requested;
        }
        process.brk as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn mmap(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let length = args.size(1) as u64;
    let protection = args.int(2);
    let flags = args.int(3);
    let result = with(ctx, |process| {
        if flags & MAP_ANONYMOUS == 0 {
            // File-backed mappings need a `file:` descriptor; the skeleton does not serve them.
            return -(Errno::ENOSYS.0 as isize);
        }
        if protection & PROT_EXEC != 0 {
            // The anonymous arena is read/write; executable anonymous memory is not served.
            return -(Errno::ENOSYS.0 as isize);
        }
        let Some(length) = round_up(length, PAGE) else {
            return -(Errno::EINVAL.0 as isize);
        };
        let Some(end) = process.mmap_next.checked_add(length) else {
            return -(Errno::ENOMEM.0 as isize);
        };
        if end > process.mmap_limit {
            return -(Errno::ENOMEM.0 as isize);
        }
        let address = process.mmap_next;
        process.mmap_next = end;
        address as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn getpid(ctx: &mut dyn LinuxContext, _args: &SyscallArgs) -> isize {
    with(ctx, |process| process.pid as isize).unwrap_or_else(|_| einval())
}

/// `utsname` is six 65-byte fields; the values are the RavenBSD identity, not copied from Linux.
fn uname(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(0) as u64;
    let result = with(ctx, |process| {
        let mut name = Vec::with_capacity(6 * 65);
        for field in [
            "Linux",
            "ravenbsd",
            "6.6.0-raven",
            "RavenBSD linuxd",
            "x86_64",
            "(none)",
        ] {
            let mut bytes = [0u8; 65];
            let take = field.len().min(64);
            bytes[..take].copy_from_slice(&field.as_bytes()[..take]);
            name.extend_from_slice(&bytes);
        }
        process.pending_write = Some((address, name));
        0
    });
    result.unwrap_or_else(|_| einval())
}

/// `rt_sigprocmask` is a no-op in the skeleton: there are no signals to block. The size argument is
/// still validated so a caller with a wrong `sigset_t` learns about it.
fn rt_sigprocmask(_ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    if args.size(3) != 8 {
        return -(Errno::EINVAL.0 as isize);
    }
    0
}

/// `arch_prctl(ARCH_SET_FS, ...)` needs a per-thread FS base the kernel does not have.
///
/// The kernel keeps no user-visible FS base and exposes no call to set one (no `PROC_SET_TLS`, no
/// `wrfsbase` enablement), so this handler answers `-ENOSYS` instead of pretending the base was set.
/// musl's `__init_tls` treats a failed thread-pointer setup as fatal (`a_crash()`), which is why the
/// static fixture stops here. See the B20 report for the exact missing primitive.
/// `arch_prctl`: the thread's FS base lives in the native thread (`THREAD_SET_TLS`, S42).
///
/// The Linux thread is blocked inside this trap, so the handler records the requested base and the
/// trap loop installs it with `THREAD_SET_TLS(child_thread, base)` **before** the reply reaches the
/// child. `ARCH_GET_FS` copies the base the table stored when the last set succeeded. A failing
/// native call replaces the success reply with `-errno`; success is never assumed.
fn arch_prctl(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let operation = args.int(0);
    let value = args.ptr(1) as u64;
    let result = with(ctx, |process| match operation {
        ARCH_SET_FS => {
            process.pending_tls = Some(value);
            0
        }
        ARCH_GET_FS => {
            process.pending_write = Some((value, process.fs_base.to_le_bytes().to_vec()));
            0
        }
        _ => -(Errno::EINVAL.0 as isize),
    });
    result.unwrap_or_else(|_| einval())
}

/// The NUL-terminated path the trap loop copied into `user_buffer`.
fn path_bytes(process: &LinuxProcess) -> Result<&[u8], i32> {
    let buffer = process.user_buffer.as_slice();
    let end = buffer.iter().position(|byte| *byte == 0).ok_or(EINVAL)?;
    if end == 0 {
        return Err(EINVAL);
    }
    buffer.get(..end).ok_or(EINVAL)
}

fn path_vec(process: &LinuxProcess) -> Result<Vec<u8>, i32> {
    path_bytes(process).map(|path| path.to_vec())
}

/// Resolve a possibly-relative path against `dirfd` (or the synthetic root for `AT_FDCWD`).
fn absolute_path(process: &LinuxProcess, dirfd: i32, path: &[u8]) -> Result<Vec<u8>, i32> {
    if path.first() == Some(&b'/') {
        return Ok(path.to_vec());
    }
    match dirfd {
        AT_FDCWD => {
            let mut out = Vec::with_capacity(path.len().saturating_add(1));
            out.push(b'/');
            out.extend_from_slice(path);
            Ok(out)
        }
        0..=2 => Err(EBADF),
        _ => {
            let Some(file) = process.file(dirfd) else {
                return Err(EBADF);
            };
            if !matches!(file.kind, FileKind::Directory(_)) {
                return Err(ENOTDIR);
            }
            let mut out = file.path.clone();
            if !out.ends_with(b"/") {
                out.push(b'/');
            }
            out.extend_from_slice(path);
            Ok(out)
        }
    }
}

/// The path argument of an `*at` metadata call, with the accepted flag set.
fn at_path(
    process: &LinuxProcess,
    dirfd: i32,
    flags: u32,
    allowed: u32,
    empty_ok: bool,
) -> Result<Vec<u8>, i32> {
    if flags & !allowed != 0 {
        return Err(EINVAL);
    }
    if process.user_buffer.first() == Some(&0) {
        if empty_ok && flags & AT_EMPTY_PATH != 0 {
            return Ok(Vec::new());
        }
        return Err(ENOENT);
    }
    let path = path_vec(process)?;
    absolute_path(process, dirfd, &path)
}

fn stat_into(process: &mut LinuxProcess, address: u64, path: &[u8], follow: bool) -> isize {
    match procfs::metadata(process, path, follow) {
        Ok(metadata) => {
            process.pending_write = Some((address, stat_layout::encode_stat(&metadata).to_vec()));
            0
        }
        Err(error) => -(error.0 as isize),
    }
}

fn statx_into(process: &mut LinuxProcess, address: u64, path: &[u8], follow: bool) -> isize {
    match procfs::metadata(process, path, follow) {
        Ok(metadata) => {
            process.pending_write = Some((address, stat_layout::encode_statx(&metadata).to_vec()));
            0
        }
        Err(error) => -(error.0 as isize),
    }
}

fn fstat_into(process: &mut LinuxProcess, fd: i32, address: u64, statx: bool) -> isize {
    match procfs::metadata_fd(process, fd) {
        Ok(metadata) => {
            let bytes = if statx {
                stat_layout::encode_statx(&metadata).to_vec()
            } else {
                stat_layout::encode_stat(&metadata).to_vec()
            };
            process.pending_write = Some((address, bytes));
            0
        }
        Err(error) => -(error.0 as isize),
    }
}

fn readonly_path(process: &LinuxProcess, path: &[u8], follow: bool) -> isize {
    match procfs::metadata(process, path, follow) {
        Ok(_) => -(Errno::EROFS.0 as isize),
        Err(error) => -(error.0 as isize),
    }
}

fn readonly_fd(process: &LinuxProcess, fd: i32) -> isize {
    match procfs::metadata_fd(process, fd) {
        Ok(_) => -(Errno::EROFS.0 as isize),
        Err(error) => -(error.0 as isize),
    }
}

/// `stat(path, buf)`: follow symlinks.
fn stat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(1) as u64;
    let result = with(ctx, |process| {
        let path = match path_vec(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let path = match absolute_path(process, AT_FDCWD, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        stat_into(process, address, &path, true)
    });
    result.unwrap_or_else(|_| einval())
}

/// `lstat(path, buf)`: do not follow the final symlink.
fn lstat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(1) as u64;
    let result = with(ctx, |process| {
        let path = match path_vec(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let path = match absolute_path(process, AT_FDCWD, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        stat_into(process, address, &path, false)
    });
    result.unwrap_or_else(|_| einval())
}

fn fstat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let address = args.ptr(1) as u64;
    with(ctx, |process| fstat_into(process, fd, address, false)).unwrap_or_else(|_| einval())
}

fn newfstatat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let address = args.ptr(2) as u64;
    let flags = args.int(3) as u32;
    let allowed = AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT | AT_EMPTY_PATH;
    let result = with(ctx, |process| {
        let path = match at_path(process, dirfd, flags, allowed, true) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        if path.is_empty() {
            return fstat_into(process, dirfd, address, false);
        }
        stat_into(process, address, &path, flags & AT_SYMLINK_NOFOLLOW == 0)
    });
    result.unwrap_or_else(|_| einval())
}

fn statx(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(2) as u32;
    let address = args.ptr(4) as u64;
    let allowed = AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT | AT_EMPTY_PATH | AT_STATX_SYNC_TYPE;
    let result = with(ctx, |process| {
        let path = match at_path(process, dirfd, flags, allowed, true) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        if path.is_empty() {
            return fstat_into(process, dirfd, address, true);
        }
        statx_into(process, address, &path, flags & AT_SYMLINK_NOFOLLOW == 0)
    });
    result.unwrap_or_else(|_| einval())
}

fn access(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let mode = args.int(1);
    let result = with(ctx, |process| {
        if mode < 0 || mode & !0o7 != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        let path = match path_vec(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let path = match absolute_path(process, AT_FDCWD, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        match procfs::metadata(process, &path, true) {
            Ok(metadata) if metadata.allows(mode) => 0,
            Ok(_) => -(Errno::EACCES.0 as isize),
            Err(error) => -(error.0 as isize),
        }
    });
    result.unwrap_or_else(|_| einval())
}

fn fchmodat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(3) as u32;
    let allowed = AT_SYMLINK_NOFOLLOW;
    let result = with(ctx, |process| {
        let path = match at_path(process, dirfd, flags, allowed, false) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        readonly_path(process, &path, flags & AT_SYMLINK_NOFOLLOW == 0)
    });
    result.unwrap_or_else(|_| einval())
}

fn fchownat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(4) as u32;
    let allowed = AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH;
    let result = with(ctx, |process| {
        let path = match at_path(process, dirfd, flags, allowed, true) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        if path.is_empty() {
            return readonly_fd(process, dirfd);
        }
        readonly_path(process, &path, flags & AT_SYMLINK_NOFOLLOW == 0)
    });
    result.unwrap_or_else(|_| einval())
}

fn utimensat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(3) as u32;
    let allowed = AT_SYMLINK_NOFOLLOW | AT_EMPTY_PATH;
    let result = with(ctx, |process| {
        let path = match at_path(process, dirfd, flags, allowed, true) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        if path.is_empty() {
            return readonly_fd(process, dirfd);
        }
        readonly_path(process, &path, flags & AT_SYMLINK_NOFOLLOW == 0)
    });
    result.unwrap_or_else(|_| einval())
}

fn truncate(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let length = args.long(1);
    if length < 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    let result = with(ctx, |process| {
        let path = match path_vec(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let absolute = match absolute_path(process, AT_FDCWD, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        match procfs::metadata(process, &absolute, true) {
            Ok(metadata) if metadata.is_dir() => -(Errno::EISDIR.0 as isize),
            Ok(_) => -(Errno::EROFS.0 as isize),
            Err(error) => -(error.0 as isize),
        }
    });
    result.unwrap_or_else(|_| einval())
}

/// Shared body of `open` and `openat`; `dirfd` is only meaningful for a relative path.
fn open_impl(process: &mut LinuxProcess, dirfd: i32, flags: i32) -> isize {
    if flags & O_ACCMODE != O_RDONLY {
        return -(Errno::EACCES.0 as isize);
    }
    let allowed = O_DIRECTORY | O_NONBLOCK | O_CLOEXEC;
    if flags & !allowed != 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    let path = match path_bytes(process) {
        Ok(path) => path.to_vec(),
        Err(_) => return -(Errno::EINVAL.0 as isize),
    };
    if dirfd != AT_FDCWD && !path.starts_with(b"/") {
        return -(Errno::EBADF.0 as isize);
    }
    match procfs::lookup(process, &path) {
        Ok(Resource::File(bytes)) => {
            if flags & O_DIRECTORY != 0 {
                return -(Errno::ENOTDIR.0 as isize);
            }
            process.open_file(path, FileKind::Regular(bytes)) as isize
        }
        Ok(Resource::Directory(entries)) => {
            process.open_file(path, FileKind::Directory(entries)) as isize
        }
        Ok(Resource::Symlink(_)) => -(Errno::ELOOP.0 as isize),
        Err(error) => -(error.0 as isize),
    }
}

fn open(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let flags = args.int(1);
    with(ctx, |process| open_impl(process, AT_FDCWD, flags)).unwrap_or_else(|_| einval())
}

fn openat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(2);
    with(ctx, |process| open_impl(process, dirfd, flags)).unwrap_or_else(|_| einval())
}

fn read(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let address = args.ptr(1) as u64;
    let count = args.size(2);
    let result = with(ctx, |process| {
        let (data, take) = {
            let Some(file) = process.file_mut(fd) else {
                return -(Errno::EBADF.0 as isize);
            };
            let bytes = match &file.kind {
                FileKind::Regular(bytes) => bytes,
                FileKind::Directory(_) => return -(Errno::EISDIR.0 as isize),
                FileKind::Symlink(_) => return -(Errno::EINVAL.0 as isize),
            };
            let start = usize::try_from(file.offset).unwrap_or(usize::MAX);
            let take = bytes.len().saturating_sub(start).min(count);
            let data = bytes
                .get(start..start.saturating_add(take))
                .unwrap_or(&[])
                .to_vec();
            file.offset = file.offset.saturating_add(take as u64);
            (data, take)
        };
        if take > 0 {
            process.pending_write = Some((address, data));
        }
        take as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn close(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let result = with(ctx, |process| {
        if process.close_file(fd) {
            0
        } else {
            -(Errno::EBADF.0 as isize)
        }
    });
    result.unwrap_or_else(|_| einval())
}

fn lseek(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let offset = args.ptr(1) as i64;
    let whence = args.int(2);
    let result = with(ctx, |process| {
        let Some(file) = process.file_mut(fd) else {
            return -(Errno::EBADF.0 as isize);
        };
        if matches!(file.kind, FileKind::Directory(_)) {
            return -(Errno::EISDIR.0 as isize);
        }
        let base = match whence {
            SEEK_SET => 0,
            SEEK_CUR => file.offset as i64,
            SEEK_END => file.size() as i64,
            _ => return -(Errno::EINVAL.0 as isize),
        };
        let Some(next) = base.checked_add(offset) else {
            return -(Errno::EINVAL.0 as isize);
        };
        if next < 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        file.offset = next as u64;
        next as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn read_link(process: &mut LinuxProcess, address: u64, size: usize) -> isize {
    if size == 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    let path = match path_bytes(process) {
        Ok(path) => path.to_vec(),
        Err(_) => return -(Errno::EINVAL.0 as isize),
    };
    match procfs::lookup(process, &path) {
        Ok(Resource::Symlink(target)) => {
            let take = target.len().min(size);
            let data = target.get(..take).unwrap_or(&[]).to_vec();
            process.pending_write = Some((address, data));
            take as isize
        }
        Ok(_) => -(Errno::EINVAL.0 as isize),
        Err(error) => -(error.0 as isize),
    }
}

fn readlink(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(1) as u64;
    let size = args.size(2);
    with(ctx, |process| read_link(process, address, size)).unwrap_or_else(|_| einval())
}

fn readlinkat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(2) as u64;
    let size = args.size(3);
    with(ctx, |process| read_link(process, address, size)).unwrap_or_else(|_| einval())
}

fn getdents64(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let address = args.ptr(1) as u64;
    let count = args.size(2);
    let result = with(ctx, |process| {
        let (data, total) = {
            let Some(file) = process.file_mut(fd) else {
                return -(Errno::EBADF.0 as isize);
            };
            let entries = match &file.kind {
                FileKind::Directory(entries) => entries,
                _ => return -(Errno::ENOTDIR.0 as isize),
            };
            let start = usize::try_from(file.offset).unwrap_or(usize::MAX);
            let Some(slice) = entries.get(start..) else {
                return 0;
            };
            let mut data = Vec::with_capacity(count.min(4096));
            let mut emitted = 0usize;
            for (index, entry) in slice.iter().enumerate() {
                let record_len = align8(19 + entry.name.len() + 1);
                if data.len().saturating_add(record_len) > count {
                    break;
                }
                let next = start.saturating_add(index).saturating_add(1) as i64;
                data.extend_from_slice(&entry.ino.to_le_bytes());
                data.extend_from_slice(&next.to_le_bytes());
                data.extend_from_slice(&(record_len as u16).to_le_bytes());
                data.push(entry.kind);
                data.extend_from_slice(&entry.name);
                data.push(0);
                data.resize(
                    data.len()
                        .saturating_add(record_len.saturating_sub(19 + entry.name.len() + 1)),
                    0,
                );
                emitted += 1;
            }
            if emitted == 0 && !slice.is_empty() {
                return -(Errno::EINVAL.0 as isize);
            }
            file.offset = file.offset.saturating_add(emitted as u64);
            let total = data.len();
            (data, total)
        };
        if total > 0 {
            process.pending_write = Some((address, data));
        }
        total as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn align8(value: usize) -> usize {
    value.saturating_add(7) & !7
}

fn round_up(value: u64, align: u64) -> Option<u64> {
    let remainder = value % align;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(align - remainder)
    }
}

#[cfg(test)]
#[path = "handlers_ut.rs"]
mod tests;
