// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The M1 syscall subset.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Timespec, clock};
use raven_abi::uname::Uname;
use raven_usercopy::UserSlice;

use crate::cmdline::CmdLine;
use crate::process;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::{copy_from_user, copy_to_user};

/// `getrandom(buf: *mut u8, len: usize, flags: u32) -> usize` syscall number.
///
/// Assigned by the reviewer (S72); not yet a `raven-abi` variant, so it is dispatched by raw
/// number in [`crate::syscall`] and documented here as the required contract diff.
pub const GETRANDOM: usize = 0x08;
/// `clock_set(clock_id: u32, ts: *const Timespec) -> 0` syscall number (reviewer-assigned, S72).
pub const CLOCK_SET: usize = 0x09;
/// `getrandom` flag: the kernel never blocks on entropy, so this is accepted and ignored.
pub const GRND_NONBLOCK: u32 = 1;
/// `getrandom` flag: the single pool serves both requests, so this is accepted and ignored.
pub const GRND_RANDOM: u32 = 2;

/// Dispatch one decoded syscall.
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    match call {
        Syscall::EXIT => process::exit_current(frame.rdi as i32),
        Syscall::YIELD => {
            crate::sched::yield_now();
            Ok(0)
        }
        Syscall::DEBUG_WRITE => debug_write(frame.rdi, frame.rsi),
        Syscall::CLOCK_GET => clock_get(frame.rdi as u32, frame.rsi),
        Syscall::NANOSLEEP => {
            let status = crate::sched::sleep::sleep_ns(frame.rdi);
            if status == Errno::EINTR.to_ret() {
                Err(Errno::EINTR)
            } else {
                Ok(0)
            }
        }
        Syscall::UNAME => uname(frame.rdi),
        Syscall::HANDLE_CLOSE => handle_close(frame.rdi as u32),
        Syscall::HANDLE_DUP => handle_dup(frame.rdi as u32, frame.rsi as u32),
        Syscall::HANDLE_REVOKE => handle_revoke(frame.rdi as u32, frame.rsi as u32),
        _ => Err(Errno::ENOSYS),
    }
}

fn debug_write(pointer: u64, length: u64) -> Result<usize, Errno> {
    let length = usize::try_from(length).map_err(|_| Errno::EINVAL)?;
    if length > 4096 {
        return Err(Errno::EINVAL);
    }
    if length == 0 {
        return Ok(0);
    }
    let source = UserSlice::new(pointer, length as u64)?;
    let mut buffer = [0u8; 4096];
    let destination = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
    copy_from_user(destination, source)?;
    let written = buffer.get(..length).ok_or(Errno::EINVAL)?;
    crate::log::_write_bytes(written);
    Ok(length)
}

fn clock_get(clock_id: u32, pointer: u64) -> Result<usize, Errno> {
    let ns = match clock_id {
        clock::MONOTONIC => time::now_ns(),
        clock::REALTIME => time::realtime_ns(),
        _ => return Err(Errno::EINVAL),
    };
    let timespec = Timespec {
        tv_sec: (ns / 1_000_000_000) as i64,
        tv_nsec: (ns % 1_000_000_000) as i64,
    };
    let destination = UserSlice::new(
        pointer,
        u64::try_from(core::mem::size_of::<Timespec>()).map_err(|_| Errno::EFAULT)?,
    )?;
    // SAFETY: `timespec` is a live stack value and the slice covers exactly its bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(timespec).cast::<u8>(),
            core::mem::size_of::<Timespec>(),
        )
    };
    copy_to_user(destination, bytes)?;
    Ok(0)
}

/// Require the caller's effective uid to be 0.
///
/// The 0.1.0 authority model is `euid == 0`; a delegable `CAP_TIME`/`CAP_DTRACE` is
/// post-0.1.0. A missing current process is `ESRCH`, a non-root caller `EPERM`.
pub(crate) fn require_root() -> Result<(), Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    if caller.ids.lock().euid == 0 {
        Ok(())
    } else {
        Err(Errno::EPERM)
    }
}

/// `CLOCK_SET`: set the wall clock from a user `Timespec`.
///
/// Only `clock::REALTIME` is settable; a non-positive or out-of-range value is `EINVAL`. The
/// monotonic clock is unaffected. The write requires effective uid 0 (FreeBSD
/// `kern_clock_settime` `priv_check(PRIV_CLOCK_SETTIME)`, NetBSD `settime1` `KAUTH_SYSTEM_TIME`);
/// the check is after the copy/validation and before the clock write, so a denied caller cannot
/// change the clock.
pub fn clock_set(frame: &SyscallFrame) -> Result<usize, Errno> {
    let clock_id = frame.rdi as u32;
    if clock_id != clock::REALTIME {
        return Err(Errno::EINVAL);
    }
    let mut timespec = Timespec::default();
    let source = UserSlice::new(
        frame.rsi,
        u64::try_from(core::mem::size_of::<Timespec>()).map_err(|_| Errno::EFAULT)?,
    )?;
    // SAFETY: `timespec` is a live stack value and the slice covers exactly its bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(timespec).cast::<u8>(),
            core::mem::size_of::<Timespec>(),
        )
    };
    copy_from_user(bytes, source)?;
    if timespec.tv_sec < 0 || !(0..1_000_000_000).contains(&timespec.tv_nsec) {
        return Err(Errno::EINVAL);
    }
    let seconds = u64::try_from(timespec.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nanos = u64::try_from(timespec.tv_nsec).map_err(|_| Errno::EINVAL)?;
    require_root()?;
    time::set_realtime_ns(seconds.saturating_mul(1_000_000_000).saturating_add(nanos));
    Ok(0)
}

/// `GETRANDOM`: fill a user buffer from the boot-seeded ChaCha20 pool.
///
/// The pool is seeded once at boot from RDRAND/RNDR; timer jitter is never cryptographic input.
/// If the platform has no hardware source (or `entropy=none` disabled it) the pool stays unseeded
/// and the call returns `EAGAIN`; it never serves a predictable fallback. `flags` is accepted and
/// ignored (the kernel does not block on entropy, so `GRND_NONBLOCK` is a no-op). The length is
/// capped at `0x1FFF_FFFF` like Linux.
pub fn getrandom(frame: &SyscallFrame) -> Result<usize, Errno> {
    let len = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let flags = frame.rdx as u32;
    if flags & !(GRND_NONBLOCK | GRND_RANDOM) != 0 {
        return Err(Errno::EINVAL);
    }
    if len > 0x1FFF_FFFF {
        return Err(Errno::EINVAL);
    }
    if len == 0 {
        return Ok(0);
    }
    let mut buffer = [0u8; 256];
    let mut copied = 0usize;
    while copied < len {
        let take = len.saturating_sub(copied).min(buffer.len());
        let Some(out) = buffer.get_mut(..take) else {
            return Err(Errno::EINVAL);
        };
        match time::entropy_fill(out) {
            Ok(n) => {
                let destination =
                    UserSlice::new(frame.rdi.saturating_add(copied as u64), n as u64)?;
                copy_to_user(destination, out)?;
                copied = copied.saturating_add(n);
            }
            Err(_) => return Err(Errno::EAGAIN),
        }
    }
    Ok(copied)
}

fn uname(pointer: u64) -> Result<usize, Errno> {
    let cmdline = CmdLine::parse(crate::memory::paging::cmdline());
    let profile = cmdline.get("profile").unwrap_or("base");
    let profile: String = profile.chars().map(|ch| ch.to_ascii_uppercase()).collect();
    let version = format!("RavenBSD 0.1 ({profile}) #1: {}", env!("RAVEN_BUILD_ID"));

    let mut uname = Uname::zeroed();
    Uname::set_field(&mut uname.sysname, raven_abi::uname::SYSNAME);
    Uname::set_field(&mut uname.nodename, "raven");
    Uname::set_field(&mut uname.release, raven_abi::uname::RELEASE);
    Uname::set_field(&mut uname.version, &version);
    Uname::set_field(&mut uname.machine, crate::arch::ARCH_NAME);

    let destination = UserSlice::new(
        pointer,
        u64::try_from(core::mem::size_of::<Uname>()).map_err(|_| Errno::EFAULT)?,
    )?;
    // SAFETY: `uname` is a live stack value and the slice covers exactly its bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(uname).cast::<u8>(),
            core::mem::size_of::<Uname>(),
        )
    };
    copy_to_user(destination, bytes)?;
    Ok(0)
}

fn handle_close(raw: u32) -> Result<usize, Errno> {
    let process = process::current().ok_or(Errno::ESRCH)?;
    // The single close primitive: it commits the table close, cancels an IRQ waiter, removes
    // event registrations, and fails/releases a server-side `SchemeRequest` that can no longer
    // receive its reply.
    crate::event::close_handle(&process, Handle(raw))?;
    Ok(0)
}

fn handle_dup(raw: u32, mask: u32) -> Result<usize, Errno> {
    let process = process::current().ok_or(Errno::ESRCH)?;
    let (duplicate, node) = {
        let mut handles = process.handles.lock();
        let duplicate = handles.dup(Handle(raw), raven_abi::types::Rights(mask))?;
        let node = handles.lineage_node(duplicate).ok();
        (duplicate, node)
    };
    if let Some(node) = node {
        crate::process::handle::register_lineage(node);
    }
    Ok(duplicate.0 as usize)
}

/// `HANDLE_REVOKE`: remove rights from a handle and every handle derived from it.
///
/// Contract C17: no right is required to reduce a handle the caller owns; `mask == 0` means every
/// right currently usable; bits not held are ignored. The target node is revoked under the handle
/// table lock, then the derived-node registry propagates the same mask across processes. The
/// handle stays open, and a later lookup that needs a revoked bit returns `EPERM`.
fn handle_revoke(raw: u32, mask: u32) -> Result<usize, Errno> {
    let process = process::current().ok_or(Errno::ESRCH)?;
    let (node, applied) = {
        let mut handles = process.handles.lock();
        handles.revoke_target(Handle(raw), raven_abi::types::Rights(mask))?
    };
    crate::process::handle::revoke_descendants(&node, applied);
    let request = {
        let handles = process.handles.lock();
        match handles.get(Handle(raw), raven_abi::types::Rights::NONE) {
            Ok(crate::process::handle::Object::SchemeRequest(request)) => Some(Arc::clone(request)),
            _ => None,
        }
    };
    if let Some(request) = request
        && applied.contains(raven_abi::types::Rights::WRITE)
    {
        // The server can no longer send the reply. Fail the client and release the capture now;
        // the handle may stay open, so a later close must not be the only release path.
        request.fail(Errno::EIO);
        request
            .server
            .active_requests
            .lock()
            .retain(|item| !Arc::ptr_eq(item, &request));
        request.release_captures();
    }
    Ok(0)
}
