// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The raw `PageProvider` implementation over `MAP`/`UNMAP`/`PROTECT`.
//!
//! The method set matches `raven-libc-malloc`'s `PageProvider` (C49 section 4.2) exactly; the
//! trait itself lives in that crate, so `raven-libc-platform` (C55) wraps this type in a local
//! newtype and delegates. `raven-libc-sys` cannot depend on `raven-libc-malloc`.
//!
//! Anonymous memory uses `Handle::NONE`; alignment above the page size is satisfied by
//! over-mapping and trimming the head and tail with `UNMAP` (PG-13 `map::REPLACE` is not
//! assumed). Every mapping created here is recorded in a small fixed registry so `unmap` only
//! releases whole mappings this provider created.

#![allow(clippy::arithmetic_side_effects)] // page/address arithmetic, validated before use

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use raven_abi::types::{Handle, map as map_flags, prot};

use crate::sys;

/// The page size asserted by C49 at startup.
pub const DEFAULT_PAGE_SIZE: usize = raven_abi::PAGE_SIZE;
/// Maximum live mappings tracked by the provider.
pub const MAX_TRACKED: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MapRecord {
    addr: usize,
    len: usize,
}

struct Registry {
    locked: AtomicBool,
    entries: UnsafeCell<[Option<MapRecord>; MAX_TRACKED]>,
}

// SAFETY: every access to `entries` happens while `locked` is held; the lock is released with
// Release ordering after the mutation.
unsafe impl Sync for Registry {}

static REGISTRY: Registry = Registry {
    locked: AtomicBool::new(false),
    entries: UnsafeCell::new([None; MAX_TRACKED]),
};

fn with_registry<R>(f: impl FnOnce(&mut [Option<MapRecord>; MAX_TRACKED]) -> R) -> R {
    while REGISTRY.locked.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: the spin lock gives this closure exclusive access to the array.
    let entries = unsafe { &mut *REGISTRY.entries.get() };
    let result = f(entries);
    REGISTRY.locked.store(false, Ordering::Release);
    result
}

fn record_mapping(addr: usize, len: usize) -> bool {
    with_registry(|entries| {
        for slot in entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(MapRecord { addr, len });
                return true;
            }
        }
        false
    })
}

fn forget_mapping(addr: usize, len: usize) -> bool {
    with_registry(|entries| {
        for slot in entries.iter_mut() {
            if *slot == Some(MapRecord { addr, len }) {
                *slot = None;
                return true;
            }
        }
        false
    })
}

fn contains_range(addr: usize, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    with_registry(|entries| {
        entries.iter().flatten().any(|record| {
            let end = record.addr.checked_add(record.len);
            let wanted = addr.checked_add(len);
            match (end, wanted) {
                (Some(end), Some(wanted)) => addr >= record.addr && wanted <= end,
                _ => false,
            }
        })
    })
}

/// Number of live records; used by tests and diagnostics.
pub fn tracked_mappings() -> usize {
    with_registry(|entries| entries.iter().flatten().count())
}

fn round_up(value: usize, page: usize) -> Option<usize> {
    if page == 0 {
        return None;
    }
    let mask = page - 1;
    value.checked_add(mask).map(|sum| sum & !mask)
}

fn page_aligned(value: usize, page: usize) -> bool {
    page != 0 && value.is_multiple_of(page)
}

/// The production provider.
#[derive(Clone, Copy, Debug)]
pub struct SysPageProvider {
    page_size: usize,
}

impl SysPageProvider {
    /// The provider with the ABI page size (4096).
    pub const fn new() -> Self {
        Self {
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// The provider for an explicit `AT_PAGESZ`; rejects anything but 4096 (PG-53).
    pub fn with_page_size(page_size: usize) -> Option<Self> {
        if page_size == DEFAULT_PAGE_SIZE {
            Some(Self { page_size })
        } else {
            None
        }
    }

    /// Build the provider from the startup auxiliary vector.
    pub fn from_auxv(auxv: &crate::auxv::AuxVec) -> Option<Self> {
        Self::with_page_size(auxv.pagesz()?)
    }

    /// The platform page size.
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Map zero-filled memory of at least `align` alignment and `prot` protection.
    ///
    /// Returns the address, or `None` on failure.
    pub fn map_pages(&self, len: usize, align: usize, prot_bits: u8) -> Option<usize> {
        if len == 0 || align == 0 || !align.is_power_of_two() {
            return None;
        }
        let page = self.page_size;
        let len = round_up(len, page)?;
        let align = align.max(page);
        let protection = u32::from(prot_bits & (prot::MASK as u8));

        if align <= page {
            let addr = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, len, protection, 0).ok()?;
            if !record_mapping(addr, len) {
                let _ = sys::unmap(Handle::SELF_PROC, addr, len);
                return None;
            }
            return Some(addr);
        }

        let over = len.checked_add(align)?;
        let base = sys::map(Handle::SELF_PROC, Handle::NONE, 0, 0, over, protection, 0).ok()?;
        let aligned = base.checked_add(align - 1)? & !(align - 1);
        let head = aligned.checked_sub(base)?;
        let tail = over.checked_sub(head)?.checked_sub(len)?;

        if head != 0 && sys::unmap(Handle::SELF_PROC, base, head).is_err() {
            let _ = sys::unmap(Handle::SELF_PROC, base, over);
            return None;
        }
        if tail != 0 && sys::unmap(Handle::SELF_PROC, aligned + len, tail).is_err() {
            let _ = sys::unmap(Handle::SELF_PROC, aligned, len);
            return None;
        }
        if !record_mapping(aligned, len) {
            let _ = sys::unmap(Handle::SELF_PROC, aligned, len);
            return None;
        }
        Some(aligned)
    }

    /// Map exactly `hint..hint + len`; fail if the range is busy (NOREPLACE).
    pub fn map_exact(&self, hint: usize, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let page = self.page_size;
        if !page_aligned(hint, page) {
            return None;
        }
        let len = round_up(len, page)?;
        let addr = sys::map(
            Handle::SELF_PROC,
            Handle::NONE,
            0,
            hint,
            len,
            (prot::READ | prot::WRITE) as u32,
            map_flags::FIXED as u32,
        )
        .ok()?;
        if !record_mapping(addr, len) {
            let _ = sys::unmap(Handle::SELF_PROC, addr, len);
            return None;
        }
        Some(addr)
    }

    /// Release a mapping previously returned by this provider.
    pub fn unmap(&self, addr: usize, len: usize) -> bool {
        if len == 0 || !forget_mapping(addr, len) {
            return false;
        }
        if sys::unmap(Handle::SELF_PROC, addr, len).is_ok() {
            return true;
        }
        // Put the record back so a retry can still find the mapping.
        let _ = record_mapping(addr, len);
        false
    }

    /// Change the protection of a mapped range.
    pub fn protect(&self, addr: usize, len: usize, prot_bits: u8) -> bool {
        if len == 0 || !contains_range(addr, len) {
            return false;
        }
        let protection = u32::from(prot_bits & (prot::MASK as u8));
        sys::protect(Handle::SELF_PROC, addr, len, protection).is_ok()
    }

    /// `mimmutable`: a no-op by default (A-malloc-3).
    pub fn seal(&self, _addr: usize, _len: usize) -> bool {
        true
    }
}

impl Default for SysPageProvider {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = assert!(DEFAULT_PAGE_SIZE == 4096);
const _: () = assert!(MAX_TRACKED > 0);

#[cfg(test)]
#[path = "pages_ut.rs"]
mod tests;
