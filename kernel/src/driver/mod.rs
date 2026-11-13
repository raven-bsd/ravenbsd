// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver objects: the root device capability, MMIO regions and boot blobs.

#[cfg(target_arch = "x86_64")]
use alloc::collections::BTreeMap;
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

/// The only defined `MMIO_CREATE` attribute set: uncached device memory (x86-64 PCD+PWT,
/// aarch64 Device-nGnRE).
pub const MMIO_ATTRS_DEFAULT: u32 = 0;

/// `DMA_ALLOC` flag: map the buffer uncached. On x86-64 the default is coherent cached
/// write-back and this flag selects the device attributes; aarch64 always maps DMA buffers
/// Normal-NC (MAIR attr2), so the flag is implied there.
pub const DMA_FLAG_NON_CACHEABLE: u32 = 1;
/// All valid `DMA_ALLOC` flags.
pub const DMA_FLAGS_MASK: u32 = DMA_FLAG_NON_CACHEABLE;
/// `DmaInfo.flags` bit: the kernel cannot claim DMA isolation today.
pub const DMA_INFO_UNPROTECTED: u32 = 1;
/// Default per-process DMA budget.
pub const DMA_PER_PROCESS_LIMIT: u64 = 64 * 1024 * 1024;

/// The DMA domain of a device capability.
///
/// x86-64 uses the identity domain; the IOMMU (S18) ticket will introduce protected domains
/// through the same record. On aarch64 the Pi 4 domain is derived from the live device tree's
/// `/soc` `dma-ranges`; QEMU `virt` has no `dma-ranges` and stays identity.
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
    /// Domain id reported through `DMA_INFO` (0 for the identity domain).
    pub id: u16,
}

/// The no-IOMMU identity domain.
pub const DMA_DOMAIN_IDENTITY: DmaDomain = DmaDomain {
    bus_offset: 0,
    max_bus_addr: u64::MAX,
    coherent: true,
    protected: false,
    id: 0,
};

/// The domain every allocation on this machine uses when no per-device IOMMU
/// domain applies. Initialised to identity and replaced once by the platform
/// boot path when the device tree declares `dma-ranges`.
static DMA_DOMAIN: Mutex<DmaDomain> = Mutex::new(DMA_DOMAIN_IDENTITY);

/// The last PCI function whose MMIO region this process created.
///
/// `DMA_ALLOC` has no device argument in the S21 ABI, so the kernel binds the
/// allocation to the device whose BAR or configuration page the caller opened
/// immediately before it (the `pcid` resource sequence is bar/config, then
/// dma). Without a hint the allocation stays on the identity/unprotected path.
#[cfg(target_arch = "x86_64")]
static DEVICE_HINTS: Mutex<BTreeMap<u64, vtd::Bdf>> = Mutex::new(BTreeMap::new());

#[cfg(target_arch = "x86_64")]
#[path = "../arch/x86_64/vtd.rs"]
mod vtd;

/// A physically contiguous, zero-filled device buffer.
pub struct DmaBuffer {
    frames: Vec<u64>,
    len: u64,
    flags: u32,
    domain: DmaDomain,
    quota: Arc<AtomicU64>,
    #[cfg(target_arch = "x86_64")]
    iova: Option<u64>,
    #[cfg(target_arch = "x86_64")]
    iommu: Option<Arc<vtd::Domain>>,
}

impl DmaBuffer {
    /// Bus address of the first byte: the IOVA for a protected domain, otherwise
    /// `phys + domain.bus_offset`.
    pub fn bus_addr(&self) -> u64 {
        #[cfg(target_arch = "x86_64")]
        if let Some(iova) = self.iova {
            return iova;
        }
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
        #[cfg(target_arch = "x86_64")]
        if let (Some(iommu), Some(iova)) = (&self.iommu, self.iova) {
            vtd::unmap_buffer(iommu, iova, self.len);
        }
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

#[cfg(target_arch = "x86_64")]
pub(crate) mod irq;
#[cfg(target_arch = "aarch64")]
#[path = "irq_aarch64.rs"]
pub(crate) mod irq;
/// Boot blobs built once during init.
struct BlobSet {
    acpi: Option<Arc<MemObject>>,
    dtb: Option<Arc<MemObject>>,
    memory_map: Option<Arc<MemObject>>,
}

static BLOBS: Mutex<Option<BlobSet>> = Mutex::new(None);

/// Build the boot blobs from the early ACPI capture and the copied memory map.
///
/// Called once from the x86-64 boot path after the heap exists. The ACPI bytes were captured
/// before loader memory was reclaimed; this step only wraps them in kernel-owned frames.
#[cfg(target_arch = "x86_64")]
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
    *BLOBS.lock() = Some(BlobSet {
        acpi,
        dtb: None,
        memory_map: build_memory_map_blob(),
    });
    vtd::init();
}

/// Build the boot blobs and derive the DMA domain on aarch64.
///
/// Called once from the aarch64 boot path after the kernel page tables and heap exist. The DTB
/// is copied into kernel-owned frames so a later `BOOT_BLOB` kind 2 does not depend on loader
/// memory; the `/soc` `dma-ranges` window becomes this machine's single DMA domain.
#[cfg(target_arch = "aarch64")]
pub fn init_aarch64(gicd: u64, gicc: u64, uart: u64) {
    *PLATFORM_OWNED.lock() = [gicd, gicc, uart];
    let dtb =
        crate::arch::aarch64::fdt::dtb_slice().and_then(|bytes| {
            match MemObject::from_bytes(bytes) {
                Ok(object) => Some(object),
                Err(error) => {
                    kprintln!("driver: dtb blob frames failed: {error:?}");
                    None
                }
            }
        });
    if let Some(window) = crate::arch::aarch64::fdt::platform_dma_window() {
        let domain = DmaDomain {
            bus_offset: window.bus_base.wrapping_sub(window.cpu_base),
            max_bus_addr: window.bus_base.saturating_add(window.size),
            coherent: false,
            protected: false,
            id: 0,
        };
        *DMA_DOMAIN.lock() = domain;
        kprintln!(
            "driver: dma offset=0x{:x} max=0x{:x}",
            domain.bus_offset,
            domain.max_bus_addr
        );
    }
    *BLOBS.lock() = Some(BlobSet {
        acpi: None,
        dtb,
        memory_map: build_memory_map_blob(),
    });
}

/// The platform windows the kernel owns and `MMIO_CREATE` must refuse on aarch64.
#[cfg(target_arch = "aarch64")]
static PLATFORM_OWNED: Mutex<[u64; 3]> = Mutex::new([0; 3]);

/// Wrap the copied memory map in a read-only boot blob and record the global DMA budget.
fn build_memory_map_blob() -> Option<Arc<MemObject>> {
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
}

fn write_region(bytes: &mut Vec<u8>, region: raven_bootinfo::MemoryRegion) {
    bytes.extend_from_slice(&region.base.to_le_bytes());
    bytes.extend_from_slice(&region.len.to_le_bytes());
    bytes.extend_from_slice(&region.kind.to_le_bytes());
    bytes.extend_from_slice(&region.reserved.to_le_bytes());
}

/// Dispatch an S21a/S21b/S21c syscall.
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    match call {
        Syscall::MMIO_CREATE => create_mmio(frame),
        Syscall::BOOT_BLOB => boot_blob(frame),
        Syscall::DMA_ALLOC => dma_alloc(frame),
        Syscall::DMA_INFO => dma_info(frame),
        Syscall::IRQ_CREATE => irq::create(frame),
        Syscall::IRQ_WAIT => irq::wait(frame),
        Syscall::IRQ_ACK => irq::ack(frame),
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

    #[cfg(target_arch = "x86_64")]
    if vtd::available()
        && let Some(bdf) = vtd::bdf_for_range(phys, len)
    {
        DEVICE_HINTS.lock().insert(caller.id, bdf);
    }

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
        BLOB_DTB => BLOBS
            .lock()
            .as_ref()
            .and_then(|set| set.dtb.clone())
            .ok_or(Errno::ENOENT)?,
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
pub(crate) fn require_root(caller: &Arc<Process>, handle: Handle) -> Result<(), Errno> {
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
    #[cfg(target_arch = "x86_64")]
    {
        let acpi = crate::arch::acpi::info();
        if acpi.local_apic_address != 0 && overlaps(phys, end, acpi.local_apic_address, APIC_WINDOW)
        {
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
    }
    #[cfg(target_arch = "aarch64")]
    {
        for base in PLATFORM_OWNED.lock().iter().copied() {
            if base != 0 && overlaps(phys, end, base, APIC_WINDOW) {
                return Err(Errno::EPERM);
            }
        }
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
    // aarch64 has no cache-maintenance syscall for drivers, so the kernel selects Normal-NC
    // (MAIR attr2) for every DMA buffer regardless of the request flags.
    #[cfg(target_arch = "aarch64")]
    let flags = flags | DMA_FLAG_NON_CACHEABLE;

    #[cfg(target_arch = "x86_64")]
    let iommu = {
        let hint = DEVICE_HINTS.lock().get(&caller.id).copied();
        match hint {
            Some(bdf) => match vtd::domain_for_bdf(bdf) {
                Ok(domain) => Some(domain),
                Err(reason) => {
                    kprintln!("vtd: domain {}: {reason}; using identity", bdf.text());
                    None
                }
            },
            None => None,
        }
    };
    #[cfg(target_arch = "x86_64")]
    let protected = iommu.is_some();
    #[cfg(not(target_arch = "x86_64"))]
    let protected = false;

    #[cfg(target_arch = "x86_64")]
    let domain = match &iommu {
        Some(iommu_domain) => DmaDomain {
            bus_offset: 0,
            max_bus_addr: iommu_domain.iova_end(),
            coherent: true,
            protected: true,
            id: iommu_domain.id(),
        },
        None => *DMA_DOMAIN.lock(),
    };
    #[cfg(not(target_arch = "x86_64"))]
    let domain = *DMA_DOMAIN.lock();

    let bus_limit = if mask_bits >= 64 {
        u64::MAX
    } else {
        1u64 << mask_bits
    };
    let max_frame = if protected {
        u64::MAX / frame::PAGE_SIZE
    } else {
        let device_limit = domain.max_bus_addr.min(bus_limit);
        let Some(max_phys_end) = device_limit.checked_sub(domain.bus_offset) else {
            return Err(Errno::ENOMEM);
        };
        max_phys_end / frame::PAGE_SIZE
    };
    let pages = len.checked_div(frame::PAGE_SIZE).ok_or(Errno::EINVAL)?;
    if pages > max_frame || max_frame == 0 {
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

    let align_pages = if protected {
        1
    } else {
        align / frame::PAGE_SIZE
    };
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

    #[cfg(target_arch = "x86_64")]
    let mut iova = None;
    #[cfg(target_arch = "x86_64")]
    if let Some(iommu_domain) = &iommu {
        match vtd::map_buffer(iommu_domain, &frames, len, align, bus_limit) {
            Ok(mapped) => {
                iova = Some(mapped);
                vtd::start_fault_worker();
            }
            Err(reason) => {
                for address in &frames {
                    let _ = frame::free_frame(frame::PhysAddr(*address));
                }
                quota.fetch_sub(len, Ordering::AcqRel);
                GLOBAL_USED.fetch_sub(len, Ordering::AcqRel);
                kprintln!("vtd: map failed: {reason}");
                return Err(Errno::ENOMEM);
            }
        }
    }

    let buffer = Arc::new(DmaBuffer {
        frames,
        len,
        flags,
        domain,
        quota,
        #[cfg(target_arch = "x86_64")]
        iova,
        #[cfg(target_arch = "x86_64")]
        iommu,
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
    #[cfg(target_arch = "x86_64")]
    vtd::poll_faults();
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
        pad: u32::from(domain.id),
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
