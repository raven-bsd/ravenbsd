// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The syscall handlers of the Linux personality.
//!
//! B20 served the numbers the static `hello` needs, B30 added the synthetic file surface and B21
//! adds the process and thread batch: `clone`, `execve`, `wait4`, `exit_group`, `gettid`,
//! `set_tid_address`, `set_robust_list`, `prctl` and `arch_prctl`. Every other number reaches the
//! dispatcher's `-ENOSYS` path and is counted there.
//!
//! `brk` and anonymous `mmap` are served from two arenas the loader maps once at load time, so a
//! handler never needs a kernel call and the dispatcher stays a pure function table. The arenas and
//! their bounds are recorded in the process table (`brk_limit`, `mmap_limit`).
//!
//! Kernel work is queued on the context: `pending_tls` installs a thread FS base, `pending_exec`
//! replaces the image, `pending_write` copies bytes out and `exit` ends the process. The trap loop
//! in `main.rs` performs each one before it replies.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_linux_abi::Arch;
use raven_linux_abi::flags::clone as clone_flags;
use raven_linux_dispatch::{Dispatcher, LinuxContext, SyscallArgs};

use crate::procfs::{self, Resource};
use crate::table::{ExecRequest, FileKind, LinuxProcess};

/// `MAP_ANONYMOUS` from the Linux UAPI (the dispatcher's `raven-linux-abi` crate owns the number).
const MAP_ANONYMOUS: i32 = 0x20;

/// `PROT_EXEC` from the Linux UAPI.
const PROT_EXEC: i32 = 0x4;

/// Page size of the guest's `mmap`.
const PAGE: u64 = 4096;

/// `ARCH_SET_GS` of x86-64.
const ARCH_SET_GS: i32 = 0x1001;

/// `ARCH_SET_FS` of x86-64.
const ARCH_SET_FS: i32 = 0x1002;

/// `ARCH_GET_FS` of x86-64.
const ARCH_GET_FS: i32 = 0x1003;

/// `ARCH_GET_GS` of x86-64.
const ARCH_GET_GS: i32 = 0x1004;

/// `PR_SET_PDEATHSIG` (linux-raw-sys `prctl`).
const PR_SET_PDEATHSIG: i32 = 1;
/// `PR_GET_PDEATHSIG`.
const PR_GET_PDEATHSIG: i32 = 2;
/// `PR_GET_DUMPABLE`.
const PR_GET_DUMPABLE: i32 = 3;
/// `PR_SET_DUMPABLE`.
const PR_SET_DUMPABLE: i32 = 4;
/// `PR_SET_NAME`.
pub(crate) const PR_SET_NAME: i32 = 15;
/// `PR_GET_NAME`.
const PR_GET_NAME: i32 = 16;
/// `PR_SET_CHILD_SUBREAPER`.
const PR_SET_CHILD_SUBREAPER: i32 = 36;
/// `PR_GET_CHILD_SUBREAPER`.
const PR_GET_CHILD_SUBREAPER: i32 = 37;
/// `PR_SET_NO_NEW_PRIVS`.
const PR_SET_NO_NEW_PRIVS: i32 = 38;
/// `PR_GET_NO_NEW_PRIVS`.
const PR_GET_NO_NEW_PRIVS: i32 = 39;
/// `PR_SET_THP_DISABLE`.
const PR_SET_THP_DISABLE: i32 = 41;
/// `PR_GET_THP_DISABLE`.
const PR_GET_THP_DISABLE: i32 = 42;

/// `WNOHANG`.
const WNOHANG: i32 = 1;

/// The `robust_list_head` size the kernel accepts (`set_robust_list`). The layout is
/// `{ next: *mut robust_list, futex_offset: long, list_op_pending: *mut robust_list }`.
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
        ("clone", clone as _),
        ("execve", execve as _),
        ("wait4", wait4 as _),
        ("gettid", gettid as _),
        ("set_tid_address", set_tid_address as _),
        ("set_robust_list", set_robust_list as _),
        ("prctl", prctl as _),
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
        // The kernel's thread-exit path clears the word it was created with, but this thread was
        // created before the program called `set_tid_address`. Queue the zero store here; the trap
        // loop performs it before it terminates the process. There is no futex wake: the server
        // cannot run FUTEX against another process's address space (documented in the B21 report).
        if process.clear_tid != 0 {
            process.pending_write = Some((process.clear_tid, 0u32.to_le_bytes().to_vec()));
        }
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

/// `arch_prctl(ARCH_SET_FS, ...)` installs the thread's FS base through the native
/// `THREAD_SET_TLS` (S42). The Linux thread is blocked inside this trap, so the handler records
/// the requested base and the trap loop installs it **before** the reply reaches the child.
/// `ARCH_GET_FS` copies the base the table stored when the last set succeeded. A failing native
/// call replaces the success reply with `-errno`; success is never assumed.
///
/// `ARCH_SET_GS`/`ARCH_GET_GS` are `-ENOSYS`: the kernel exposes a per-thread FS/TPIDR base but no
/// user GS base, and storing a value the CPU does not use would be a fake success.
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
        ARCH_SET_GS | ARCH_GET_GS => -(Errno::ENOSYS.0 as isize),
        _ => -(Errno::EINVAL.0 as isize),
    });
    result.unwrap_or_else(|_| einval())
}

/// `gettid` returns the kernel thread id captured when `linuxd` created the thread
/// (`THREAD_CREATE_EX.tid_out`). Linux requires it to be the thread id used by futexes and the
/// robust list, not a server-local counter.
fn gettid(ctx: &mut dyn LinuxContext, _args: &SyscallArgs) -> isize {
    with(ctx, |process| process.tid as isize).unwrap_or_else(|_| einval())
}

/// `set_tid_address(ptr)` records the clear-child-tid word and returns the caller's tid.
///
/// The kernel fixes the word of a thread at `THREAD_CREATE`; it has no call to change it later, so
/// the server records the address and clears it itself at `exit_group`. A futex waiter on the word
/// is not woken; see [`exit_group`].
fn set_tid_address(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let address = args.ptr(0) as u64;
    with(ctx, |process| {
        process.clear_tid = address;
        process.tid as isize
    })
    .unwrap_or_else(|_| einval())
}

/// `set_robust_list(head, len)` records the Linux robust-list head. The size is the UAPI
/// `sizeof(struct robust_list_head)` (24 on 64-bit).
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

/// `prctl` serves the process attributes a static Linux program sets at startup: the task name,
/// the parent-death signal, dumpability, no-new-privileges, transparent-hugepage opt-out and the
/// child-subreaper flag.
///
/// `PR_SET_NAME` is the only option whose argument is a pointer, so the trap loop prefetches its
/// 16-byte window (see `trap::copy_in`). Other options are scalar or copy out through
/// `pending_write`.
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
        PR_SET_PDEATHSIG => {
            let signal = args.ptr(1);
            if signal > 64 {
                return -(Errno::EINVAL.0 as isize);
            }
            process.pdeathsig = signal as u32;
            0
        }
        PR_GET_PDEATHSIG => {
            process.pending_write =
                Some((args.ptr(1) as u64, process.pdeathsig.to_le_bytes().to_vec()));
            0
        }
        PR_SET_DUMPABLE => match args.ptr(1) {
            0 => {
                process.dumpable = false;
                0
            }
            1 | 2 => {
                process.dumpable = true;
                0
            }
            _ => -(Errno::EINVAL.0 as isize),
        },
        PR_GET_DUMPABLE => isize::from(process.dumpable as u8),
        PR_SET_NO_NEW_PRIVS => {
            if args.ptr(1) != 1 || args.ptr(2) != 0 || args.ptr(3) != 0 || args.ptr(4) != 0 {
                return -(Errno::EINVAL.0 as isize);
            }
            process.no_new_privs = true;
            0
        }
        PR_GET_NO_NEW_PRIVS => isize::from(process.no_new_privs as u8),
        PR_SET_THP_DISABLE => {
            if args.ptr(1) > 1 || args.ptr(2) != 0 || args.ptr(3) != 0 || args.ptr(4) != 0 {
                return -(Errno::EINVAL.0 as isize);
            }
            process.thp_disable = args.ptr(1) != 0;
            0
        }
        PR_GET_THP_DISABLE => isize::from(process.thp_disable as u8),
        PR_SET_CHILD_SUBREAPER => {
            if args.ptr(1) > 1 {
                return -(Errno::EINVAL.0 as isize);
            }
            process.subreaper = args.ptr(1) != 0;
            0
        }
        PR_GET_CHILD_SUBREAPER => {
            process.pending_write = Some((
                args.ptr(1) as u64,
                (process.subreaper as u32).to_le_bytes().to_vec(),
            ));
            0
        }
        _ => -(Errno::EINVAL.0 as isize),
    });
    result.unwrap_or_else(|_| einval())
}

/// `clone` is the only call in this batch the native ABI cannot serve.
///
/// Linux's child resumes at the instruction after the `syscall` with the parent's whole register
/// set (only `rax` zeroed and `rsp` replaced). The personality trap carries the number and six
/// arguments, not the user frame, and `THREAD_CREATE` starts a thread at a fresh entry with every
/// register zero except the argument. `PROC_FORK` (S50b) gives a COW address space but no thread
/// context either. A child started that way would not return from `clone`, so the handler rejects
/// valid requests with `-ENOSYS` instead of returning a tid for a process that cannot run. Unknown
/// flag bits are `-EINVAL`, as on Linux.
fn clone(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let flags = args.ptr(0) as u64;
    let stack = args.ptr(1);
    let result = with(ctx, |_process| {
        // The low byte is the signal delivered to the parent, not a clone flag.
        if (flags & 0xff) > 64 || (flags & !0xff & !KNOWN_CLONE_FLAGS) != 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        if flags & clone_flags::CLONE_THREAD != 0 && stack == 0 {
            return -(Errno::EINVAL.0 as isize);
        }
        if stack != 0 && !stack.is_multiple_of(16) {
            return -(Errno::EINVAL.0 as isize);
        }
        -(Errno::ENOSYS.0 as isize)
    });
    result.unwrap_or_else(|_| einval())
}

/// Every clone flag the kernel UAPI defines; a bit outside this set is an unknown request.
const KNOWN_CLONE_FLAGS: u64 = clone_flags::CLONE_CHILD_CLEARTID
    | clone_flags::CLONE_CHILD_SETTID
    | clone_flags::CLONE_CLEAR_SIGHAND
    | clone_flags::CLONE_DETACHED
    | clone_flags::CLONE_FILES
    | clone_flags::CLONE_FS
    | clone_flags::CLONE_INTO_CGROUP
    | clone_flags::CLONE_IO
    | clone_flags::CLONE_NEWCGROUP
    | clone_flags::CLONE_NEWIPC
    | clone_flags::CLONE_NEWNET
    | clone_flags::CLONE_NEWNS
    | clone_flags::CLONE_NEWPID
    | clone_flags::CLONE_NEWTIME
    | clone_flags::CLONE_NEWUSER
    | clone_flags::CLONE_NEWUTS
    | clone_flags::CLONE_PARENT
    | clone_flags::CLONE_PARENT_SETTID
    | clone_flags::CLONE_PIDFD
    | clone_flags::CLONE_PTRACE
    | clone_flags::CLONE_SETTLS
    | clone_flags::CLONE_SIGHAND
    | clone_flags::CLONE_SYSVSEM
    | clone_flags::CLONE_THREAD
    | clone_flags::CLONE_UNTRACED
    | clone_flags::CLONE_VFORK
    | clone_flags::CLONE_VM;

/// `execve` validates the path and queues the image replacement for the trap loop.
///
/// A handler cannot read another process's memory or call the kernel, so it only extracts the
/// path from the window the trap loop prefetched; [`crate::ExecRequest`] carries the `argv`/`envp`
/// pointers. The trap loop loads the new image from the initfs, starts its native process and
/// terminates the old one. Only static images present in the initfs can be executed; there is no
/// filesystem or dynamic loader yet. A relative path has no cwd in the server and is `-ENOENT`.
fn execve(ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    let argv = args.ptr(1) as u64;
    let envp = args.ptr(2) as u64;
    let result = with(ctx, |process| {
        let Some(end) = process.user_buffer.iter().position(|byte| *byte == 0) else {
            return -(Errno::ENAMETOOLONG.0 as isize);
        };
        let path = process.user_buffer.get(..end).unwrap_or(&[]);
        if path.is_empty() || !path.starts_with(b"/") {
            return -(Errno::ENOENT.0 as isize);
        }
        process.pending_exec = Some(ExecRequest {
            path: path.to_vec(),
            argv,
            envp,
        });
        0
    });
    result.unwrap_or_else(|_| einval())
}

/// `wait4` with no child is `-ECHILD`, which is what a single-process personality returns.
///
/// `clone` is the only way to create a child and it is blocked (see [`clone`]), so the child table
/// is always empty. The option word is still validated: `WNOHANG` is the only supported option.
fn wait4(_ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    if args.int(2) & !WNOHANG != 0 {
        return -(Errno::EINVAL.0 as isize);
    }
    -(Errno::ECHILD.0 as isize)
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
