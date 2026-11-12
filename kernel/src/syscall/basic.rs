// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The M1 syscall subset.

use alloc::format;
use alloc::string::String;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Timespec, clock};
use raven_abi::uname::Uname;
use raven_usercopy::UserSlice;

use crate::arch::serial;
use crate::cmdline::CmdLine;
use crate::process;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::{copy_from_user, copy_to_user};

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
            let _ = crate::sched::sleep::sleep_ns(frame.rdi);
            Ok(0)
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
    serial::write_bytes(written);
    Ok(length)
}

fn clock_get(clock_id: u32, pointer: u64) -> Result<usize, Errno> {
    let ns = match clock_id {
        clock::MONOTONIC => time::now_ns(),
        clock::REALTIME => time::boot_realtime_ns(),
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
    process.handles.lock().close(Handle(raw))?;
    // Deregistration is observable only after the close commits: a failed close (stale
    // generation, empty slot) must leave every event-queue registration untouched.
    crate::event::on_handle_close(&process, Handle(raw));
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
    Ok(0)
}
