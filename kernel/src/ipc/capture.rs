// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Zero-copy capture of a client buffer into a server's address space.
//!
//! The client's present pages are translated through its page tables and mapped into the server
//! at a reserved window. The kernel copies no payload; the packet carries the server-side virtual
//! address. The mapping is removed when the request completes or the server dies.
//!
//! The client must not unmap the range while a request is in flight. A single-threaded client is
//! safe because it blocks in the syscall; multi-threaded clients need a future per-range pin in
//! `UserAddressSpace` (documented in the report).

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::prot;
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};

use crate::process::Process;

use super::protocol::MAX_TRANSFER;

/// First virtual address of the server capture window.
pub const CAPTURE_BASE: u64 = 0x0000_6000_0000_0000;
/// Size of one capture slot.
pub const CAPTURE_SLOT_SIZE: u64 = 4 * 1024 * 1024;
/// Number of capture slots per process.
pub const CAPTURE_SLOTS: usize = 8;
/// Page size.
const PAGE: u64 = 4096;

/// Per-process allocation state for capture slots.
pub struct CaptureSlots {
    used: [bool; CAPTURE_SLOTS],
}

impl CaptureSlots {
    /// Empty allocator.
    pub const fn new() -> Self {
        Self {
            used: [false; CAPTURE_SLOTS],
        }
    }

    /// Allocate one slot large enough for `pages` pages.
    pub fn acquire(&mut self, pages: u64) -> Option<usize> {
        let required = pages.saturating_mul(PAGE);
        if required > CAPTURE_SLOT_SIZE {
            return None;
        }
        let mut index = 0usize;
        while index < CAPTURE_SLOTS {
            if self.used.get(index) == Some(&false) {
                if let Some(slot) = self.used.get_mut(index) {
                    *slot = true;
                }
                return Some(index);
            }
            index = index.saturating_add(1);
        }
        None
    }

    /// Release a slot.
    pub fn release(&mut self, index: usize) {
        if let Some(slot) = self.used.get_mut(index) {
            *slot = false;
        }
    }
}

impl Default for CaptureSlots {
    fn default() -> Self {
        Self::new()
    }
}

/// One live capture.
pub struct Capture {
    slot: Option<usize>,
    server: Arc<Process>,
    address: u64,
    base: u64,
    pages: u64,
    mapped: bool,
}

impl Capture {
    /// Server-side virtual address of the first requested byte.
    pub fn address(&self) -> u64 {
        self.address
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if self.mapped {
            self.mapped = false;
            let mut address_space = self.server.address_space.lock();
            if let Some(address_space) = address_space.as_mut()
                && let Ok(mut mapper) = address_space.mapper()
            {
                let mut page = 0u64;
                while page < self.pages {
                    let address = self.base.saturating_add(page.saturating_mul(PAGE));
                    if let Some(virt) = VirtAddr::new(address) {
                        let _ = mapper.unmap(virt);
                    }
                    page = page.saturating_add(1);
                }
            }
        }
        if let Some(slot) = self.slot.take() {
            self.server.capture_slots.lock().release(slot);
        }
    }
}

/// Map `client_addr..client_addr + len` into `server`.
///
/// `writable` is true when the server writes into the client buffer (`Read`), false when it only
/// reads it (`Write`/`Call` request). Returns the capture or an errno.
pub fn create(
    client: &Arc<Process>,
    server: &Arc<Process>,
    client_addr: u64,
    len: u64,
    writable: bool,
) -> Result<Capture, Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    if len > MAX_TRANSFER {
        return Err(Errno::E2BIG);
    }
    if Arc::ptr_eq(client, server) {
        return Ok(Capture {
            slot: None,
            server: Arc::clone(server),
            address: client_addr,
            base: 0,
            pages: 0,
            mapped: false,
        });
    }

    let start = client_addr & !(PAGE - 1);
    let offset = client_addr.saturating_sub(start);
    let span = offset
        .checked_add(len)
        .and_then(|value| value.checked_add(PAGE - 1))
        .map(|value| value & !(PAGE - 1))
        .ok_or(Errno::EINVAL)?;
    let pages = span / PAGE;
    if pages == 0 || pages.saturating_mul(PAGE) > CAPTURE_SLOT_SIZE {
        return Err(Errno::E2BIG);
    }

    let mut physical = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
    {
        let mut client_space = client.address_space.lock();
        let Some(address_space) = client_space.as_mut() else {
            return Err(Errno::ESRCH);
        };
        if !address_space.vmas().is_fully_mapped(start, span) {
            return Err(Errno::EFAULT);
        }
        let mut page = 0u64;
        while page < pages {
            let address = start.saturating_add(page.saturating_mul(PAGE));
            let Some(vma) = address_space.vmas().find(address) else {
                return Err(Errno::EFAULT);
            };
            if vma.prot() & (prot::READ as u8) == 0 {
                return Err(Errno::EFAULT);
            }
            if writable && vma.prot() & (prot::WRITE as u8) == 0 {
                return Err(Errno::EFAULT);
            }
            page = page.saturating_add(1);
        }
        let mut mapper = address_space.mapper().map_err(map_errno)?;
        page = 0;
        while page < pages {
            let address = start.saturating_add(page.saturating_mul(PAGE));
            let virt = VirtAddr::new(address).ok_or(Errno::EINVAL)?;
            let Some((frame, flags, _)) = mapper.translate(virt) else {
                return Err(Errno::EFAULT);
            };
            if writable && !flags.is_writable() {
                return Err(Errno::EFAULT);
            }
            physical.push(frame.0);
            page = page.saturating_add(1);
        }
    }

    let slot = server
        .capture_slots
        .lock()
        .acquire(pages)
        .ok_or(Errno::ENOMEM)?;
    let base = CAPTURE_BASE.saturating_add((slot as u64).saturating_mul(CAPTURE_SLOT_SIZE));

    let mut address_space = server.address_space.lock();
    let Some(address_space) = address_space.as_mut() else {
        server.capture_slots.lock().release(slot);
        return Err(Errno::ESRCH);
    };
    if !address_space.vmas().is_free(base, CAPTURE_SLOT_SIZE) {
        server.capture_slots.lock().release(slot);
        return Err(Errno::EBUSY);
    }
    let flags = if writable {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    };
    let mut mapper = match address_space.mapper() {
        Ok(mapper) => mapper,
        Err(error) => {
            server.capture_slots.lock().release(slot);
            return Err(map_errno(error));
        }
    };
    let mut page = 0u64;
    while page < pages {
        let address = base.saturating_add(page.saturating_mul(PAGE));
        let Some(virt) = VirtAddr::new(address) else {
            unmap_partial(&mut mapper, base, page);
            server.capture_slots.lock().release(slot);
            return Err(Errno::EINVAL);
        };
        let Some(frame) = physical.get(usize::try_from(page).unwrap_or(usize::MAX)) else {
            unmap_partial(&mut mapper, base, page);
            server.capture_slots.lock().release(slot);
            return Err(Errno::EFAULT);
        };
        if mapper
            .map(virt, PhysAddr(*frame), flags, PageSize::Size4K)
            .is_err()
        {
            unmap_partial(&mut mapper, base, page);
            server.capture_slots.lock().release(slot);
            return Err(Errno::ENOMEM);
        }
        page = page.saturating_add(1);
    }
    Ok(Capture {
        slot: Some(slot),
        server: Arc::clone(server),
        address: base.saturating_add(offset),
        base,
        pages,
        mapped: true,
    })
}

fn unmap_partial(
    mapper: &mut raven_rmm::Mapper<
        '_,
        crate::memory::paging::KernelFrameAllocator,
        crate::memory::paging::KernelPhysMemory,
        crate::memory::paging::KernelTlb,
    >,
    base: u64,
    pages: u64,
) {
    let mut page = 0u64;
    while page < pages {
        let address = base.saturating_add(page.saturating_mul(PAGE));
        if let Some(virt) = VirtAddr::new(address) {
            let _ = mapper.unmap(virt);
        }
        page = page.saturating_add(1);
    }
}

fn map_errno(error: MapError) -> Errno {
    match error {
        MapError::OutOfFrames => Errno::ENOMEM,
        MapError::NotCanonical | MapError::Misaligned => Errno::EINVAL,
        _ => Errno::ENOMEM,
    }
}

const _: () = {
    assert!(CAPTURE_SLOT_SIZE == 4 * 1024 * 1024);
    assert!(CAPTURE_SLOTS > 0);
    assert!(CAPTURE_BASE < raven_abi::layout::x86_64::USER_END);
};
