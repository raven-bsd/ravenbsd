// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver objects: the root device capability, MMIO regions and boot blobs.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_bootinfo::MemoryKind;
use spin::Mutex;

use crate::kprintln;
use crate::memory::{frame, paging};
use crate::process::handle::{MemObject, MmioRegion, Object};
use crate::process::{self, Process};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;
use raven_usercopy::UserSlice;

/// `BOOT_BLOB` kind: the ACPI bundle.
pub const BLOB_ACPI: u32 = 1;
/// `BOOT_BLOB` kind: the device tree (absent on x86-64).
pub const BLOB_DTB: u32 = 2;
/// `BOOT_BLOB` kind: the memory map summary.
pub const BLOB_MEMORY_MAP: u32 = 3;

/// The only defined `MMIO_CREATE` attribute set: uncached (PCD+PWT) device memory.
pub const MMIO_ATTRS_DEFAULT: u32 = 0;

/// `DMA_ALLOC` flag: map the buffer uncached. On x86-64 the default is cached write-back,
/// which is coherent; this flag exists for aarch64 and is honoured on x86-64 by mapping the
/// buffer with the device cache attributes.
pub const DMA_FLAG_NON_CACHEABLE: u32 = 1;
/// All valid `DMA_ALLOC` flags.
pub const DMA_FLAGS_MASK: u32 = DMA_FLAG_NON_CACHEABLE;
/// `DmaInfo.flags` bit: the kernel cannot claim DMA isolation today.
pub const DMA_INFO_UNPROTECTED: u32 = 1;
/// Default per-process DMA budget.
pub const DMA_PER_PROCESS_LIMIT: u64 = 64 * 1024 * 1024;

/// The DMA domain of a device capability. Today every device uses the identity domain; the
/// IOMMU (S18) and aarch64 `dma-ranges` (S21d) tickets will introduce real domains through the
/// same record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaDomain {
    /// Added to physical addresses to produce bus addresses.
    pub bus_offset: u64,
    /// One past the largest valid bus address.
    pub max_bus_addr: u64,
    /// Device accesses are coherent with the CPU without cache maintenance.
    pub coherent: bool,
    /// An IOMMU protects the device's accesses.
    pub protected: bool,
}

/// The no-IOMMU identity domain.
pub const DMA_DOMAIN_IDENTITY: DmaDomain = DmaDomain {
    bus_offset: 0,
    max_bus_addr: u64::MAX,
    coherent: true,
    protected: false,
};

/// A physically contiguous, zero-filled device buffer.
pub struct DmaBuffer {
    frames: Vec<u64>,
    len: u64,
    flags: u32,
    domain: DmaDomain,
    quota: Arc<AtomicU64>,
}

impl DmaBuffer {
    /// Bus address of the first byte (`phys + domain.bus_offset`).
    pub fn bus_addr(&self) -> u64 {
        self.frames
            .first()
            .copied()
            .unwrap_or(0)
            .saturating_add(self.domain.bus_offset)
    }

    /// Length in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Allocation flags.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// The domain this buffer was allocated against.
    pub fn domain(&self) -> DmaDomain {
        self.domain
    }

    /// Physical frame for page `index` inside the buffer.
    pub fn frame_at(&self, index: u64) -> Option<u64> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.frames.get(index).copied())
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        for physical in &self.frames {
            let _ = frame::free_frame(frame::PhysAddr(*physical));
        }
        self.quota.fetch_sub(self.len, Ordering::AcqRel);
        GLOBAL_USED.fetch_sub(self.len, Ordering::AcqRel);
    }
}

/// Global DMA budget: 25 % of usable RAM, computed once at init.
static GLOBAL_LIMIT: AtomicU64 = AtomicU64::new(0);
/// Currently allocated DMA bytes across all processes.
static GLOBAL_USED: AtomicU64 = AtomicU64::new(0);

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
        let mut usable = 0u64;
        for region in paging::memory_map() {
            if region.kind() == Some(MemoryKind::Usable) {
                usable = usable.saturating_add(region.len);
            }
            write_region(&mut bytes, *region);
        }
        GLOBAL_LIMIT.store(usable / 4, Ordering::Release);
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

/// Dispatch an S21a/S21b syscall.
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    match call {
        Syscall::MMIO_CREATE => create_mmio(frame),
        Syscall::BOOT_BLOB => boot_blob(frame),
        Syscall::DMA_ALLOC => dma_alloc(frame),
        Syscall::DMA_INFO => dma_info(frame),
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

/// `dma_alloc(root_dev, len, align, mask_bits, flags) -> Handle`.
pub fn dma_alloc(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    require_root(&caller, Handle(frame.rdi as u32))?;
    let len = frame.rsi;
    let align = frame.rdx;
    let mask_bits = u32::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    let flags = u32::try_from(frame.r8).map_err(|_| Errno::EINVAL)?;

    if len == 0 || !len.is_multiple_of(frame::PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    let align = if align == 0 { frame::PAGE_SIZE } else { align };
    if !align.is_multiple_of(frame::PAGE_SIZE) || !align.is_power_of_two() {
        return Err(Errno::EINVAL);
    }
    let mask_bits = if mask_bits == 0 { 32 } else { mask_bits };
    if !(20..=64).contains(&mask_bits) {
        return Err(Errno::EINVAL);
    }
    if flags & !DMA_FLAGS_MASK != 0 {
        return Err(Errno::EINVAL);
    }
    let max_frame = if mask_bits >= 64 {
        u64::MAX
    } else {
        (1u64 << mask_bits) / frame::PAGE_SIZE
    };
    if len.checked_div(frame::PAGE_SIZE).ok_or(Errno::EINVAL)? > max_frame {
        return Err(Errno::ENOMEM);
    }

    // Per-process and global budgets are checked atomically before any frame is taken.
    let quota = Arc::clone(&caller.dma_bytes);
    quota
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(len)
                .filter(|total| *total <= DMA_PER_PROCESS_LIMIT)
        })
        .map_err(|_| Errno::ENOMEM)?;
    GLOBAL_USED
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(len)
                .filter(|total| *total <= GLOBAL_LIMIT.load(Ordering::Acquire))
        })
        .map_err(|_| {
            quota.fetch_sub(len, Ordering::AcqRel);
            Errno::ENOMEM
        })?;

    let pages = len / frame::PAGE_SIZE;
    let align_pages = align / frame::PAGE_SIZE;
    let physical = frame::alloc_contiguous(pages, align_pages, max_frame);
    let Some(physical) = physical else {
        quota.fetch_sub(len, Ordering::AcqRel);
        GLOBAL_USED.fetch_sub(len, Ordering::AcqRel);
        return Err(Errno::ENOMEM);
    };

    // Zero the whole buffer through the HHDM; fresh frames are not trusted to be clear.
    let mut frames = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
    let mut page = 0u64;
    let mut failure = false;
    while page < pages {
        let address = physical
            .0
            .saturating_add(page.saturating_mul(frame::PAGE_SIZE));
        let pointer = frame::phys_to_virt(frame::PhysAddr(address));
        if pointer.is_null() {
            failure = true;
            break;
        }
        // SAFETY: `pointer` is the HHDM mapping of the freshly allocated frame, one page long.
        unsafe {
            core::ptr::write_bytes(pointer, 0, frame::PAGE_SIZE as usize);
        }
        frames.push(address);
        page = page.saturating_add(1);
    }
    if failure {
        for address in frames {
            let _ = frame::free_frame(frame::PhysAddr(address));
        }
        quota.fetch_sub(len, Ordering::AcqRel);
        GLOBAL_USED.fetch_sub(len, Ordering::AcqRel);
        return Err(Errno::ENOMEM);
    }

    let buffer = Arc::new(DmaBuffer {
        frames,
        len,
        flags,
        domain: DMA_DOMAIN_IDENTITY,
        quota,
    });
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::MAP)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    Ok(caller
        .handles
        .lock()
        .insert(Object::DmaBuffer(buffer), rights)?
        .0 as usize)
}

/// `dma_info(h, out) -> 0`.
pub fn dma_info(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let buffer = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::DmaBuffer(buffer) => Arc::clone(buffer),
            _ => return Err(Errno::EBADF),
        }
    };
    let domain = buffer.domain();
    let info = DmaInfoLayout {
        bus_addr: buffer.bus_addr(),
        len: buffer.len(),
        flags: if domain.protected {
            0
        } else {
            DMA_INFO_UNPROTECTED
        },
        pad: 0,
    };
    let destination = UserSlice::new(frame.rsi, 24)?;
    let bytes = info.as_bytes();
    copy_to_user(destination, &bytes)?;
    Ok(0)
}

/// The user-visible `DmaInfo` record. Its layout is fixed by the ABI syscall documentation.
#[repr(C)]
struct DmaInfoLayout {
    bus_addr: u64,
    len: u64,
    flags: u32,
    pad: u32,
}

impl DmaInfoLayout {
    fn as_bytes(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        let mut at = 0usize;
        for value in [self.bus_addr, self.len] {
            let end = at.checked_add(8).unwrap_or(0);
            if let Some(slot) = bytes.get_mut(at..end) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
            at = end;
        }
        for value in [self.flags, self.pad] {
            let end = at.checked_add(4).unwrap_or(0);
            if let Some(slot) = bytes.get_mut(at..end) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
            at = end;
        }
        bytes
    }
}
