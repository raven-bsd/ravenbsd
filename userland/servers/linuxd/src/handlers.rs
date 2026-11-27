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
use raven_linux_abi::flags::clone as clone_flags;
use raven_linux_dispatch::{Dispatcher, LinuxContext, SyscallArgs};

use crate::linux_error;

use crate::files::{
    self, AT_FDCWD, F_DUPFD, F_DUPFD_CLOEXEC, F_GETFD, F_GETFL, F_SETFD, F_SETFL, FD_CLOEXEC,
    FileOp, O_ACCMODE, O_APPEND, O_CLOEXEC, O_DIRECTORY, O_NOFOLLOW, O_NONBLOCK, O_RDONLY, O_TRUNC,
    SEEK_CUR, SEEK_END, SEEK_SET, bounded_len,
};
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

/// The Linux root is read-only for the synthetic surface.
const EROFS: i32 = raven_linux_abi::errno::EROFS as i32;

/// `PR_SET_NAME`.
const PR_SET_NAME: i32 = 15;
/// `PR_GET_NAME`.
const PR_GET_NAME: i32 = 16;

/// `sizeof(struct robust_list_head)` on 64-bit Linux.
const ROBUST_LIST_HEAD_SIZE: usize = 24;

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
        ("pread64", pread64 as _),
        ("pwrite64", pwrite64 as _),
        ("fcntl", fcntl as _),
        ("dup3", dup3 as _),
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
const EINVAL: i32 = raven_linux_abi::errno::EINVAL as i32;

fn einval() -> isize {
    -(EINVAL as isize)
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
            return linux_error(Errno::EINVAL);
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
        _ => linux_error(Errno::EINVAL),
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
            return linux_error(Errno::ENOSYS);
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
            Err(error) => return linux_error(error),
        }
        if request.flags & clone_flags::CLONE_PARENT_SETTID != 0
            && !writable_word(process, request.ptid)
        {
            return linux_error(Errno::EFAULT);
        }
        if request.flags & clone_flags::CLONE_CHILD_SETTID != 0
            && !writable_word(process, request.ctid)
        {
            return linux_error(Errno::EFAULT);
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
            return linux_error(Errno::ENOSYS);
        }
        if protection & PROT_EXEC != 0 {
            // The anonymous arena is read/write; executable anonymous memory is not served.
            return linux_error(Errno::ENOSYS);
        }
        let Some(length) = round_up(length, PAGE) else {
            return linux_error(Errno::EINVAL);
        };
        let Some(end) = process.mmap_next.checked_add(length) else {
            return linux_error(Errno::ENOMEM);
        };
        if end > process.mmap_limit {
            return linux_error(Errno::ENOMEM);
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
        return linux_error(Errno::EINVAL);
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
        _ => linux_error(Errno::EINVAL),
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

/// Shared body of `open` and `openat`.
///
/// Synthetic `/proc`/`/sys` paths keep the in-memory content and stay read-only. Every other path
/// resolves to a `file:`/`ufs:` scheme path and is queued for the trap loop, which obtains the
/// scheme handle and installs the descriptor.
fn open_impl(process: &mut LinuxProcess, dirfd: i32, flags: i32) -> isize {
    let path = match path_bytes(process) {
        Ok(path) => path.to_vec(),
        Err(_) => return linux_error(Errno::EINVAL),
    };
    let resolved = match files::resolve_path(process, dirfd, &path) {
        Ok(resolved) => resolved,
        Err(error) => return linux_error(error),
    };
    if files::is_synthetic(&resolved) {
        if flags & O_ACCMODE != O_RDONLY
            || flags & (O_APPEND | O_TRUNC | crate::files::O_CREAT) != 0
        {
            return -(EROFS as isize);
        }
        let allowed = O_DIRECTORY
            | O_NONBLOCK
            | O_CLOEXEC
            | O_NOFOLLOW
            | crate::files::O_LARGEFILE
            | crate::files::O_NOCTTY;
        if flags & !allowed != 0 {
            return linux_error(Errno::EINVAL);
        }
        return match procfs::lookup(process, &resolved) {
            Ok(Resource::File(bytes)) => {
                if flags & O_DIRECTORY != 0 {
                    return linux_error(Errno::ENOTDIR);
                }
                process.open_with_flags(resolved, FileKind::Regular(bytes), flags) as isize
            }
            Ok(Resource::Directory(entries)) => {
                process.open_with_flags(resolved, FileKind::Directory(entries), flags) as isize
            }
            Ok(Resource::Symlink(_)) => linux_error(Errno::ELOOP),
            Err(error) => linux_error(error),
        };
    }
    let scheme = files::scheme_path(&resolved);
    if scheme.len() > 256 {
        return linux_error(Errno::ENAMETOOLONG);
    }
    process.pending_file = Some(FileOp::Open {
        scheme,
        path: resolved,
        flags,
    });
    0
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
    let count = bounded_len(args.size(2));
    let result = with(ctx, |process| {
        let remote = process.file(fd).is_some_and(|file| file.remote().is_some());
        if remote {
            process.pending_file = Some(FileOp::Read {
                fd,
                address,
                len: count,
            });
            return count as isize;
        }
        let (data, take) = {
            let Some(file) = process.file_mut(fd) else {
                return linux_error(Errno::EBADF);
            };
            let bytes = match &file.kind {
                FileKind::Regular(bytes) => bytes,
                FileKind::Directory(_) => return linux_error(Errno::EISDIR),
                FileKind::Symlink(_) => return linux_error(Errno::EINVAL),
                FileKind::Remote(_) => return linux_error(Errno::EBADF),
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

fn write(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let length = args.size(2);
    let result = with(ctx, |process| {
        let take = process.user_buffer.len().min(length);
        if process.file(fd).is_some_and(|file| file.remote().is_some()) {
            if take == 0 {
                return 0;
            }
            let data = process.user_buffer.get(..take).unwrap_or(&[]).to_vec();
            process.pending_file = Some(FileOp::Write { fd, data });
            return take as isize;
        }
        if fd != 1 && fd != 2 {
            return linux_error(Errno::EBADF);
        }
        let bytes = process.user_buffer.as_slice();
        process.output.extend_from_slice(&bytes[..take]);
        take as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn pread64(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let address = args.ptr(1) as u64;
    let count = bounded_len(args.size(2));
    let offset = args.ptr(3) as u64;
    let result = with(ctx, |process| {
        let remote = process.file(fd).is_some_and(|file| file.remote().is_some());
        if remote {
            process.pending_file = Some(FileOp::Pread {
                fd,
                address,
                len: count,
                offset,
            });
            return count as isize;
        }
        let (data, take) = {
            let Some(file) = process.file_mut(fd) else {
                return linux_error(Errno::EBADF);
            };
            let bytes = match &file.kind {
                FileKind::Regular(bytes) => bytes,
                FileKind::Directory(_) => return linux_error(Errno::EISDIR),
                FileKind::Symlink(_) => return linux_error(Errno::EINVAL),
                FileKind::Remote(_) => return linux_error(Errno::EBADF),
            };
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            let take = bytes.len().saturating_sub(start).min(count);
            let data = bytes
                .get(start..start.saturating_add(take))
                .unwrap_or(&[])
                .to_vec();
            (data, take)
        };
        if take > 0 {
            process.pending_write = Some((address, data));
        }
        take as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn pwrite64(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let length = args.size(2);
    let offset = args.ptr(3) as u64;
    let result = with(ctx, |process| {
        let take = process.user_buffer.len().min(length);
        if process.file(fd).is_some_and(|file| file.remote().is_some()) {
            if take == 0 {
                return 0;
            }
            let data = process.user_buffer.get(..take).unwrap_or(&[]).to_vec();
            process.pending_file = Some(FileOp::Pwrite { fd, data, offset });
            return take as isize;
        }
        -(EROFS as isize)
    });
    result.unwrap_or_else(|_| einval())
}

fn close(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let result = with(ctx, |process| {
        if process.file(fd).is_some_and(|file| file.remote().is_some()) {
            process.pending_file = Some(FileOp::Close { fd });
            return 0;
        }
        if process.close_file(fd) {
            0
        } else {
            linux_error(Errno::EBADF)
        }
    });
    result.unwrap_or_else(|_| einval())
}

fn lseek(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let offset = args.ptr(1) as i64;
    let whence = args.int(2);
    let result = with(ctx, |process| {
        if !matches!(whence, SEEK_SET | SEEK_CUR | SEEK_END) {
            return linux_error(Errno::EINVAL);
        }
        if process.file(fd).is_some_and(|file| file.remote().is_some()) {
            process.pending_file = Some(FileOp::Seek { fd, offset, whence });
            return 0;
        }
        let Some(file) = process.file_mut(fd) else {
            return linux_error(Errno::EBADF);
        };
        if matches!(file.kind, FileKind::Directory(_)) {
            return linux_error(Errno::EISDIR);
        }
        let base = match whence {
            SEEK_SET => 0,
            SEEK_CUR => file.offset as i64,
            SEEK_END => file.size() as i64,
            _ => return linux_error(Errno::EINVAL),
        };
        let Some(next) = base.checked_add(offset) else {
            return linux_error(Errno::EINVAL);
        };
        if next < 0 {
            return linux_error(Errno::EINVAL);
        }
        file.offset = next as u64;
        next as isize
    });
    result.unwrap_or_else(|_| einval())
}

fn fcntl(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let fd = args.fd(0);
    let command = args.int(1);
    let argument = args.ptr(2) as u64;
    let result = with(ctx, |process| {
        let Some(file) = process.file(fd) else {
            return linux_error(Errno::EBADF);
        };
        match command {
            F_DUPFD | F_DUPFD_CLOEXEC => {
                let Ok(min) = i32::try_from(argument) else {
                    return linux_error(Errno::EINVAL);
                };
                if min < 0 {
                    return linux_error(Errno::EINVAL);
                }
                process.pending_file = Some(FileOp::FcntlDup {
                    fd,
                    min,
                    cloexec: command == F_DUPFD_CLOEXEC,
                });
                0
            }
            F_GETFD => file.fd_flags as isize,
            F_SETFD => {
                let flags = argument as i32 & FD_CLOEXEC;
                if let Some(file) = process.file_mut(fd) {
                    file.fd_flags = flags;
                }
                0
            }
            F_GETFL => file.flags as isize,
            F_SETFL => {
                let mask = O_APPEND | O_NONBLOCK;
                if let Some(file) = process.file_mut(fd) {
                    file.flags = (file.flags & !mask) | (argument as i32 & mask);
                }
                0
            }
            _ => linux_error(Errno::EINVAL),
        }
    });
    result.unwrap_or_else(|_| einval())
}

fn dup3(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let oldfd = args.fd(0);
    let newfd = args.fd(1);
    let flags = args.int(2);
    let result = with(ctx, |process| {
        if oldfd == newfd || flags & !O_CLOEXEC != 0 || newfd < 0 {
            return linux_error(Errno::EINVAL);
        }
        if process.file(oldfd).is_none() {
            return linux_error(Errno::EBADF);
        }
        process.pending_file = Some(FileOp::Dup3 {
            oldfd,
            newfd,
            cloexec: flags & O_CLOEXEC != 0,
        });
        0
    });
    result.unwrap_or_else(|_| einval())
}

fn read_link(process: &mut LinuxProcess, address: u64, size: usize) -> isize {
    if size == 0 {
        return linux_error(Errno::EINVAL);
    }
    let path = match path_bytes(process) {
        Ok(path) => path.to_vec(),
        Err(_) => return linux_error(Errno::EINVAL),
    };
    match procfs::lookup(process, &path) {
        Ok(Resource::Symlink(target)) => {
            let take = target.len().min(size);
            let data = target.get(..take).unwrap_or(&[]).to_vec();
            process.pending_write = Some((address, data));
            take as isize
        }
        Ok(_) => linux_error(Errno::EINVAL),
        Err(error) => linux_error(error),
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
                return linux_error(Errno::EBADF);
            };
            let entries = match &file.kind {
                FileKind::Directory(entries) => entries,
                _ => return linux_error(Errno::ENOTDIR),
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
                return linux_error(Errno::EINVAL);
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
