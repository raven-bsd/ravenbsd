// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The syscall handlers of the B20 skeleton.
//!
//! Only the numbers the static `hello` needs are served: `write`, `exit_group`, `brk`, anonymous
//! `mmap`, `getpid`, `uname`, `rt_sigprocmask` and `arch_prctl`. Every other number reaches the
//! dispatcher's `-ENOSYS` path and is counted there.
//!
//! `brk` and anonymous `mmap` are served from two arenas the loader maps once at load time, so a
//! handler never needs a kernel call and the dispatcher stays a pure function table. The arenas and
//! their bounds are recorded in the process table (`brk_limit`, `mmap_limit`).

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_linux_abi::Arch;
use raven_linux_abi::flags::at_fd::{AT_EMPTY_PATH, AT_STATX_SYNC_TYPE, AT_SYMLINK_NOFOLLOW};
use raven_linux_abi::flags::clone as clone_flags;
use raven_linux_dispatch::{Dispatcher, LinuxContext, SyscallArgs};

use crate::meta;
use crate::procfs::{self, Resource};
use crate::table::{CloneRequest, FileKind, LinuxProcess};

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

/// `PR_SET_NAME`.
const PR_SET_NAME: i32 = 15;
/// `PR_GET_NAME`.
const PR_GET_NAME: i32 = 16;

/// `sizeof(struct robust_list_head)` on 64-bit Linux.
const ROBUST_LIST_HEAD_SIZE: usize = 24;

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

/// `O_NOFOLLOW` (accepted; the synthetic tree has no symlinks to follow).
const O_NOFOLLOW: i32 = 0x2_0000;

/// `O_LARGEFILE` (accepted; x86-64 is 64-bit already).
const O_LARGEFILE: i32 = 0x8000;

/// `SEEK_SET`.
const SEEK_SET: i32 = 0;
/// `SEEK_CUR`.
const SEEK_CUR: i32 = 1;
/// `SEEK_END`.
const SEEK_END: i32 = 2;

/// `R_OK`, `W_OK` and `X_OK` for `access`.
const R_OK: i32 = 4;
const W_OK: i32 = 2;
const X_OK: i32 = 1;

/// The Linux root is read-only; metadata changes report `EROFS`.
const EROFS: i32 = Errno::EROFS.0 as i32;

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
        ("fchmodat", fchmodat as _),
        ("fchownat", fchownat as _),
        ("utimensat", utimensat as _),
        ("truncate", truncate as _),
        ("access", access as _),
        ("gettid", gettid as _),
        ("set_tid_address", set_tid_address as _),
        ("set_robust_list", set_robust_list as _),
        ("prctl", prctl as _),
        ("clone", clone as _),
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
        // The main thread was created before the program called `set_tid_address`, so the kernel's
        // clear-on-exit word is unset. Queue the zero store and let the trap loop perform it before
        // the process is terminated.
        if process.clear_tid != 0 {
            process.pending_write = Some((process.clear_tid, 0u32.to_le_bytes().to_vec()));
        }
        process.exit = Some(code);
        0
    });
    result.unwrap_or_else(|_| einval())
}

/// `gettid` returns the native thread id captured when linuxd created the Linux thread.
fn gettid(ctx: &mut dyn LinuxContext, _args: &SyscallArgs) -> isize {
    with(ctx, |process| process.tid as isize).unwrap_or_else(|_| einval())
}

/// `set_tid_address(ptr)` records the clear-child-tid word and returns the caller's tid.
fn set_tid_address(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(0) as u64;
    with(ctx, |process| {
        process.clear_tid = address;
        process.tid as isize
    })
    .unwrap_or_else(|_| einval())
}

/// `set_robust_list(head, len)` records the robust-list head; only the 24-byte UAPI head is valid.
fn set_robust_list(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let head = args.ptr(0) as u64;
    let length = args.size(1);
    with(ctx, |process| {
        if length != ROBUST_LIST_HEAD_SIZE {
            return -(Errno::EINVAL.0 as isize);
        }
        process.robust_list = head;
        process.robust_list_len = length;
        0
    })
    .unwrap_or_else(|_| einval())
}

/// The name attributes a static Linux program sets at startup.
///
/// Only `PR_SET_NAME`/`PR_GET_NAME` are served in B21b; the other B21 attributes are not needed by
/// `clone` and remain `EINVAL` until the process/thread batch lands.
fn prctl(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let option = args.int(0);
    let result = with(ctx, |process| match option {
        PR_SET_NAME => {
            let name = process.user_buffer.clone();
            process.set_name(&name);
            0
        }
        PR_GET_NAME => {
            process.pending_write = Some((args.ptr(1) as u64, process.name_bytes().to_vec()));
            0
        }
        _ => -(Errno::EINVAL.0 as isize),
    });
    result.unwrap_or_else(|_| einval())
}

/// `clone` validates the Linux arguments and queues the request for the trap loop.
///
/// The handler cannot create a native thread or process, so it records the B21a frame and the six
/// arguments in [`CloneRequest`]; `main.rs` performs the mapping and creation, then replaces the
/// placeholder reply with the child pid/tid or the kernel's errno. A missing frame means the trap
/// did not come from the B21a x86-64 path, so the honest answer is `-ENOSYS`.
fn clone(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let result = with(ctx, |process| {
        let Some(frame) = process.frame else {
            return -(Errno::ENOSYS.0 as isize);
        };
        let request = CloneRequest {
            flags: args.ptr(0) as u64,
            child_stack: args.ptr(1) as u64,
            ptid: args.ptr(2) as u64,
            ctid: args.ptr(3) as u64,
            tls: args.ptr(4) as u64,
            frame,
        };
        match crate::clone::classify(&request) {
            Ok(_) => {}
            Err(error) => return -(error.0 as isize),
        }
        if request.flags & clone_flags::CLONE_PARENT_SETTID != 0
            && !writable_word(process, request.ptid)
        {
            return -(Errno::EFAULT.0 as isize);
        }
        if request.flags & clone_flags::CLONE_CHILD_SETTID != 0
            && !writable_word(process, request.ctid)
        {
            return -(Errno::EFAULT.0 as isize);
        }
        process.pending_clone = Some(request);
        0
    });
    result.unwrap_or_else(|_| einval())
}

/// Whether a four-byte pointer is zero or lies inside a known writable region.
fn writable_word(process: &LinuxProcess, address: u64) -> bool {
    address == 0 || process.region_for(address, 4).is_some()
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

/// The NUL-terminated path, including an empty path (`AT_EMPTY_PATH` callers check it themselves).
fn raw_path(process: &LinuxProcess) -> Result<Vec<u8>, i32> {
    let buffer = process.user_buffer.as_slice();
    let end = buffer.iter().position(|byte| *byte == 0).ok_or(EINVAL)?;
    Ok(buffer.get(..end).ok_or(EINVAL)?.to_vec())
}

/// Normalize an absolute path: drop empty and `.` components, apply `..` without leaving `/`.
fn normalize_abs(path: &[u8]) -> Option<Vec<u8>> {
    if path.first() != Some(&b'/') {
        return None;
    }
    let mut components: Vec<&[u8]> = Vec::new();
    for component in path.split(|byte| *byte == b'/') {
        match component {
            b"" | b"." => {}
            b".." => {
                components.pop();
            }
            other => components.push(other),
        }
    }
    let mut normalized = Vec::new();
    for component in components {
        normalized.push(b'/');
        normalized.extend_from_slice(component);
        if normalized.len() > 4096 {
            return None;
        }
    }
    if normalized.is_empty() {
        normalized.push(b'/');
    }
    Some(normalized)
}

/// Resolve a path against `dirfd`: an absolute path is normalized, `AT_FDCWD` uses the process cwd,
/// and a directory descriptor uses its opened path.
fn resolve_path(process: &LinuxProcess, dirfd: i32, path: &[u8]) -> Result<Vec<u8>, i32> {
    if path.is_empty() {
        return Err(Errno::ENOENT.0 as i32);
    }
    if path.first() == Some(&b'/') {
        return normalize_abs(path).ok_or(EINVAL);
    }
    let base = if dirfd == AT_FDCWD {
        process.cwd().to_vec()
    } else {
        match process.file(dirfd) {
            Some(file) if matches!(file.kind, FileKind::Directory(_)) => file.path.clone(),
            Some(_) => return Err(Errno::ENOTDIR.0 as i32),
            None => return Err(Errno::EBADF.0 as i32),
        }
    };
    let mut joined = base;
    if !joined.ends_with(b"/") {
        joined.push(b'/');
    }
    joined.extend_from_slice(path);
    normalize_abs(&joined).ok_or(EINVAL)
}

/// Shared body of `open` and `openat`; `dirfd` is only meaningful for a relative path.
fn open_impl(process: &mut LinuxProcess, dirfd: i32, flags: i32) -> isize {
    if flags & O_ACCMODE != O_RDONLY {
        return -(Errno::EACCES.0 as isize);
    }
    let allowed = O_DIRECTORY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW | O_LARGEFILE;
    if flags & !allowed != 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    let path = match raw_path(process) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    let path = match resolve_path(process, dirfd, &path) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
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

fn read_link(process: &mut LinuxProcess, dirfd: i32, address: u64, size: usize) -> isize {
    if size == 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    let path = match raw_path(process) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    let path = match resolve_path(process, dirfd, &path) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
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
    with(ctx, |process| read_link(process, AT_FDCWD, address, size)).unwrap_or_else(|_| einval())
}

fn readlinkat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let address = args.ptr(2) as u64;
    let size = args.size(3);
    with(ctx, |process| read_link(process, dirfd, address, size)).unwrap_or_else(|_| einval())
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

fn stat_common(process: &mut LinuxProcess, dirfd: i32, address: u64) -> isize {
    let path = match raw_path(process) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    let path = match resolve_path(process, dirfd, &path) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    let meta = match meta::path_meta(process, &path) {
        Ok(meta) => meta,
        Err(error) => return -(error.0 as isize),
    };
    process.pending_write = Some((address, meta::stat_bytes(&meta)));
    0
}

fn stat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(1) as u64;
    with(ctx, |process| stat_common(process, AT_FDCWD, address)).unwrap_or_else(|_| einval())
}

/// `lstat` shares the layout; the staged tree has no symlinks and the synthetic symlinks are
/// reported as symlinks by both calls (documented in `docs/linux-syscalls.md`).
fn lstat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(1) as u64;
    with(ctx, |process| stat_common(process, AT_FDCWD, address)).unwrap_or_else(|_| einval())
}

fn fstat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let address = args.ptr(1) as u64;
    with(ctx, |process| {
        let meta = match meta::file_meta(process, fd) {
            Ok(meta) => meta,
            Err(error) => return -(error.0 as isize),
        };
        process.pending_write = Some((address, meta::stat_bytes(&meta)));
        0
    })
    .unwrap_or_else(|_| einval())
}

fn newfstatat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let address = args.ptr(2) as u64;
    let flags = args.int(3);
    with(ctx, |process| {
        if flags & !(AT_SYMLINK_NOFOLLOW as i32 | AT_EMPTY_PATH as i32) != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        let path = match raw_path(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        if path.is_empty() {
            if flags & AT_EMPTY_PATH as i32 == 0 {
                return -(Errno::ENOENT.0 as isize);
            }
            let meta = match meta::file_meta(process, dirfd) {
                Ok(meta) => meta,
                Err(error) => return -(error.0 as isize),
            };
            process.pending_write = Some((address, meta::stat_bytes(&meta)));
            return 0;
        }
        let path = match resolve_path(process, dirfd, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let meta = match meta::path_meta(process, &path) {
            Ok(meta) => meta,
            Err(error) => return -(error.0 as isize),
        };
        process.pending_write = Some((address, meta::stat_bytes(&meta)));
        0
    })
    .unwrap_or_else(|_| einval())
}

fn statx(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(2);
    let mask = args.ptr(3) as u32;
    let address = args.ptr(4) as u64;
    with(ctx, |process| {
        let allowed = AT_SYMLINK_NOFOLLOW as i32 | AT_EMPTY_PATH as i32 | AT_STATX_SYNC_TYPE as i32;
        if flags & !allowed != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        let path = match raw_path(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let meta = if path.is_empty() {
            if flags & AT_EMPTY_PATH as i32 == 0 {
                return -(Errno::ENOENT.0 as isize);
            }
            match meta::file_meta(process, dirfd) {
                Ok(meta) => meta,
                Err(error) => return -(error.0 as isize),
            }
        } else {
            let path = match resolve_path(process, dirfd, &path) {
                Ok(path) => path,
                Err(error) => return -(error as isize),
            };
            match meta::path_meta(process, &path) {
                Ok(meta) => meta,
                Err(error) => return -(error.0 as isize),
            }
        };
        process.pending_write = Some((address, meta::statx_bytes(&meta, mask)));
        0
    })
    .unwrap_or_else(|_| einval())
}

fn fchmodat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let mode = args.int(2);
    let flags = args.int(3);
    with(ctx, |process| {
        if mode & !0o7777 != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        if flags & !(AT_SYMLINK_NOFOLLOW as i32) != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        read_only_change(process, dirfd)
    })
    .unwrap_or_else(|_| einval())
}

fn fchownat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(4);
    with(ctx, |process| {
        if flags & !(AT_SYMLINK_NOFOLLOW as i32 | AT_EMPTY_PATH as i32) != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        read_only_change(process, dirfd)
    })
    .unwrap_or_else(|_| einval())
}

fn utimensat(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let dirfd = args.int(0);
    let flags = args.int(3);
    with(ctx, |process| {
        if flags & !(AT_SYMLINK_NOFOLLOW as i32) != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        read_only_change(process, dirfd)
    })
    .unwrap_or_else(|_| einval())
}

/// Resolve the path of a metadata-changing call and report the read-only root.
fn read_only_change(process: &mut LinuxProcess, dirfd: i32) -> isize {
    let path = match raw_path(process) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    if path.is_empty() {
        return -(Errno::ENOENT.0 as isize);
    }
    let path = match resolve_path(process, dirfd, &path) {
        Ok(path) => path,
        Err(error) => return -(error as isize),
    };
    if let Err(error) = meta::path_meta(process, &path) {
        return -(error.0 as isize);
    }
    -(EROFS as isize)
}

fn truncate(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let length = args.long(1);
    with(ctx, |process| {
        if length < 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        read_only_change(process, AT_FDCWD)
    })
    .unwrap_or_else(|_| einval())
}

fn access(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let mode = args.int(1);
    with(ctx, |process| {
        if mode & !7 != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        let path = match raw_path(process) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let path = match resolve_path(process, AT_FDCWD, &path) {
            Ok(path) => path,
            Err(error) => return -(error as isize),
        };
        let meta = match meta::path_meta(process, &path) {
            Ok(meta) => meta,
            Err(error) => return -(error.0 as isize),
        };
        if mode & W_OK != 0 {
            return -(EROFS as isize);
        }
        let mut needed = 0;
        if mode & R_OK != 0 {
            needed |= 0o400;
        }
        if mode & X_OK != 0 {
            needed |= 0o100;
        }
        if meta.mode & needed == needed {
            0
        } else {
            -(Errno::EACCES.0 as isize)
        }
    })
    .unwrap_or_else(|_| einval())
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
