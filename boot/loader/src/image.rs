// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Loading the kernel ELF into `LOADER_DATA` pages (CONTRACTS C9).
//!
//! S03 consumes [`LoadedKernel`]: the loader keeps the kernel's relative segment layout in one
//! page-aligned allocation and records where each segment ended up.

use alloc::vec::Vec;
use core::fmt;

use raven_elf::{Elf, ElfError, Machine, Segment};
use uefi::boot::{self, AllocateType, MemoryType};

/// Page size used for kernel allocations.
const PAGE_SIZE: u64 = 4096;

/// ELF machine expected for the loader's target architecture.
#[cfg(target_arch = "x86_64")]
const MACHINE: Machine = Machine::X86_64;
/// ELF machine expected for the loader's target architecture.
#[cfg(target_arch = "aarch64")]
const MACHINE: Machine = Machine::Aarch64;

/// One loaded `PT_LOAD` segment.
#[derive(Clone, Copy, Debug)]
pub struct LoadedSegment {
    /// Virtual address from the ELF.
    pub vaddr: u64,
    /// Physical address of the segment in the loader allocation.
    pub paddr: u64,
    /// Segment size in memory, including the zero-filled tail.
    pub len: u64,
    /// RavenBSD protection bits: `1 = read`, `2 = write`, `4 = execute`.
    pub prot: u8,
}

/// A kernel image placed in UEFI loader memory.
#[derive(Debug)]
pub struct LoadedKernel {
    /// Physical base of the allocation.
    pub phys_base: u64,
    /// Virtual base (the first `PT_LOAD` segment's virtual address).
    pub virt_base: u64,
    /// Page-rounded size of the loaded image in bytes.
    pub size: u64,
    /// ELF entry point.
    pub entry: u64,
    /// Loaded segments in ascending virtual address order.
    pub segments: Vec<LoadedSegment>,
}

/// Why a kernel image could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// `raven-elf` rejected the file.
    Elf(ElfError),
    /// The ELF has no `PT_LOAD` segments.
    NoSegments,
    /// The image span or segment offset does not fit the loader's counters.
    Layout,
    /// UEFI could not allocate the image pages.
    Allocation(uefi::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Elf(error) => write!(f, "invalid ELF: {error}"),
            LoadError::NoSegments => f.write_str("ELF has no loadable segments"),
            LoadError::Layout => f.write_str("ELF segment layout is not representable"),
            LoadError::Allocation(error) => write!(f, "cannot allocate kernel pages: {error}"),
        }
    }
}

impl From<ElfError> for LoadError {
    fn from(error: ElfError) -> Self {
        LoadError::Elf(error)
    }
}

/// Parse and load a kernel ELF into freshly allocated, zeroed `LOADER_DATA` pages.
pub fn load(data: &[u8]) -> Result<LoadedKernel, LoadError> {
    let elf = Elf::parse(data, MACHINE, crate::layout::KERNEL_RANGE)?;

    let mut iterator = elf.segments();
    let first: Segment = iterator.next().ok_or(LoadError::NoSegments)?;
    let virt_base = first.vaddr;
    let mut image_end = first
        .vaddr
        .checked_add(first.memsz)
        .ok_or(LoadError::Layout)?;
    let mut segments = Vec::new();
    segments.push(first);
    for segment in iterator {
        image_end = image_end.max(
            segment
                .vaddr
                .checked_add(segment.memsz)
                .ok_or(LoadError::Layout)?,
        );
        segments.push(segment);
    }

    let span = image_end.checked_sub(virt_base).ok_or(LoadError::Layout)?;
    let size = span.checked_add(PAGE_SIZE - 1).ok_or(LoadError::Layout)? & !(PAGE_SIZE - 1);
    let pages = usize::try_from(size / PAGE_SIZE).map_err(|_| LoadError::Layout)?;
    if pages == 0 {
        return Err(LoadError::Layout);
    }

    let allocation = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(LoadError::Allocation)?;
    let base = allocation.as_ptr();
    let phys_base = base as usize as u64;

    // SAFETY: `allocation` has `pages * PAGE_SIZE == size` valid bytes and is owned by the loader
    // for the rest of the boot. Zeroing the whole image makes every BSS tail zero before the file
    // bytes are copied over the front of each segment.
    unsafe {
        core::ptr::write_bytes(
            base,
            0,
            usize::try_from(size).map_err(|_| LoadError::Layout)?,
        );
    }

    let mut loaded = Vec::with_capacity(segments.len());
    for segment in &segments {
        let offset = segment
            .vaddr
            .checked_sub(virt_base)
            .ok_or(LoadError::Layout)?;
        let offset = usize::try_from(offset).map_err(|_| LoadError::Layout)?;
        let file_bytes = elf.segment_data(segment);
        let file_end = offset
            .checked_add(file_bytes.len())
            .ok_or(LoadError::Layout)?;
        if file_end > usize::try_from(size).map_err(|_| LoadError::Layout)? {
            return Err(LoadError::Layout);
        }

        // SAFETY: the range was checked against the allocation size, the source slice belongs to
        // the input ELF and does not overlap the UEFI allocation.
        unsafe {
            core::ptr::copy_nonoverlapping(file_bytes.as_ptr(), base.add(offset), file_bytes.len());
        }

        loaded.push(LoadedSegment {
            vaddr: segment.vaddr,
            paddr: phys_base
                .checked_add(u64::try_from(offset).map_err(|_| LoadError::Layout)?)
                .ok_or(LoadError::Layout)?,
            len: segment.memsz,
            prot: segment.prot,
        });
    }

    Ok(LoadedKernel {
        phys_base,
        virt_base,
        size,
        entry: elf.entry(),
        segments: loaded,
    })
}
