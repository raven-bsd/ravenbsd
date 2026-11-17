// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MemView` and `RelocApplier` over live target memory.

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_elf::{ApplyError, MemView, RelocApplier};

/// The highest address the loader has mapped (for the raw view's sanity check).
static LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Static TLS TPOFF bases indexed by object id.
static TLS_OFFSETS: [AtomicUsize; 64] = [const { AtomicUsize::new(0) }; 64];

/// Record one object's static TLS offset for the relocation applier.
pub fn set_tls_offset(id: raven_libc_elf::ObjId, offset: usize) {
    if let Some(slot) = TLS_OFFSETS.get(usize::try_from(id.0).unwrap_or(usize::MAX)) {
        slot.store(offset, Ordering::Release);
    }
}

/// Record the top of the mapped address space.
pub fn set_limit(top: usize) {
    LIMIT.store(top, Ordering::Relaxed);
}

/// A `MemView` over live memory.
pub struct LiveMem;

impl MemView for LiveMem {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        if len == 0 {
            return Some(&[]);
        }
        let start = usize::try_from(vaddr).ok()?;
        let end = start.checked_add(len)?;
        let limit = LIMIT.load(Ordering::Relaxed);
        if limit != 0 && end > limit {
            return None;
        }
        // SAFETY: the loader only asks for ranges C52's digest accepted from a PT_LOAD; the
        // limit check above rejects a range above the reserved image.
        Some(unsafe { core::slice::from_raw_parts(start as *const u8, len) })
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        let start = usize::try_from(vaddr).ok()?;
        let limit = LIMIT.load(Ordering::Relaxed);
        let max = if limit > start {
            limit.saturating_sub(start).min(1 << 20)
        } else {
            1 << 20
        };
        let mut length = 0usize;
        while length < max {
            // SAFETY: the loader asks only for strings inside a validated object; the bound
            // stops the scan at the end of the user address space (or 1 MiB).
            if unsafe { *(start as *const u8).add(length) } == 0 {
                // SAFETY: the bytes scanned above are readable; the slice ends before the NUL.
                return Some(unsafe { core::slice::from_raw_parts(start as *const u8, length) });
            }
            length = length.saturating_add(1);
        }
        None
    }
}

/// A `RelocApplier` over live memory.
pub struct LiveApplier;

impl RelocApplier for LiveApplier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        let address = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        unsafe {
            (address as *mut u64).write_unaligned(val);
        }
        Ok(())
    }

    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        let address = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        unsafe {
            (address as *mut u32).write_unaligned(val);
        }
        Ok(())
    }

    fn read64(&self, addr: u64) -> Option<u64> {
        let address = usize::try_from(addr).ok()?;
        // SAFETY: `apply_object` validated the relocation offset against the PT_LOAD.
        Some(unsafe { (address as *const u64).read_unaligned() })
    }

    fn call_ifunc(
        &mut self,
        resolver: u64,
        _arg: &raven_libc_elf::IfuncArg,
    ) -> Result<u64, ApplyError> {
        let address = usize::try_from(resolver).map_err(|_| ApplyError::BadIfunc)?;
        // SAFETY: the resolver is an STT_GNU_IFUNC definition in a validated executable segment.
        let result = unsafe { core::mem::transmute::<usize, extern "C" fn() -> usize>(address)() };
        u64::try_from(result).map_err(|_| ApplyError::BadIfunc)
    }

    fn tls_module_id(&self, obj: raven_libc_elf::ObjId) -> u64 {
        u64::from(obj.0).saturating_add(1)
    }

    fn tls_static_offset(&mut self, obj: raven_libc_elf::ObjId) -> Result<i64, ApplyError> {
        let offset = TLS_OFFSETS
            .get(usize::try_from(obj.0).unwrap_or(usize::MAX))
            .map_or(0, |slot| slot.load(Ordering::Acquire));
        i64::try_from(offset).map_err(|_| ApplyError::OutOfRange)
    }

    fn tls_desc_alloc(
        &mut self,
        _obj: raven_libc_elf::ObjId,
        _sym_value: u64,
        _addend: i64,
    ) -> Result<raven_libc_elf::TlsDesc, ApplyError> {
        Ok(raven_libc_elf::TlsDesc {
            resolver: 0,
            arg: 0,
        })
    }

    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError> {
        let dst = usize::try_from(dst).map_err(|_| ApplyError::BadCopy)?;
        let src = usize::try_from(src).map_err(|_| ApplyError::BadCopy)?;
        let len = usize::try_from(len).map_err(|_| ApplyError::BadCopy)?;
        // SAFETY: both ranges were validated by the relocation walk.
        unsafe {
            core::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, len);
        }
        Ok(())
    }
}
