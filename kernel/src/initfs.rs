// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Boot initfs access.
//!
//! The loader places the initfs in memory that stays `KernelAndModules`; paging keeps it mapped
//! through the HHDM. The image is parsed and validated once, then looked up without allocation.

use core::cell::UnsafeCell;

use raven_initfs::{Entry, Initfs};

use crate::memory::{frame, paging};

struct InitfsCell(UnsafeCell<Option<Initfs<'static>>>);

// SAFETY: the cell is written once during single-CPU boot and read-only afterwards.
unsafe impl Sync for InitfsCell {}

static INITFS: InitfsCell = InitfsCell(UnsafeCell::new(None));

/// Parse the initfs image from the copied BootInfo.
pub fn init() {
    // SAFETY: paging published the copied BootInfo before initfs initialisation.
    let info = unsafe { &*paging::boot_info_ptr() };
    if info.initfs.base == 0 || info.initfs.len == 0 {
        return;
    }
    let Some(length) = usize::try_from(info.initfs.len).ok() else {
        return;
    };
    let pointer = frame::phys_to_virt(frame::PhysAddr(info.initfs.base));
    if pointer.is_null() {
        return;
    }
    // SAFETY: the range is the loader-provided initfs copy, remains mapped as KernelAndModules,
    // and the length was validated by the boot handoff.
    let bytes = unsafe { core::slice::from_raw_parts(pointer, length) };
    if let Ok(parsed) = Initfs::parse(bytes) {
        // SAFETY: the cell is written once during boot before any lookup.
        unsafe {
            INITFS.0.get().write(Some(parsed));
        }
    }
}

/// Look up a path in the parsed initfs; `None` when the image is missing or malformed.
pub fn lookup(path: &str) -> Option<Entry<'static>> {
    // SAFETY: the cell is written once during boot and only read afterwards.
    unsafe { (*INITFS.0.get()).and_then(|initfs| initfs.get(path)) }
}
