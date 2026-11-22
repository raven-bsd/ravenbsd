// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MEMOBJ_CREATE`, `MAP`, `UNMAP` and `PROTECT`.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::{Handle, Rights, map, memobj, prot};
use raven_usercopy::UserSlice;
use raven_vma::PAGE_SIZE;

use crate::driver::DmaBuffer;
use crate::memory::uspace::{InheritIdentity, MapInherit};
use crate::process::handle::MmioRegion;
use crate::process::{self, Process, handle::MemObject, handle::Object};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_from_user;

/// Largest memory object accepted by `MEMOBJ_CREATE` (1 GiB).
const MAX_MEMOBJ_BYTES: u64 = 1 << 30;
/// POSIX `MS_ASYNC`.
const MS_ASYNC: usize = 1;
/// POSIX `MS_INVALIDATE`.
const MS_INVALIDATE: usize = 2;
/// POSIX `MS_SYNC`.
const MS_SYNC: usize = 4;

enum MappedObject {
    Anonymous,
    Initfs,
    MemObject(Arc<MemObject>),
    Mmio(Arc<MmioRegion>),
    BootBlob(Arc<MemObject>),
    Dma(Arc<DmaBuffer>),
}

/// Debug hook: a TLB shootdown must never run while this CPU holds an address-space mutex.
struct AddressSpaceLockProbe;

impl AddressSpaceLockProbe {
    fn held() -> Self {
        crate::sched::smp::percpu::note_unmap_lock_held(true);
        Self
    }
}

impl Drop for AddressSpaceLockProbe {
    fn drop(&mut self) {
        crate::sched::smp::percpu::note_unmap_lock_held(false);
    }
}

/// `memobj_create(len, flags) -> Handle`.
pub fn memobj_create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let len = frame.rdi;
    let flags = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if flags & !memobj::PAGER != 0
        || len == 0
        || !len.is_multiple_of(PAGE_SIZE)
        || len > MAX_MEMOBJ_BYTES
    {
        return Err(Errno::EINVAL);
    }
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let paged = flags & memobj::PAGER != 0;
    let object = if paged {
        MemObject::new_paged(len / PAGE_SIZE, &caller)?
    } else {
        MemObject::new(len / PAGE_SIZE)?
    };
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::MAP)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    let raw = caller
        .handles
        .lock()
        .insert(Object::Memory(object.clone()), rights)?;
    if paged {
        object.set_pager_handle(raw.0);
    }
    Ok(raw.0 as usize)
}

/// `pager_supply(object, offset, data, data_len) -> 0` (contract C18).
pub fn pager_supply(frame: &SyscallFrame) -> Result<usize, Errno> {
    let process = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let offset = frame.rsi;
    let data_len = usize::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    if data_len == 0 || data_len > PAGE_SIZE as usize || !offset.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    let object = {
        let handles = process.handles.lock();
        match handles.get(handle, Rights::WRITE) {
            Ok(Object::Memory(object)) => Arc::clone(object),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if !object.is_pager(&process) {
        return Err(Errno::EPERM);
    }
    let end = offset.checked_add(PAGE_SIZE).ok_or(Errno::EINVAL)?;
    if end > object.len() {
        return Err(Errno::EINVAL);
    }
    let mut buffer = alloc::vec![0u8; data_len];
    let source = UserSlice::new(frame.rdx, data_len as u64)?;
    copy_from_user(&mut buffer, source)?;
    object.supply_page(offset / PAGE_SIZE, &buffer)?;
    Ok(0)
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

    let allowed = prot::MASK
        | ((map::FIXED | map::REPLACE | map::SHARED | map::NOFORK | map::ZERO_ON_FORK)
            << map::SHIFT);
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
    let replace = map_flags & map::REPLACE != 0;
    let shared = map_flags & map::SHARED != 0;
    let nofork = map_flags & map::NOFORK != 0;
    let zero_on_fork = map_flags & map::ZERO_ON_FORK != 0;
    if replace && !fixed {
        return Err(Errno::EINVAL);
    }
    if nofork && zero_on_fork {
        return Err(Errno::EINVAL);
    }
    if zero_on_fork && shared {
        return Err(Errno::EINVAL);
    }
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }

    let object = resolve_object(&caller, raw_memobj, offset, len, protection, shared)?;
    let inherit = if nofork {
        MapInherit::NoFork
    } else if zero_on_fork {
        MapInherit::ZeroOnFork
    } else {
        MapInherit::Inherit
    };
    let identity = InheritIdentity {
        handle: raw_memobj,
        object: match &object {
            MappedObject::MemObject(object) | MappedObject::BootBlob(object) => Some(object.id()),
            _ => None,
        },
    };

    if fixed {
        let end = addr.checked_add(len).ok_or(Errno::EINVAL)?;
        if addr < USER_MIN || end > USER_END {
            return Err(Errno::EINVAL);
        }
    }

    let mut guard = target.address_space.lock();
    let probe = AddressSpaceLockProbe::held();
    let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
    let mut deferred = None;
    let vaddr = if replace {
        // CR-24: REPLACE is the atomic form of FIXED for a PROT_NONE reservation. It refuses a
        // range carrying any real mapping (EEXIST) instead of silently discarding it.
        if !address_space.is_reservation(addr, len) {
            return Err(Errno::EEXIST);
        }
        deferred = Some(address_space.unmap(addr, len)?);
        addr
    } else if fixed {
        if !address_space.vmas().is_free(addr, len) {
            return Err(Errno::EEXIST);
        }
        addr
    } else {
        let hint = if addr == 0 {
            address_space.mmap_base()
        } else {
            addr
        };
        address_space
            .vmas()
            .find_free(len, hint, PAGE_SIZE)
            .ok_or(Errno::ENOMEM)?
    };

    let mapped = match object {
        MappedObject::Anonymous => address_space.map_anonymous(vaddr, len, protection),
        MappedObject::Initfs => address_space.map_initfs(vaddr, len, offset, protection),
        MappedObject::MemObject(object) => {
            address_space.map_memobj(vaddr, len, offset, protection, object, shared)
        }
        MappedObject::Mmio(region) => match region.phys().checked_add(offset) {
            Some(physical) => {
                let writable = protection & (prot::WRITE as u8) != 0;
                address_space.map_mmio(vaddr, len, physical, writable)
            }
            None => Err(Errno::EINVAL),
        },
        MappedObject::BootBlob(object) => {
            address_space.map_memobj(vaddr, len, offset, protection, object, false)
        }
        MappedObject::Dma(buffer) => {
            let non_cacheable = buffer.flags() & crate::driver::DMA_FLAG_NON_CACHEABLE != 0;
            let writable = protection & (prot::WRITE as u8) != 0;
            address_space.map_dma(vaddr, len, offset, writable, non_cacheable, buffer)
        }
    };
    if let Err(error) = mapped {
        if replace {
            // The old reservation was removed under the lock; put an equivalent PROT_NONE
            // reservation back so a failed replace does not leave a hole.
            if address_space.map_anonymous(addr, len, 0).is_ok() {
                address_space.set_inherit(
                    addr,
                    len,
                    MapInherit::Inherit,
                    InheritIdentity::default(),
                );
            }
        }
        drop(guard);
        drop(probe);
        if let Some(deferred) = deferred {
            deferred.finish();
        }
        return Err(error);
    }
    address_space.set_inherit(vaddr, len, inherit, identity);
    drop(guard);
    drop(probe);
    if let Some(deferred) = deferred {
        deferred.finish();
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
    let (dirty, shared_range, deferred) = {
        let mut guard = target.address_space.lock();
        let _probe = AddressSpaceLockProbe::held();
        let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
        let shared_range = address_space.shared_futex_range(addr, len);
        // Collect before the mapping disappears; the object Arcs keep the pages alive for the
        // PageOut requests below.
        let dirty = address_space.collect_pager_dirty(addr, len);
        let deferred = address_space.unmap(addr, len)?;
        (dirty, shared_range, deferred)
    };
    // The address-space lock is released above. Wait for the remote TLB acknowledgement and
    // recycle frames only now, so an aarch64 peer blocked on this mutex can take the TLB SGI.
    deferred.finish();
    crate::syscall::futex::remove_private_range(target.id, addr, addr.saturating_add(len));
    if let Some((object, start, end)) = shared_range {
        crate::syscall::futex::remove_shared_range(target.id, object, start, end);
    }
    // C18: the address-space lock is released before the pager IPC wait.
    #[cfg(target_arch = "x86_64")]
    for (object, page) in dirty {
        crate::memory::pager::page_out(&caller, &object, page)?;
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = dirty;
    Ok(0)
}

/// `msync(proc, addr, len, flags) -> 0`.
///
/// Only shared pager-backed mappings have dirty pages to write back. `MS_ASYNC` is implemented
/// synchronously (the writeback is started and completed before returning); `MS_INVALIDATE` has
/// no additional effect because private copies are never cached across page-outs.
pub fn msync(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let target = lookup_process(&caller, Handle(frame.rdi as u32), Rights::MAP)?;
    let addr = frame.rsi;
    let len = frame.rdx;
    let flags = usize::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    if flags & !(MS_ASYNC | MS_INVALIDATE | MS_SYNC) != 0 || flags & (MS_ASYNC | MS_SYNC) == 0 {
        return Err(Errno::EINVAL);
    }
    validate_range(addr, len)?;
    let dirty = {
        let guard = target.address_space.lock();
        let address_space = guard.as_ref().ok_or(Errno::ESRCH)?;
        address_space.collect_pager_dirty(addr, len)
    };
    // C18: the address-space lock is released before the pager IPC wait.
    #[cfg(target_arch = "x86_64")]
    for (object, page) in dirty {
        crate::memory::pager::page_out(&caller, &object, page)?;
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = dirty;
    Ok(0)
}

/// Unmap `[addr, addr + len)` from the calling process for `THREAD_EXIT_EX`.
///
/// The exiting thread is already on its kernel stack, so removing its user
/// stack is safe; an invalid or already-unmapped range is ignored (the caller
/// never returns to user mode).
pub fn unmap_current(addr: u64, len: u64) {
    let Some(caller) = process::current() else {
        return;
    };
    if validate_range(addr, len).is_err() {
        return;
    }
    let deferred = {
        let mut guard = caller.address_space.lock();
        let _probe = AddressSpaceLockProbe::held();
        guard
            .as_mut()
            .and_then(|address_space| address_space.unmap(addr, len).ok())
    };
    if let Some(deferred) = deferred {
        deferred.finish();
    }
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
    let deferred = {
        let mut guard = target.address_space.lock();
        let _probe = AddressSpaceLockProbe::held();
        let address_space = guard.as_mut().ok_or(Errno::ESRCH)?;
        address_space.protect(addr, len, protection)?
    };
    deferred.finish();
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
    len: u64,
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
        enum Kind {
            Initfs,
            Memory,
            Mmio,
            Blob,
            Dma,
        }
        let kind = {
            let handles = caller.handles.lock();
            match handles.get(handle, Rights::NONE) {
                Ok(Object::MemObjectInitfs) => Kind::Initfs,
                Ok(Object::Memory(_)) => Kind::Memory,
                Ok(Object::MmioRegion(_)) => Kind::Mmio,
                Ok(Object::BootBlob(_)) => Kind::Blob,
                Ok(Object::DmaBuffer(_)) => Kind::Dma,
                Ok(_) => return Err(Errno::EBADF),
                Err(error) => return Err(error),
            }
        };
        match kind {
            Kind::Initfs => {
                // A transferred initfs handle keeps the strict well-known-handle rules: the
                // backing stays `Backing::Initfs`, so `protect` cannot make the physical pages
                // writable, and shared mappings are not allowed.
                if shared {
                    return Err(Errno::EINVAL);
                }
                let mut needed = Rights::READ.union(Rights::MAP);
                if exec {
                    needed = needed.union(Rights::EXEC);
                }
                let handles = caller.handles.lock();
                match handles.get(handle, needed)? {
                    Object::MemObjectInitfs => Ok(MappedObject::Initfs),
                    _ => Err(Errno::EBADF),
                }
            }
            Kind::Memory => {
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
            Kind::Mmio => {
                if exec {
                    return Err(Errno::EINVAL);
                }
                let mut needed = Rights::MAP;
                if protection & (prot::WRITE as u8) != 0 {
                    needed = needed.union(Rights::WRITE);
                }
                let region = {
                    let handles = caller.handles.lock();
                    match handles.get(handle, needed) {
                        Ok(Object::MmioRegion(region)) => Arc::clone(region),
                        Ok(_) => return Err(Errno::EPERM),
                        Err(Errno::EACCES) => return Err(Errno::EPERM),
                        Err(error) => return Err(error),
                    }
                };
                let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
                if end > region.len() {
                    return Err(Errno::EINVAL);
                }
                Ok(MappedObject::Mmio(region))
            }
            Kind::Blob => {
                if protection & (prot::WRITE as u8) != 0 {
                    return Err(Errno::EPERM);
                }
                if shared {
                    return Err(Errno::EINVAL);
                }
                let handles = caller.handles.lock();
                match handles.get(handle, Rights::MAP)? {
                    Object::BootBlob(object) => Ok(MappedObject::BootBlob(Arc::clone(object))),
                    _ => Err(Errno::EBADF),
                }
            }
            Kind::Dma => {
                let mut needed = Rights::MAP;
                if protection & (prot::WRITE as u8) != 0 {
                    needed = needed.union(Rights::WRITE);
                }
                let buffer = {
                    let handles = caller.handles.lock();
                    match handles.get(handle, needed) {
                        Ok(Object::DmaBuffer(buffer)) => Arc::clone(buffer),
                        Ok(_) => return Err(Errno::EPERM),
                        Err(Errno::EACCES) => return Err(Errno::EPERM),
                        Err(error) => return Err(error),
                    }
                };
                let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
                if end > buffer.len() {
                    return Err(Errno::EINVAL);
                }
                Ok(MappedObject::Dma(buffer))
            }
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
