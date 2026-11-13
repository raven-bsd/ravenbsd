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
use raven_linux_dispatch::{Dispatcher, LinuxContext, SyscallArgs};

use crate::table::LinuxProcess;

/// `MAP_ANONYMOUS` from the Linux UAPI (the dispatcher's `raven-linux-abi` crate owns the number).
const MAP_ANONYMOUS: i32 = 0x20;

/// `PROT_EXEC` from the Linux UAPI.
const PROT_EXEC: i32 = 0x4;

/// Page size of the guest's `mmap`.
const PAGE: u64 = 4096;

/// `ARCH_SET_FS` of x86-64.
const ARCH_SET_FS: i32 = 0x1002;

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
fn arch_prctl(_ctx: &mut dyn LinuxContext, args: &SyscallArgs) -> isize {
    if args.int(0) == ARCH_SET_FS {
        return -(Errno::ENOSYS.0 as isize);
    }
    -(Errno::EINVAL.0 as isize)
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
