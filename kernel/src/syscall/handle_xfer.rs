// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `HANDLE_TRANSFER`: duplicate a transferable capability into another process.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_handles::LineageNode;

use crate::ipc::protocol::{ObjectKind, transfer_rights, transferable};
use crate::process::{self, Process, handle::Object};
use crate::syscall::SyscallFrame;

/// Dispatch `handle_transfer(dest_proc, handle, rights_mask)`.
pub fn transfer(frame: &SyscallFrame) -> Result<usize, Errno> {
    let destination_handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let source_handle = Handle(u32::try_from(frame.rsi).map_err(|_| Errno::EBADF)?);
    let mask = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = destination(&caller, destination_handle)?;
    let (object, rights, parent) = clone_transferable(&caller, source_handle)?;
    let (installed, node) = {
        let mut handles = destination.handles.lock();
        let installed = handles.insert_derived(object, transfer_rights(rights, mask), &parent)?;
        let node = handles.lineage_node(installed).ok();
        (installed, node)
    };
    if let Some(node) = node {
        crate::process::handle::register_lineage(node);
    }
    Ok(installed.0 as usize)
}

/// Dispatch `handle_transfer_file(dest_file, handle, rights_mask)` (raw 0x55).
///
/// The destination is the process that owns the scheme server behind `dest_file`, so a client can
/// send a resource to a server without holding a process handle for it. The source must be
/// transferable, the file must be a scheme file the caller can read, and the installed rights are
/// the source rights intersected with `rights_mask`.
pub fn transfer_to_file(frame: &SyscallFrame) -> Result<usize, Errno> {
    let file_handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let source_handle = Handle(u32::try_from(frame.rsi).map_err(|_| Errno::EBADF)?);
    let mask = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let file = crate::ipc::file_from_handle(&caller, file_handle, Rights::READ)?;
    let destination = Arc::clone(&file.server);
    let (object, rights, parent) = clone_transferable(&caller, source_handle)?;
    let (installed, node) = {
        let mut handles = destination.handles.lock();
        let installed = handles.insert_derived(object, transfer_rights(rights, mask), &parent)?;
        let node = handles.lineage_node(installed).ok();
        (installed, node)
    };
    if let Some(node) = node {
        crate::process::handle::register_lineage(node);
    }
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

/// `handle_copy(dst_proc, src_handle, rights_mask) -> Handle` (raw 0x54).
///
/// The caller is the source process; both the caller and `dst_proc` need
/// `ADMIN`. The source handle is cloned with `rights_mask` (which must be a
/// subset of its usable rights), derives from the source lineage node, and is
/// installed in the destination table. `Process` objects are copied as an Arc;
/// transferable-only C14 rules are not involved.
pub fn copy(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let dst_handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let src_handle = Handle(u32::try_from(frame.rsi).map_err(|_| Errno::EBADF)?);
    let mask = Rights(u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?);
    if mask == Rights::NONE {
        return Err(Errno::EINVAL);
    }
    let self_admin = {
        let handles = caller.handles.lock();
        handles.get(Handle::SELF_PROC, Rights::ADMIN).is_ok()
    };
    if !self_admin {
        return Err(Errno::EPERM);
    }
    let (object, rights, lineage) = {
        let handles = caller.handles.lock();
        let (object, rights) = handles.get_with_rights(src_handle, Rights::NONE)?;
        let lineage = handles.lineage_node(src_handle)?;
        (clone_for_copy(object)?, rights, lineage)
    };
    // The split is C17: a right this handle held and an ancestor revoked is EPERM; a right it
    // never held is EACCES even when the same bit was revoked on an ancestor.
    let revoked = lineage.revoked_effective();
    if mask.0 & revoked.0 != 0 {
        return Err(Errno::EPERM);
    }
    if mask.0 & !rights.0 != 0 {
        return Err(Errno::EACCES);
    }
    let destination = {
        let handles = caller.handles.lock();
        match handles.get(dst_handle, Rights::ADMIN) {
            Ok(Object::Process(target)) => Arc::clone(target),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    let (installed, node) = {
        let mut handles = destination.handles.lock();
        let installed = handles.insert_derived(object, mask, &lineage)?;
        let node = handles.lineage_node(installed).ok();
        (installed, node)
    };
    if let Some(node) = node {
        crate::process::handle::register_lineage(node);
    }
    Ok(installed.0 as usize)
}

/// Clone one `Object` for `HANDLE_COPY`; unsupported kinds are rejected.
fn clone_for_copy(object: &Object) -> Result<Object, Errno> {
    match object {
        Object::Process(process) => Ok(Object::Process(Arc::clone(process))),
        Object::Memory(memory) => Ok(Object::Memory(Arc::clone(memory))),
        Object::Timer(timer) => Ok(Object::Timer(Arc::clone(timer))),
        Object::UserEvent(event) => Ok(Object::UserEvent(Arc::clone(event))),
        Object::RootDevice => Ok(Object::RootDevice),
        Object::MmioRegion(region) => Ok(Object::MmioRegion(Arc::clone(region))),
        Object::BootBlob(blob) => Ok(Object::BootBlob(Arc::clone(blob))),
        Object::DmaBuffer(buffer) => Ok(Object::DmaBuffer(Arc::clone(buffer))),
        Object::Irq(irq) => Ok(Object::Irq(Arc::clone(irq))),
        Object::SchemeFile(file) => Ok(Object::SchemeFile(Arc::clone(file))),
        Object::SchemeServer(server) => Ok(Object::SchemeServer(Arc::clone(server))),
        Object::Pipe(end) => Ok(Object::Pipe(Arc::clone(end))),
        Object::Duplex(end) => Ok(Object::Duplex(Arc::clone(end))),
        _ => Err(Errno::EINVAL),
    }
}

/// Clone a transferable object and return the rights stored on its handle without taking two
/// table locks.
/// The source table is released before the destination table is acquired, so cross-process
/// transfers cannot form a table-lock cycle. An `Arc` keeps the object alive after source close.
/// Map a kernel object to the C14 policy vocabulary.
const fn object_kind(object: &Object) -> ObjectKind {
    match object {
        Object::Process(_) => ObjectKind::Process,
        Object::MemObjectInitfs => ObjectKind::MemObjectInitfs,
        Object::Memory(_) => ObjectKind::MemObject,
        Object::Namespace => ObjectKind::Namespace,
        Object::Thread(_) => ObjectKind::Thread,
        Object::EventQueue(_) => ObjectKind::EventQueue,
        Object::Timer(_) => ObjectKind::Timer,
        Object::UserEvent(_) => ObjectKind::UserEvent,
        Object::RootDevice => ObjectKind::RootDevice,
        Object::MmioRegion(_) => ObjectKind::MmioRegion,
        Object::BootBlob(_) => ObjectKind::BootBlob,
        Object::DmaBuffer(_) => ObjectKind::DmaBuffer,
        Object::Irq(_) => ObjectKind::Irq,
        Object::SchemeFile(_) => ObjectKind::SchemeFile,
        Object::SchemeServer(_) => ObjectKind::SchemeServer,
        Object::SchemeRequest(_) => ObjectKind::SchemeRequest,
        Object::Pipe(_) => ObjectKind::Pipe,
        Object::Duplex(_) => ObjectKind::Duplex,
    }
}

pub fn clone_transferable(
    caller: &Arc<Process>,
    handle: Handle,
) -> Result<(Object, Rights, Arc<LineageNode>), Errno> {
    let table = caller.handles.lock();
    let (entry, rights) = match table.get_with_rights(handle, Rights::TRANSFER) {
        Ok(item) => item,
        Err(Errno::EACCES) => return Err(Errno::EPERM),
        Err(error) => return Err(error),
    };
    let node = table.lineage_node(handle)?;
    // C14: only MemObject, SchemeFile and SchemeServer cross a transfer.
    if !transferable(object_kind(entry)) {
        return Err(Errno::EBADF);
    }
    let object = match entry {
        Object::MemObjectInitfs => Object::MemObjectInitfs,
        Object::Memory(object) => Object::Memory(Arc::clone(object)),
        Object::Timer(object) => Object::Timer(Arc::clone(object)),
        Object::UserEvent(object) => Object::UserEvent(Arc::clone(object)),
        Object::RootDevice => Object::RootDevice,
        Object::MmioRegion(object) => Object::MmioRegion(Arc::clone(object)),
        Object::BootBlob(object) => Object::BootBlob(Arc::clone(object)),
        Object::DmaBuffer(object) => Object::DmaBuffer(Arc::clone(object)),
        Object::Irq(object) => Object::Irq(Arc::clone(object)),
        Object::SchemeFile(object) => Object::SchemeFile(Arc::clone(object)),
        Object::SchemeServer(object) => Object::SchemeServer(Arc::clone(object)),
        Object::Pipe(object) => Object::Pipe(Arc::clone(object)),
        Object::Duplex(object) => Object::Duplex(Arc::clone(object)),
        _ => return Err(Errno::EBADF),
    };
    Ok((object, rights, node))
}
