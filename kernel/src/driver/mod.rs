// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver objects: the root device capability, MMIO regions and boot blobs.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_bootinfo::MemoryKind;
use spin::Mutex;

use crate::kprintln;
use crate::memory::paging;
use crate::process::handle::{MemObject, MmioRegion, Object};
use crate::process::{self, Process};
use crate::syscall::SyscallFrame;

/// `BOOT_BLOB` kind: the ACPI bundle.
pub const BLOB_ACPI: u32 = 1;
/// `BOOT_BLOB` kind: the device tree (absent on x86-64).
pub const BLOB_DTB: u32 = 2;
/// `BOOT_BLOB` kind: the memory map summary.
pub const BLOB_MEMORY_MAP: u32 = 3;

/// The only defined `MMIO_CREATE` attribute set: uncached (PCD+PWT) device memory.
pub const MMIO_ATTRS_DEFAULT: u32 = 0;

/// Size of one local APIC / IOAPIC window the kernel owns.
const APIC_WINDOW: u64 = 0x1000;

/// Boot blobs built once during init.
struct BlobSet {
    acpi: Option<Arc<MemObject>>,
    memory_map: Option<Arc<MemObject>>,
}

static BLOBS: Mutex<Option<BlobSet>> = Mutex::new(None);

/// Build the boot blobs from the early ACPI capture and the copied memory map.
///
/// Called once from the x86-64 boot path after the heap exists. The ACPI bytes were captured
/// before loader memory was reclaimed; this step only wraps them in kernel-owned frames.
pub fn init() {
    let acpi = match crate::arch::acpi::early_blob() {
        Some(bytes) => match MemObject::from_bytes(bytes) {
            Ok(object) => Some(object),
            Err(error) => {
                kprintln!("driver: acpi blob frames failed: {error:?}");
                None
            }
        },
        None => None,
    };
    let memory_map = {
        let mut bytes = Vec::new();
        for region in paging::memory_map() {
            write_region(&mut bytes, *region);
        }
        match MemObject::from_bytes(&bytes) {
            Ok(object) => Some(object),
            Err(error) => {
                kprintln!("driver: memory map blob frames failed: {error:?}");
                None
            }
        }
    };
    *BLOBS.lock() = Some(BlobSet { acpi, memory_map });
}

fn write_region(bytes: &mut Vec<u8>, region: raven_bootinfo::MemoryRegion) {
    bytes.extend_from_slice(&region.base.to_le_bytes());
    bytes.extend_from_slice(&region.len.to_le_bytes());
    bytes.extend_from_slice(&region.kind.to_le_bytes());
    bytes.extend_from_slice(&region.reserved.to_le_bytes());
}

/// Dispatch an S21a syscall.
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    match call {
        Syscall::MMIO_CREATE => create_mmio(frame),
        Syscall::BOOT_BLOB => boot_blob(frame),
        _ => Err(Errno::ENOSYS),
    }
}

/// `mmio_create(root_dev, phys, len, attrs) -> Handle`.
pub fn create_mmio(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    require_root(&caller, Handle(frame.rdi as u32))?;
    let phys = frame.rsi;
    let len = frame.rdx;
    let attrs = u32::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    if attrs != MMIO_ATTRS_DEFAULT {
        return Err(Errno::EINVAL);
    }
    if len == 0 || !phys.is_multiple_of(0x1000) || !len.is_multiple_of(0x1000) {
        return Err(Errno::EINVAL);
    }
    let end = phys.checked_add(len).ok_or(Errno::EINVAL)?;
    refuse_kernel_owned(phys, end)?;

    let region = Arc::new(MmioRegion::new(phys, len, attrs));
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::MAP)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    Ok(caller
        .handles
        .lock()
        .insert(Object::MmioRegion(region), rights)?
        .0 as usize)
}

/// `boot_blob(root_dev, kind) -> Handle`.
pub fn boot_blob(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    require_root(&caller, Handle(frame.rdi as u32))?;
    let kind = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let object = match kind {
        BLOB_ACPI => BLOBS
            .lock()
            .as_ref()
            .and_then(|set| set.acpi.clone())
            .ok_or(Errno::ENOENT)?,
        BLOB_DTB => return Err(Errno::ENOENT),
        BLOB_MEMORY_MAP => BLOBS
            .lock()
            .as_ref()
            .and_then(|set| set.memory_map.clone())
            .ok_or(Errno::ENOENT)?,
        _ => return Err(Errno::EINVAL),
    };
    let rights = Rights::READ
        .union(Rights::MAP)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    Ok(caller
        .handles
        .lock()
        .insert(Object::BootBlob(object), rights)?
        .0 as usize)
}

/// The caller must hold a `RootDevice` capability with `ADMIN`.
fn require_root(caller: &Arc<Process>, handle: Handle) -> Result<(), Errno> {
    let table = caller.handles.lock();
    match table.get(handle, Rights::ADMIN) {
        Ok(Object::RootDevice) => Ok(()),
        Ok(_) => Err(Errno::EPERM),
        Err(Errno::EACCES) => Err(Errno::EPERM),
        Err(error) => Err(error),
    }
}

/// Refuse ranges that overlap RAM, the kernel image, or the interrupt-controller windows the
/// kernel owns. Device space is everything the memory map does not report as usable-like memory.
fn refuse_kernel_owned(phys: u64, end: u64) -> Result<(), Errno> {
    for region in paging::memory_map() {
        let kind = region.kind();
        if kind == Some(MemoryKind::Reserved) || kind == Some(MemoryKind::Mmio) {
            continue;
        }
        let Some(region_end) = region.end() else {
            continue;
        };
        if phys < region_end && region.base < end {
            return Err(Errno::EPERM);
        }
    }
    // SAFETY: paging published the copied BootInfo during init and it is immutable afterwards.
    let info = unsafe { &*paging::boot_info_ptr() };
    let kernel_end = info
        .kernel_phys_base
        .checked_add(info.kernel_size)
        .ok_or(Errno::EINVAL)?;
    if phys < kernel_end && info.kernel_phys_base < end {
        return Err(Errno::EPERM);
    }
    let acpi = crate::arch::acpi::info();
    if acpi.local_apic_address != 0 && overlaps(phys, end, acpi.local_apic_address, APIC_WINDOW) {
        return Err(Errno::EPERM);
    }
    let mut index = 0usize;
    while index < acpi.io_apic_count {
        if let Some(io_apic) = acpi.io_apics.get(index)
            && overlaps(phys, end, io_apic.address, APIC_WINDOW)
        {
            return Err(Errno::EPERM);
        }
        index = index.saturating_add(1);
    }
    Ok(())
}

fn overlaps(a: u64, a_end: u64, b: u64, b_len: u64) -> bool {
    let Some(b_end) = b.checked_add(b_len) else {
        return false;
    };
    a < b_end && b < a_end
}
