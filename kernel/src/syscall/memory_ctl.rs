// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MEMOBJ_CREATE`, `MAP`, `UNMAP` and `PROTECT`.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::{Handle, Rights, map, prot};
use raven_vma::PAGE_SIZE;

use crate::process::{self, Process, handle::MemObject, handle::Object};
use crate::syscall::SyscallFrame;

/// Largest memory object accepted by `MEMOBJ_CREATE` (1 GiB).
const MAX_MEMOBJ_BYTES: u64 = 1 << 30;

enum MappedObject {
    Anonymous,
    Initfs,
    MemObject(Arc<MemObject>),
}

/// `memobj_create(len, flags) -> Handle`.
pub fn memobj_create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let len = frame.rdi;
    if frame.rsi != 0 || len == 0 || !len.is_multiple_of(PAGE_SIZE) || len > MAX_MEMOBJ_BYTES {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let object = MemObject::new(len / PAGE_SIZE)?;
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::MAP)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    let raw = caller
        .handles
        .lock()
        .insert(Object::Memory(object), rights)?;
    Ok(raw.0 as usize)
}

/// `map(proc, memobj, offset, addr, len, flags) -> addr`.
pub fn map(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = lookup_process(&caller, Handle(frame.rdi as u32), Rights::MAP)?;
    let raw_memobj = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let offset = frame.rdx;
    let addr = frame.r10;
    let len = frame.r8;
    let flags = usize::try_from(frame.r9).map_err(|_| Errno::EINVAL)?;

    let allowed = prot::MASK | ((map::FIXED | map::SHARED) << map::SHIFT);
    if flags & !allowed != 0 {
        return Err(Errno::EINVAL);
    }
    let protection = (flags & prot::MASK) as u8;
    if protection & !(prot::MASK as u8) != 0 {
        return Err(Errno::EINVAL);
    }
    if protection & (prot::WRITE as u8) != 0 && protection & (prot::EXEC as u8) != 0 {
        return Err(Errno::EINVAL);
    }
    let map_flags = flags >> map::SHIFT;
    let fixed = map_flags & map::FIXED != 0;
    let shared = map_flags & map::SHARED != 0;
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }

    let object = resolve_object(&caller, raw_memobj, offset, protection, shared)?;

    if fixed {
        let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
        if addr < USER_MIN || end > USER_END {
            return Err(Errno::EINVAL);
        }
    }

    let mut guard = target.address_space.lock();
    let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
    let vaddr = if fixed {
        if !address_space.vmas().is_free(addr, len) {
            return Err(Errno::EEXIST);
        }
        addr
    } else {
        address_space
            .vmas()
            .find_free(len, addr, PAGE_SIZE)
            .ok_or(Errno::ENOMEM)?
    };

    match object {
        MappedObject::Anonymous => address_space.map_anonymous(vaddr, len, protection)?,
        MappedObject::Initfs => {
            address_space.map_initfs(vaddr, len, offset, protection)?;
        }
        MappedObject::MemObject(object) => {
            address_space.map_memobj(vaddr, len, offset, protection, object, shared)?;
        }
    }
    Ok(vaddr as usize)
}

/// `unmap(proc, addr, len) -> 0`.
pub fn unmap(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = lookup_process(&caller, Handle(frame.rdi as u32), Rights::MAP)?;
    let addr = frame.rsi;
    let len = frame.rdx;
    validate_range(addr, len)?;
    let mut guard = target.address_space.lock();
    let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
    address_space.unmap(addr, len)?;
    Ok(0)
}

/// `protect(proc, addr, len, prot) -> 0`.
pub fn protect(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = lookup_process(&caller, Handle(frame.rdi as u32), Rights::MAP)?;
    let addr = frame.rsi;
    let len = frame.rdx;
    let protection = u8::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    validate_range(addr, len)?;
    if protection & !(prot::MASK as u8) != 0
        || (protection & (prot::WRITE as u8) != 0 && protection & (prot::EXEC as u8) != 0)
    {
        return Err(Errno::EINVAL);
    }
    let mut guard = target.address_space.lock();
    let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
    address_space.protect(addr, len, protection)?;
    Ok(0)
}

fn lookup_process(
    caller: &Arc<Process>,
    handle: Handle,
    rights: Rights,
) -> Result<Arc<Process>, Errno> {
    let handles = caller.handles.lock();
    match handles.get(handle, rights)? {
        Object::Process(process) => Ok(Arc::clone(process)),
        _ => Err(Errno::EBADF),
    }
}

fn resolve_object(
    caller: &Arc<Process>,
    raw: u32,
    offset: u64,
    protection: u8,
    shared: bool,
) -> Result<MappedObject, Errno> {
    if raw == Handle::NONE.0 {
        if shared || offset != 0 {
            return Err(Errno::EINVAL);
        }
        return Ok(MappedObject::Anonymous);
    }
    if !offset.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    let handle = Handle(raw);
    let exec = protection & (prot::EXEC as u8) != 0;
    if raw == Handle::INITFS.0 {
        if shared {
            return Err(Errno::EINVAL);
        }
        let mut needed = Rights::READ.union(Rights::MAP);
        if exec {
            needed = needed.union(Rights::EXEC);
        }
        let handles = caller.handles.lock();
        match handles.get(Handle::INITFS, needed)? {
            Object::MemObjectInitfs => Ok(MappedObject::Initfs),
            _ => Err(Errno::EBADF),
        }
    } else {
        let mut needed = Rights::MAP;
        if shared && protection & (prot::WRITE as u8) != 0 {
            needed = needed.union(Rights::WRITE);
        }
        if exec {
            needed = needed.union(Rights::EXEC);
        }
        let handles = caller.handles.lock();
        match handles.get(handle, needed)? {
            Object::Memory(object) => Ok(MappedObject::MemObject(Arc::clone(object))),
            _ => Err(Errno::EBADF),
        }
    }
}

fn validate_range(addr: u64, len: u64) -> Result<(), Errno> {
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
    if addr < USER_MIN || end > USER_END {
        return Err(Errno::EINVAL);
    }
    Ok(())
}
