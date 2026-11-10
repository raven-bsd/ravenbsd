// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme IPC syscalls (`0x60..0x69`).

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

use crate::arch::x86_64::trapframe::TrapFrame;
use crate::ipc::model::{self, NAME_MAX, PATH_MAX};
use crate::ipc::{self, registry, router};
use crate::process::{self, Process, handle::Object};

/// Dispatch one scheme IPC syscall.
pub fn dispatch(call: raven_abi::syscall::Syscall, frame: &TrapFrame) -> Result<usize, Errno> {
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

fn register(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
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
    registry::register(caller, &name)?;
    Ok(0)
}

fn open(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let path = router::user_string(frame.rdi, frame.rsi, PATH_MAX)?;
    let (scheme, suffix) = model::split_path(&path).map_err(|_| Errno::EINVAL)?;
    let flags = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let server = registry::lookup(caller, scheme)?;
    let handle = router::open(caller, server, suffix, flags)?;
    Ok(handle.0 as usize)
}

fn read(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::READ)?;
    router::read(caller, &file, frame.rsi, frame.rdx)
}

fn write(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::WRITE)?;
    router::write(caller, &file, frame.rsi, frame.rdx)
}

fn close(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    router::close(caller, handle)?;
    Ok(0)
}

fn dup(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let mask = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let duplicate = router::dup(caller, handle, mask)?;
    Ok(duplicate.0 as usize)
}

fn fstat(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::READ)?;
    router::fstat(caller, &file, frame.rsi, frame.rdx)?;
    Ok(0)
}

fn call_scheme(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = ipc::file_from_handle(caller, handle, Rights::READ)?;
    router::call(caller, &file, frame.rsi, frame.rdx, frame.r10, frame.r8)
}

fn recv(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = router::recv(caller, frame.rdi, frame.rsi)?;
    Ok(handle.0 as usize)
}

fn send(caller: &Arc<Process>, frame: &TrapFrame) -> Result<usize, Errno> {
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    router::send(caller, handle, frame.rsi, frame.rdx)?;
    Ok(0)
}
