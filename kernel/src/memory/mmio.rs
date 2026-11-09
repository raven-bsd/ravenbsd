// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Uncached kernel mappings for device MMIO.
//!
//! S07 uses [`map_mmio`] to reach device registers. The mappings live in a dedicated 64 GiB
//! window above the growable heap part of the kernel heap region and are never unmapped.

#![allow(dead_code)] // S07 maps device registers through this module.

use raven_abi::layout::x86_64::KERNEL_HEAP_BASE;
use raven_rmm::{
    MapError, PageFlags, PageSize, PhysAddr, VirtAddr, page_align_down, page_align_up,
};
use spin::Mutex;

use crate::memory::heap::MMIO_WINDOW_OFFSET;
use crate::memory::paging;

/// Size of the MMIO bump window.
const MMIO_WINDOW_SIZE: u64 = 64 * 1024 * 1024 * 1024;
/// First virtual address of the MMIO window.
const MMIO_START: u64 = KERNEL_HEAP_BASE.saturating_add(MMIO_WINDOW_OFFSET);
/// One past the last MMIO window address.
const MMIO_END: u64 = MMIO_START.saturating_add(MMIO_WINDOW_SIZE);

static CURSOR: Mutex<u64> = Mutex::new(MMIO_START);

/// Map `len` bytes of physical MMIO at the next free window address and return a pointer to the
/// first requested byte.
///
/// On a mapping error the bump cursor is not rolled back; the partially created mappings remain.
pub fn map_mmio(physical: u64, len: usize) -> Result<*mut u8, MapError> {
    if len == 0 {
        return Err(MapError::Misaligned);
    }
    let len = u64::try_from(len).map_err(|_| MapError::NotCanonical)?;
    let base = page_align_down(physical);
    let offset = physical.checked_sub(base).ok_or(MapError::NotCanonical)?;
    let span = offset.checked_add(len).ok_or(MapError::NotCanonical)?;
    let map_len = page_align_up(span).ok_or(MapError::NotCanonical)?;

    let mut cursor = CURSOR.lock();
    let start = *cursor;
    let end = start.checked_add(map_len).ok_or(MapError::NotCanonical)?;
    if end > MMIO_END {
        return Err(MapError::OutOfFrames);
    }
    *cursor = end;
    drop(cursor);

    let pages = map_len >> 12;
    for page in 0..pages {
        let virtual_address = start
            .checked_add(page << 12)
            .ok_or(MapError::NotCanonical)?;
        let physical_address = base.checked_add(page << 12).ok_or(MapError::NotCanonical)?;
        let virt = VirtAddr::new(virtual_address).ok_or(MapError::NotCanonical)?;
        paging::with_kernel_mapper(|mapper| {
            mapper.map(
                virt,
                PhysAddr(physical_address),
                PageFlags::mmio(),
                PageSize::Size4K,
            )
        })??;
    }

    let address = start.checked_add(offset).ok_or(MapError::NotCanonical)?;
    Ok(address as *mut u8)
}

const _: () = {
    assert!(MMIO_WINDOW_SIZE == 64 * 1024 * 1024 * 1024);
    assert!(MMIO_START < MMIO_END);
};
