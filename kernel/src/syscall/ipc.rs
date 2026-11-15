// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme IPC syscalls (`0x60..0x69`).

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::model::{self, NAME_MAX, PATH_MAX};
#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::{self, registry, router};
#[cfg(target_arch = "x86_64")]
use crate::ipc::model::{self, NAME_MAX, PATH_MAX};
#[cfg(target_arch = "x86_64")]
use crate::ipc::{self, registry, router};
use crate::process::{self, Process, handle::Object};
use crate::syscall::SyscallFrame;

/// Raw `pread(handle, buffer, len, offset)` number (PG-22; the ABI crate owns the enum value).
pub const PREAD: usize = 0x6b;
/// Raw `pwrite(handle, buffer, len, offset)` number.
pub const PWRITE: usize = 0x6c;
/// Raw `seek(handle, offset, whence) -> new_offset` number.
pub const SEEK: usize = 0x6d;

/// Dispatch one scheme IPC syscall.
pub fn dispatch(call: raven_abi::syscall::Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    match call {
        raven_abi::syscall::Syscall::SCHEME_REGISTER => register(&caller, frame),
        raven_abi::syscall::Syscall::OPEN => open(&caller, frame),
        raven_abi::syscall::Syscall::READ => read(&caller, frame),
        raven_abi::syscall::Syscall::WRITE => write(&caller, frame),
        raven_abi::syscall::Syscall::CLOSE => close(&caller, frame),
        raven_abi::syscall::Syscall::DUP => dup(&caller, frame),
        raven_abi::syscall::Syscall::FSTAT => fstat(&caller, frame),
        raven_abi::syscall::Syscall::CALL => call_scheme(&caller, frame),
        raven_abi::syscall::Syscall::RECV => recv(&caller, frame),
        raven_abi::syscall::Syscall::SEND => send(&caller, frame),
        _ => Err(Errno::ENOSYS),
    }
}

fn register(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let name = router::user_string(frame.rdi, frame.rsi, NAME_MAX)?;
    if !model::valid_scheme_name(&name) {
        return Err(Errno::EINVAL);
    }
    let handle = Handle(u32::try_from(frame.rdx).map_err(|_| Errno::EBADF)?);
    {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::ADMIN)? {
            Object::Process(process) if Arc::ptr_eq(process, caller) => {}
            _ => return Err(Errno::EACCES),
        }
    }
    let server = registry::register(caller, &name)?;
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    let handle = caller
        .handles
        .lock()
        .insert(Object::SchemeServer(server), rights)?;
    Ok(handle.0 as usize)
}

fn open(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let path = router::user_string(frame.rdi, frame.rsi, PATH_MAX)?;
    let (scheme, suffix) = model::split_path(&path).map_err(|_| Errno::EINVAL)?;
    let flags = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let server = registry::lookup(caller, scheme)?;
    let handle = router::open(caller, server, suffix, flags)?;
    Ok(handle.0 as usize)
}

fn read(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::READ)?;
    router::read(caller, &file, frame.rsi, frame.rdx)
}

fn write(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::WRITE)?;
    router::write(caller, &file, frame.rsi, frame.rdx)
}

/// `pread(handle, buffer, len, offset) -> bytes` (PG-22).
pub fn pread(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(&caller, handle, Rights::READ)?;
    router::pread(&caller, &file, frame.rsi, frame.rdx, frame.r10)
}

/// `pwrite(handle, buffer, len, offset) -> bytes` (PG-22).
pub fn pwrite(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(&caller, handle, Rights::WRITE)?;
    router::pwrite(&caller, &file, frame.rsi, frame.rdx, frame.r10)
}

/// `seek(handle, offset: i64, whence) -> new_offset` (PG-22).
pub fn seek(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    // Any valid SchemeFile handle may move its cursor, like FSTAT (PG-23).
    let file = ipc::file_from_handle(&caller, handle, Rights::NONE)?;
    let offset = frame.rsi as i64;
    let whence = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let new_offset = router::seek(&caller, &file, offset, whence)?;
    usize::try_from(new_offset).map_err(|_| Errno::EOVERFLOW)
}

fn close(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    router::close(caller, handle)?;
    Ok(0)
}

fn dup(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let mask = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let duplicate = router::dup(caller, handle, mask)?;
    Ok(duplicate.0 as usize)
}

fn fstat(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    // PG-23: `FSTAT` is metadata, not data access. Any valid SchemeFile handle may ask; the
    // server enforces the access its open-file description actually carries.
    let file = ipc::file_from_handle(caller, handle, Rights::NONE)?;
    router::fstat(caller, &file, frame.rsi, frame.rdx)?;
    Ok(0)
}

fn call_scheme(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    // PG-23: a typed `CALL` is a server operation selected by the OFD; the handle right does not
    // gate it. A non-SchemeFile handle is still `EBADF` in `file_from_handle`.
    let file = ipc::file_from_handle(caller, handle, Rights::NONE)?;
    router::call(caller, &file, frame.rsi, frame.rdx, frame.r10, frame.r8)
}

fn recv(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = router::recv(caller, frame.rdi, frame.rsi)?;
    Ok(handle.0 as usize)
}

fn send(caller: &Arc<Process>, frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    router::send(caller, handle, frame.rsi, frame.rdx)?;
    Ok(0)
}
