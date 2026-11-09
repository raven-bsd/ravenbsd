// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel page tables: build the kernel's own address space, switch to it and reclaim loader
//! memory.
//!
//! The loader's tables are replaced after the frame allocator exists. The new root maps the
//! complete HHDM (2 MiB pages where alignment allows), the four kernel image segments with their
//! final permissions, and the boot stack (inside `.bss`, so it is part of the data mapping; no
//! separate guard page is possible without unmapping unrelated `.bss` objects).
//!
//! The `BootInfo`, command line and memory map are copied into kernel-owned statics before
//! `LoaderReclaimable` frames are returned to the frame allocator. The initfs stays
//! `KernelAndModules` and is never freed.

use core::cell::UnsafeCell;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::layout::x86_64::HHDM_BASE;
use raven_bootinfo::{BootInfo, MemoryKind, MemoryRegion, PhysRange};
use raven_rmm::{
    FrameAllocator as RmmFrameAllocator, MapError, Mapper, PageFlags, PageSize, PageTable,
    PhysAddr, PhysMemory, TlbFlusher, VirtAddr, page_align_down, page_align_up,
};

use crate::boot::BootData;
use crate::kprintln;
use crate::memory::frame;

/// Maximum memory-map entries kept in kernel-owned memory.
const MAX_MEMORY_MAP: usize = 512;
/// Command-line buffer size required by the ticket.
const CMDLINE_MAX: usize = 1024;

struct BootInfoCell(UnsafeCell<BootInfo>);

// SAFETY: the cell is written once during single-CPU paging init and read-only afterwards.
unsafe impl Sync for BootInfoCell {}

static BOOT_INFO: BootInfoCell = BootInfoCell(UnsafeCell::new(BootInfo::zeroed()));

const ZERO_REGION: MemoryRegion = MemoryRegion {
    base: 0,
    len: 0,
    kind: 0,
    reserved: 0,
};

struct MemoryMapCell(UnsafeCell<[MemoryRegion; MAX_MEMORY_MAP]>);

// SAFETY: the array is written once during paging init and read-only afterwards.
unsafe impl Sync for MemoryMapCell {}

static MEMORY_MAP: MemoryMapCell = MemoryMapCell(UnsafeCell::new([ZERO_REGION; MAX_MEMORY_MAP]));

struct CmdlineCell(UnsafeCell<[u8; CMDLINE_MAX]>);

// SAFETY: the buffer is written once during paging init and read-only afterwards; `boot::validate`
// only reads it.
unsafe impl Sync for CmdlineCell {}

static CMDLINE: CmdlineCell = CmdlineCell(UnsafeCell::new([0; CMDLINE_MAX]));

/// Active kernel root frame, or zero before [`init`].
static KERNEL_ROOT: AtomicU64 = AtomicU64::new(0);

/// Why paging initialisation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagingError {
    /// A mapper operation failed.
    Map(MapError),
    /// No frame was available for the root table.
    OutOfFrames,
    /// The command line does not fit the 1 KiB static buffer.
    CmdlineTooLong,
    /// The memory map does not fit the static array.
    TooManyRegions,
    /// A kernel static is outside the loaded image.
    BadKernelAddress,
    /// An address calculation overflowed.
    Overflow,
    /// The HHDM base does not match the architecture contract.
    HhdmMismatch,
}

impl fmt::Display for PagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PagingError::Map(error) => write!(f, "page mapping failed: {error}"),
            PagingError::OutOfFrames => f.write_str("out of frames"),
            PagingError::CmdlineTooLong => f.write_str("command line does not fit 1 KiB"),
            PagingError::TooManyRegions => f.write_str("memory map does not fit 512 entries"),
            PagingError::BadKernelAddress => f.write_str("kernel static outside the image"),
            PagingError::Overflow => f.write_str("address overflow"),
            PagingError::HhdmMismatch => f.write_str("HHDM base mismatch"),
        }
    }
}

impl From<MapError> for PagingError {
    fn from(error: MapError) -> Self {
        PagingError::Map(error)
    }
}

/// A frame allocator backed by the kernel's own bitmap allocator.
pub struct KernelFrameAllocator;

impl RmmFrameAllocator for KernelFrameAllocator {
    fn allocate(&mut self) -> Option<PhysAddr> {
        frame::alloc_zeroed_frame().map(rmm_phys)
    }

    fn free(&mut self, frame: PhysAddr) {
        let _ = frame::free_frame(frame_phys(frame));
    }
}

/// Direct-map access used by the mapper.
pub struct KernelPhysMemory;

impl PhysMemory for KernelPhysMemory {
    fn phys_to_virt(&self, phys: PhysAddr) -> *mut u8 {
        frame::phys_to_virt(frame_phys(phys))
    }
}

/// TLB invalidation for the active kernel tables.
pub struct KernelTlb;

impl TlbFlusher for KernelTlb {
    fn flush(&mut self, virt: VirtAddr) {
        // SAFETY: `invlpg` is a ring-0 instruction that only invalidates one TLB entry.
        unsafe {
            core::arch::asm!(
                "invlpg [{}]",
                in(reg) virt.data(),
                options(nostack, preserves_flags),
            );
        }
    }

    fn flush_all(&mut self) {
        // SAFETY: reloading CR3 with its current value invalidates non-global TLB entries; it does
        // not change the address space.
        unsafe {
            core::arch::asm!(
                "mov rax, cr3",
                "mov cr3, rax",
                out("rax") _,
                options(nostack, preserves_flags),
            );
        }
    }
}

/// The kernel mapper over the active root.
pub type KernelMapper<'a> = Mapper<'a, KernelFrameAllocator, KernelPhysMemory, KernelTlb>;

/// Run `operation` with a mapper over the active kernel root.
pub fn with_kernel_mapper<R>(
    operation: impl FnOnce(&mut KernelMapper<'_>) -> R,
) -> Result<R, MapError> {
    let root = kernel_root().ok_or(MapError::OutOfFrames)?;
    let mut allocator = KernelFrameAllocator;
    let memory = KernelPhysMemory;
    let mut tlb = KernelTlb;
    let mut mapper = Mapper::new(root, &mut allocator, &memory, &mut tlb)?;
    Ok(operation(&mut mapper))
}

/// Build and activate the kernel address space.
///
/// Must be called after [`crate::memory::frame::init`]. Returns the pointer to the copied
/// `BootInfo` that later boot stages must use.
pub fn init(data: &BootData) -> Result<*const BootInfo, PagingError> {
    if data.info.hhdm_offset != HHDM_BASE {
        return Err(PagingError::HhdmMismatch);
    }
    copy_boot_data(data)?;
    let kernel_offset = data
        .info
        .kernel_virt_base
        .checked_sub(data.info.kernel_phys_base)
        .ok_or(PagingError::BadKernelAddress)?;

    let root = frame::alloc_zeroed_frame().ok_or(PagingError::OutOfFrames)?;
    let mut allocator = KernelFrameAllocator;
    let memory = KernelPhysMemory;
    let mut tlb = KernelTlb;
    let mut mapper = Mapper::new(rmm_phys(root), &mut allocator, &memory, &mut tlb)?;

    let mut hhdm_bytes = 0u64;
    for region in data.memory_map {
        if region.len == 0 || region.kind() == Some(MemoryKind::BadMemory) {
            continue;
        }
        let virt = HHDM_BASE
            .checked_add(region.base)
            .ok_or(PagingError::Overflow)?;
        let virt = VirtAddr::new(virt).ok_or(MapError::NotCanonical)?;
        mapper.map_range(
            virt,
            rmm_phys(frame::PhysAddr(region.base)),
            region.len,
            PageFlags::kernel_rw(),
        )?;
        hhdm_bytes = hhdm_bytes.saturating_add(region.len);
    }

    map_kernel_image(&mut mapper, kernel_offset)?;
    activate(root);
    KERNEL_ROOT.store(root.0, Ordering::Release);

    let reclaimed_frames = reclaim_loader();
    let reclaimed_bytes = reclaimed_frames.saturating_mul(frame::PAGE_SIZE);
    kprintln!(
        "paging: kernel tables active (HHDM {} MiB, reclaimed {} MiB)",
        hhdm_bytes >> 20,
        reclaimed_bytes >> 20
    );
    Ok(boot_info_ptr())
}

/// The active kernel root frame.
pub fn kernel_root() -> Option<PhysAddr> {
    let root = KERNEL_ROOT.load(Ordering::Acquire);
    (root != 0).then_some(PhysAddr(root))
}

/// The copied BootInfo.
pub fn boot_info_ptr() -> *const BootInfo {
    BOOT_INFO.0.get()
}

/// The copied memory map.
pub fn memory_map() -> &'static [MemoryRegion] {
    let length = usize::try_from(copied_info().memory_map_len).unwrap_or(0);
    let length = length.min(MAX_MEMORY_MAP);
    // SAFETY: the static array was filled during init and is immutable afterwards; `length` was
    // validated at copy time and is clamped to the array bound.
    unsafe { core::slice::from_raw_parts(MEMORY_MAP.0.get().cast::<MemoryRegion>(), length) }
}

/// The copied command line.
pub fn cmdline() -> &'static str {
    let info = copied_info();
    let length = usize::try_from(info.cmdline.len)
        .unwrap_or(0)
        .min(CMDLINE_MAX);
    // SAFETY: the bytes were copied from the UTF-8 command line validated by `boot::validate`
    // and are immutable after init; `length` is bounded by the static buffer.
    unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            CMDLINE.0.get().cast::<u8>(),
            length,
        ))
    }
}

/// Map `len` bytes of freshly allocated frames at `HEAP_BASE + offset` with RW+NX flags.
pub fn map_heap_range(offset: u64, len: u64) -> Result<(), MapError> {
    let length = page_align_up(len).ok_or(MapError::NotCanonical)?;
    let base = raven_abi::layout::x86_64::KERNEL_HEAP_BASE
        .checked_add(offset)
        .ok_or(MapError::NotCanonical)?;
    let mut mapped = 0u64;
    while mapped < length {
        let virt = VirtAddr::new(base.checked_add(mapped).ok_or(MapError::NotCanonical)?)
            .ok_or(MapError::NotCanonical)?;
        let frame = frame::alloc_zeroed_frame().ok_or(MapError::OutOfFrames)?;
        let result = with_kernel_mapper(|mapper| {
            mapper.map(
                virt,
                rmm_phys(frame),
                PageFlags::kernel_rw(),
                PageSize::Size4K,
            )
        })?;
        if let Err(error) = result {
            let _ = frame::free_frame(frame);
            return Err(error);
        }
        mapped = mapped.saturating_add(frame::PAGE_SIZE);
    }
    Ok(())
}

/// Count pages that are both writable and executable in the active kernel tables.
pub fn wx_violations() -> u64 {
    let Some(root) = kernel_root() else {
        return u64::MAX;
    };
    let memory = KernelPhysMemory;
    let mut count = 0u64;
    walk_wx(PageTable::new(root, 3), &memory, &mut count);
    count
}

/// Run the `selftest=paging` checks.
pub fn selftest() {
    let test_address = match VirtAddr::new(0x0000_0000_5000_0000) {
        Some(address) => address,
        None => {
            kprintln!("selftest: paging failed (bad test address)");
            crate::boot::exit_qemu(0x11);
        }
    };
    let frame = match frame::alloc_zeroed_frame() {
        Some(frame) => frame,
        None => {
            kprintln!("selftest: paging failed (out of frames)");
            crate::boot::exit_qemu(0x11);
        }
    };
    let result = with_kernel_mapper(|mapper| {
        mapper.map(
            test_address,
            rmm_phys(frame),
            PageFlags::user_rw(),
            PageSize::Size4K,
        )?;
        // SAFETY: the mapping was just installed in the active kernel page table and is writable
        // from ring 0; the address is page aligned and mapped for one byte.
        unsafe {
            (test_address.data() as *mut u8).write_volatile(0x5a);
        }
        // SAFETY: same mapping, read back the byte just written.
        let value = unsafe { (test_address.data() as *const u8).read_volatile() };
        if value != 0x5a {
            return Err(MapError::NotMapped);
        }
        mapper.protect(test_address, PageFlags::user_ro())?;
        let (physical, flags, size) = mapper.translate(test_address).ok_or(MapError::NotMapped)?;
        if physical != rmm_phys(frame) || size != PageSize::Size4K || flags.is_writable() {
            return Err(MapError::NotMapped);
        }
        mapper.unmap(test_address)?;
        Ok(())
    });
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) | Err(error) => {
            kprintln!("selftest: paging failed ({error})");
            crate::boot::exit_qemu(0x11);
        }
    }
    let _ = frame::free_frame(frame);
    kprintln!("selftest: paging ok");
    let violations = wx_violations();
    kprintln!("paging: wx-violations={violations}");
    if violations != 0 {
        crate::boot::exit_qemu(0x11);
    }
}

/// Free the user half of an address-space root, including the root frame.
///
/// Mapped data frames are not freed; their ownership belongs to the address space owner.
#[allow(dead_code)] // Consumed by `AddressSpace::drop` in S08/S24.
pub fn free_user_tables(root: PhysAddr) {
    let memory = KernelPhysMemory;
    let table = PageTable::new(root, 3);
    for index in 0..256 {
        let Some(entry) = table.entry(&memory, index) else {
            continue;
        };
        if entry.is_present() && !entry.is_huge() {
            free_table(entry.address(), 2, &memory);
        }
    }
    let _ = frame::free_frame(frame_phys(root));
}

#[allow(dead_code)] // Called by `free_user_tables`; reachable once S08/S24 use address spaces.
fn free_table(table_phys: PhysAddr, level: u8, memory: &KernelPhysMemory) {
    let table = PageTable::new(table_phys, level);
    if level > 0 {
        for index in 0..512 {
            let Some(entry) = table.entry(memory, index) else {
                continue;
            };
            if entry.is_present() && !entry.is_huge() {
                free_table(entry.address(), level.saturating_sub(1), memory);
            }
        }
    }
    let _ = frame::free_frame(frame_phys(table_phys));
}

fn walk_wx(table: PageTable, memory: &KernelPhysMemory, count: &mut u64) {
    for index in 0..512 {
        let Some(entry) = table.entry(memory, index) else {
            continue;
        };
        if !entry.is_present() {
            continue;
        }
        if table.level() == 0 || entry.is_huge() {
            let flags = entry.flags();
            if flags.is_writable() && flags.is_executable() {
                *count = count.saturating_add(1);
            }
        } else if let Some(child) = table.next(memory, index) {
            walk_wx(child, memory, count);
        }
    }
}

fn copy_boot_data(data: &BootData) -> Result<(), PagingError> {
    let cmdline_length = data.cmdline.len();
    if cmdline_length > CMDLINE_MAX {
        return Err(PagingError::CmdlineTooLong);
    }
    let map_length = data.memory_map.len();
    if map_length > MAX_MEMORY_MAP {
        return Err(PagingError::TooManyRegions);
    }

    let cmdline_virtual = CMDLINE.0.get() as u64;
    let map_virtual = MEMORY_MAP.0.get() as u64;
    let cmdline_physical = kernel_phys(&data.info, cmdline_virtual)?;
    let map_physical = kernel_phys(&data.info, map_virtual)?;

    // SAFETY: the destination statics are owned by the kernel and are larger than the copied
    // ranges (checked above); the source ranges were validated by `boot::validate`.
    unsafe {
        if cmdline_length != 0 {
            core::ptr::copy_nonoverlapping(
                data.cmdline.as_ptr(),
                CMDLINE.0.get().cast::<u8>(),
                cmdline_length,
            );
        }
        core::ptr::copy_nonoverlapping(
            data.memory_map.as_ptr(),
            MEMORY_MAP.0.get().cast::<MemoryRegion>(),
            map_length,
        );
    }

    let mut info = data.info;
    info.cmdline = if cmdline_length == 0 {
        PhysRange { base: 0, len: 0 }
    } else {
        PhysRange {
            base: cmdline_physical,
            len: cmdline_length as u64,
        }
    };
    info.memory_map_base = map_physical;
    info.memory_map_len = map_length as u64;
    // SAFETY: the cell is written once during single-CPU init before it is published.
    unsafe {
        BOOT_INFO.0.get().write(info);
    }
    Ok(())
}

fn copied_info() -> &'static BootInfo {
    // SAFETY: the cell is written once during init and only read afterwards.
    unsafe { &*BOOT_INFO.0.get() }
}

fn kernel_phys(info: &BootInfo, virtual_address: u64) -> Result<u64, PagingError> {
    virtual_address
        .checked_sub(info.kernel_virt_base)
        .and_then(|offset| info.kernel_phys_base.checked_add(offset))
        .ok_or(PagingError::BadKernelAddress)
}

fn reclaim_loader() -> u64 {
    let mut frames = 0u64;
    for region in memory_map() {
        if region.kind() == Some(MemoryKind::LoaderReclaimable) {
            frames = frames.saturating_add(frame::free_range(region.base, region.len));
        }
    }
    frames
}

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __boot_stack_bottom: u8;
    static __boot_stack_top: u8;
}

fn map_kernel_image(mapper: &mut KernelMapper<'_>, kernel_offset: u64) -> Result<(), PagingError> {
    let segments = [
        (
            core::ptr::addr_of!(__text_start) as u64,
            core::ptr::addr_of!(__text_end) as u64,
            PageFlags::kernel_rx(),
        ),
        (
            core::ptr::addr_of!(__rodata_start) as u64,
            core::ptr::addr_of!(__rodata_end) as u64,
            PageFlags::kernel_ro(),
        ),
        (
            core::ptr::addr_of!(__data_start) as u64,
            core::ptr::addr_of!(__data_end) as u64,
            PageFlags::kernel_rw(),
        ),
        (
            core::ptr::addr_of!(__bss_start) as u64,
            core::ptr::addr_of!(__bss_end) as u64,
            PageFlags::kernel_rw(),
        ),
    ];
    for (start, end, flags) in segments {
        map_segment(mapper, start, end, flags, kernel_offset)?;
    }

    // The boot stack lives inside `.bss` and is therefore covered by the writable data mapping.
    // A guard page below it would require unmapping unrelated `.bss` objects, so it is omitted.
    let stack_bottom = core::ptr::addr_of!(__boot_stack_bottom) as u64;
    let stack_top = core::ptr::addr_of!(__boot_stack_top) as u64;
    let bss_start = core::ptr::addr_of!(__bss_start) as u64;
    let bss_end = core::ptr::addr_of!(__bss_end) as u64;
    if stack_bottom < bss_start
        || stack_top > bss_end
        || stack_top <= stack_bottom
        || !stack_bottom.is_multiple_of(frame::PAGE_SIZE)
    {
        return Err(PagingError::BadKernelAddress);
    }
    Ok(())
}

fn map_segment(
    mapper: &mut KernelMapper<'_>,
    start: u64,
    end: u64,
    flags: PageFlags,
    kernel_offset: u64,
) -> Result<(), PagingError> {
    if end <= start {
        return Ok(());
    }
    let start = page_align_down(start);
    let end = page_align_up(end).ok_or(PagingError::Overflow)?;
    let length = end.checked_sub(start).ok_or(PagingError::Overflow)?;
    let physical = start
        .checked_sub(kernel_offset)
        .ok_or(PagingError::BadKernelAddress)?;
    let virt = VirtAddr::new(start).ok_or(MapError::NotCanonical)?;
    mapper
        .map_range(virt, rmm_phys(frame::PhysAddr(physical)), length, flags)
        .map_err(PagingError::Map)
}

fn activate(root: frame::PhysAddr) {
    // SAFETY: `wrmsr`/`rdmsr`/CR manipulation are ring-0 operations; the root was built by the
    // mapper, EFER.NXE is already set by the loader and is set idempotently here, and CR0.WP only
    // enables write protection for supervisor writes to read-only pages.
    unsafe {
        core::arch::asm!(
            "mov ecx, 0xC0000080",
            "rdmsr",
            "bts eax, 11",
            "wrmsr",
            out("eax") _,
            out("edx") _,
            out("ecx") _,
            options(nostack, preserves_flags),
        );
        let cr0: u64;
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        core::arch::asm!(
            "mov cr0, {}",
            in(reg) cr0 | (1u64 << 16),
            options(nomem, nostack, preserves_flags),
        );
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) root.0,
            options(nomem, nostack, preserves_flags),
        );
        core::arch::asm!(
            "mov rax, cr3",
            "mov cr3, rax",
            out("rax") _,
            options(nostack, preserves_flags),
        );
    }
}

fn rmm_phys(phys: frame::PhysAddr) -> PhysAddr {
    PhysAddr(phys.0)
}

fn frame_phys(phys: PhysAddr) -> frame::PhysAddr {
    frame::PhysAddr(phys.0)
}

const _: () = {
    assert!(CMDLINE_MAX == 1024);
    assert!(MAX_MEMORY_MAP == 512);
};
