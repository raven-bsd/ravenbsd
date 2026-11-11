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

/// Clone a transferable object and return the rights stored on its handle without taking two
/// table locks.
/// The source table is released before the destination table is acquired, so cross-process
/// transfers cannot form a table-lock cycle. An `Arc` keeps the object alive after source close.
pub fn clone_transferable(
    caller: &Arc<Process>,
    handle: Handle,
) -> Result<(Object, Rights), Errno> {
    let table = caller.handles.lock();
    let (entry, rights) = match table.get_with_rights(handle, Rights::TRANSFER) {
        Ok(item) => item,
        Err(Errno::EACCES) => return Err(Errno::EPERM),
        Err(error) => return Err(error),
    };
    let object = match entry {
        Object::MemObjectInitfs => Object::MemObjectInitfs,
        Object::Memory(object) => Object::Memory(Arc::clone(object)),
        Object::Timer(object) => Object::Timer(Arc::clone(object)),
        Object::UserEvent(object) => Object::UserEvent(Arc::clone(object)),
        Object::RootDevice => Object::RootDevice,
        #[cfg(target_arch = "x86_64")]
        Object::MmioRegion(object) => Object::MmioRegion(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Object::BootBlob(object) => Object::BootBlob(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Object::DmaBuffer(object) => Object::DmaBuffer(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Object::Irq(object) => Object::Irq(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Object::SchemeFile(object) => Object::SchemeFile(Arc::clone(object)),
        #[cfg(target_arch = "x86_64")]
        Object::SchemeServer(object) => Object::SchemeServer(Arc::clone(object)),
        _ => return Err(Errno::EBADF),
    };
    Ok((object, rights))
}
