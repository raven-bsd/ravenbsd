// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `HANDLE_TRANSFER`: duplicate a transferable capability into another process.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

use crate::process::{self, Process, handle::Object};
use crate::syscall::SyscallFrame;

/// Dispatch `handle_transfer(dest_proc, handle, rights_mask)`.
pub fn transfer(frame: &SyscallFrame) -> Result<usize, Errno> {
    let destination_handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let source_handle = Handle(u32::try_from(frame.rsi).map_err(|_| Errno::EBADF)?);
    let mask = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if mask == 0 {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = destination(&caller, destination_handle)?;
    let (object, rights) = clone_transferable(&caller, source_handle)?;
    let installed = destination
        .handles
        .lock()
        .insert(object, rights.intersect(Rights(mask)))?;
    Ok(installed.0 as usize)
}

fn destination(caller: &Arc<Process>, handle: Handle) -> Result<Arc<Process>, Errno> {
    let table = caller.handles.lock();
    match table.get(handle, Rights::WRITE) {
        Ok(Object::Process(process)) => Ok(Arc::clone(process)),
        Ok(_) => Err(Errno::EINVAL),
        Err(Errno::EACCES) => Err(Errno::EPERM),
        Err(error) => Err(error),
    }
}

/// Clone a transferable object and reconstruct its current rights without taking two tables locks.
/// The source table is released before the destination table is acquired, so cross-process
/// transfers cannot form a table-lock cycle. An `Arc` keeps the object alive after source close.
pub fn clone_transferable(
    caller: &Arc<Process>,
    handle: Handle,
) -> Result<(Object, Rights), Errno> {
    let table = caller.handles.lock();
    let object = match table.get(handle, Rights::TRANSFER) {
        Ok(Object::MemObjectInitfs) => Object::MemObjectInitfs,
        Ok(Object::Memory(object)) => Object::Memory(Arc::clone(object)),
        Ok(Object::Timer(object)) => Object::Timer(Arc::clone(object)),
        Ok(Object::UserEvent(object)) => Object::UserEvent(Arc::clone(object)),
        Ok(Object::RootDevice) => Object::RootDevice,
        #[cfg(target_arch = "x86_64")]
        Ok(Object::MmioRegion(object)) => Object::MmioRegion(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Ok(Object::BootBlob(object)) => Object::BootBlob(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Ok(Object::DmaBuffer(object)) => Object::DmaBuffer(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Ok(Object::SchemeFile(object)) => Object::SchemeFile(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Ok(Object::SchemeServer(object)) => Object::SchemeServer(Arc::clone(object)),
        Ok(_) => return Err(Errno::EBADF),
        Err(Errno::EACCES) => return Err(Errno::EPERM),
        Err(error) => return Err(error),
    };
    let mut rights = Rights::NONE;
    for bit in [
        Rights::READ,
        Rights::WRITE,
        Rights::MAP,
        Rights::EXEC,
        Rights::DUP,
        Rights::TRANSFER,
        Rights::SIGNAL,
        Rights::ADMIN,
    ] {
        if table.get(handle, bit).is_ok() {
            rights = rights.union(bit);
        }
    }
    Ok((object, rights))
}
