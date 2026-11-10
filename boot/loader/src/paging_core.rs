// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Page-table builder contract shared by the x86-64 and aarch64 implementations.
//!
//! The builders are generic over [`PhysMem`] so the host tests can run them against a `Vec<u8>`
//! backed fake instead of UEFI allocations. [`UefiPhysMem`] is the real frame pool: one contiguous
//! `LOADER_DATA` allocation whose frames are handed out in order.

use core::fmt;

/// Size of one 4 KiB page.
pub const PAGE_SIZE: u64 = 4096;
/// Size of one 2 MiB huge page / block.
pub const PAGE_SIZE_2M: u64 = 0x20_0000;
/// Size of one 1 GiB huge page / block.
pub const PAGE_SIZE_1G: u64 = 0x4000_0000;
/// Architectural maximum physical address width accepted by either builder.
pub const MAX_PHYSICAL_BITS: u32 = 52;
/// Minimum address width that can describe a 4 KiB page.
pub const MIN_PHYSICAL_BITS: u32 = 12;

/// Why a page-table operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageError {
    /// The frame pool is exhausted.
    OutOfFrames,
    /// A physical address or alignment is invalid.
    InvalidAddress,
    /// The virtual address is already mapped.
    AlreadyMapped,
    /// A leaf would be both writable and executable.
    WritableAndExecutable,
    /// A huge page was found where a table was expected.
    HugePage,
    /// UEFI could not allocate the frame pool.
    Allocation,
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageError::OutOfFrames => f.write_str("page-table frame pool exhausted"),
            PageError::InvalidAddress => f.write_str("invalid page-table address"),
            PageError::AlreadyMapped => f.write_str("virtual address already mapped"),
            PageError::WritableAndExecutable => f.write_str("refusing a writable+executable page"),
            PageError::HugePage => f.write_str("unexpected huge page in a table walk"),
            PageError::Allocation => f.write_str("cannot allocate page-table frames"),
        }
    }
}

/// Physical memory used by the page-table builders.
pub trait PhysMem {
    /// Allocate one zeroed 4 KiB frame and return its physical address.
    fn allocate_frame(&mut self) -> Result<u64, PageError>;
    /// Read one little-endian `u64` from a physical address.
    fn read_u64(&self, physical: u64) -> Option<u64>;
    /// Write one little-endian `u64` to a physical address.
    fn write_u64(&mut self, physical: u64, value: u64) -> bool;
    /// Zero one 4 KiB frame.
    fn zero_frame(&mut self, physical: u64) -> bool;
}

/// Real frame pool backed by one UEFI `LOADER_DATA` allocation.
#[cfg(target_os = "uefi")]
pub struct UefiPhysMem {
    base: u64,
    next: u64,
    end: u64,
}

#[cfg(target_os = "uefi")]
impl UefiPhysMem {
    /// Allocate and zero a pool of `pages` frames.
    pub fn allocate(pages: usize) -> Result<Self, PageError> {
        let bytes = pages
            .checked_mul(PAGE_SIZE as usize)
            .ok_or(PageError::Allocation)?;
        let allocation = uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            uefi::boot::MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|_| PageError::Allocation)?;
        let base = allocation.as_ptr() as usize as u64;
        // SAFETY: the UEFI allocation is exactly `bytes` bytes and the loader owns it.
        unsafe {
            core::ptr::write_bytes(allocation.as_ptr(), 0, bytes);
        }
        Ok(Self {
            base,
            next: base,
            end: base
                .checked_add(bytes as u64)
                .ok_or(PageError::Allocation)?,
        })
    }

    /// Physical base of the pool.
    pub fn base(&self) -> u64 {
        self.base
    }

    /// Size of the pool in bytes.
    pub fn size(&self) -> u64 {
        self.end - self.base
    }
}

#[cfg(target_os = "uefi")]
impl PhysMem for UefiPhysMem {
    fn allocate_frame(&mut self) -> Result<u64, PageError> {
        let frame = self.next;
        let next = frame.checked_add(PAGE_SIZE).ok_or(PageError::OutOfFrames)?;
        if next > self.end {
            return Err(PageError::OutOfFrames);
        }
        self.next = next;
        if !self.zero_frame(frame) {
            return Err(PageError::InvalidAddress);
        }
        Ok(frame)
    }

    fn read_u64(&self, physical: u64) -> Option<u64> {
        if physical < self.base
            || !physical.is_multiple_of(8)
            || physical.checked_add(8)? > self.end
        {
            return None;
        }
        // SAFETY: the address is inside the pool, which is identity-mapped while boot services
        // are active, and the loader owns the allocation.
        Some(unsafe { (physical as *const u64).read() })
    }

    fn write_u64(&mut self, physical: u64, value: u64) -> bool {
        if physical < self.base
            || !physical.is_multiple_of(8)
            || physical.checked_add(8).is_none_or(|end| end > self.end)
        {
            return false;
        }
        // SAFETY: the address is inside the pool, which is identity-mapped while boot services
        // are active, and the loader owns the allocation.
        unsafe {
            (physical as *mut u64).write(value);
        }
        true
    }

    fn zero_frame(&mut self, physical: u64) -> bool {
        if physical < self.base
            || physical
                .checked_add(PAGE_SIZE)
                .is_none_or(|end| end > self.end)
        {
            return false;
        }
        // SAFETY: the frame is inside the pool and owned by the loader.
        unsafe {
            core::ptr::write_bytes(physical as *mut u8, 0, PAGE_SIZE as usize);
        }
        true
    }
}

/// Inputs shared by the architecture-specific loader page-table builders.
#[cfg(target_os = "uefi")]
pub struct LoaderMap<'a> {
    /// Frame pool.
    pub pool: UefiPhysMem,
    /// Converted memory map.
    pub regions: &'a [raven_bootinfo::MemoryRegion],
    /// Loaded kernel image.
    pub kernel: &'a crate::image::LoadedKernel,
    /// Loader image physical base (identity-mapped).
    pub image_base: u64,
    /// Loader image size.
    pub image_size: u64,
    /// Firmware stack window to identity-map.
    pub old_stack_start: u64,
    /// End of the firmware stack window.
    pub old_stack_end: u64,
    /// New kernel stack physical base (mapped in HHDM).
    pub stack_base_phys: u64,
    /// New kernel stack size in bytes.
    pub stack_size: u64,
}
