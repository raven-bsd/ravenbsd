// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Copy-backed capture of a client buffer into a server's address space.
//!
//! A cross-process capture allocates kernel-owned frames, copies the client range into them (so
//! the server never maps client memory), maps those frames into the server at a reserved window
//! and copies them back to the still-mapped client pages when a writable request completes. The
//! capture owns its frames until the request is retired; a client that unmaps its range only
//! loses the copy-back, never the server's snapshot.
//!
//! Client cancellation keeps a delivered capture mapped until the server exits or a queued
//! cancelled entry is skipped by `RECV`, so a server that is still touching the buffer sees
//! live kernel-owned memory. Server death releases the frames after the client has been woken.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::prot;
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};

use crate::memory::frame;
use crate::memory::uspace::capture_window;
use crate::process::Process;

use super::protocol::MAX_TRANSFER;

/// Page size.
const PAGE: u64 = 4096;

/// Kernel-owned capture frames currently in use (debug leak probe).
static CAPTURE_FRAMES: AtomicU64 = AtomicU64::new(0);

/// Number of capture frames currently owned by live captures.
pub fn frames_in_use() -> u64 {
    CAPTURE_FRAMES.load(Ordering::Acquire)
}

/// Per-process allocation state for capture slots.
pub struct CaptureSlots {
    used: [bool; capture_window::SLOTS],
}

impl CaptureSlots {
    /// Empty allocator.
    pub const fn new() -> Self {
        Self {
            used: [false; capture_window::SLOTS],
        }
    }

    /// Allocate one slot large enough for `pages` pages.
    pub fn acquire(&mut self, pages: u64) -> Option<usize> {
        let required = pages.saturating_mul(PAGE);
        if required > capture_window::SLOT_SIZE {
            return None;
        }
        let mut index = 0usize;
        while index < capture_window::SLOTS {
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

/// Kernel-owned frames for one capture.
struct OwnedFrames {
    frames: Vec<u64>,
}

impl OwnedFrames {
    /// Empty set (same-process captures).
    const fn empty() -> Self {
        Self { frames: Vec::new() }
    }

    /// Allocate `pages` zeroed frames. On failure the partial set is released by `Drop`.
    fn allocate(pages: u64) -> Result<Self, Errno> {
        let count = usize::try_from(pages).map_err(|_| Errno::ENOMEM)?;
        let mut owned = Self {
            frames: Vec::with_capacity(count),
        };
        let mut page = 0u64;
        while page < pages {
            let Some(physical) = frame::alloc_zeroed_frame() else {
                return Err(Errno::ENOMEM);
            };
            owned.frames.push(physical.0);
            CAPTURE_FRAMES.fetch_add(1, Ordering::AcqRel);
            page = page.saturating_add(1);
        }
        Ok(owned)
    }

    fn physical(&self, page: u64) -> Option<u64> {
        self.frames.get(usize::try_from(page).ok()?).copied()
    }

    /// Keep the frames and their counter in place after a failed TLB shootdown. Freeing them
    /// while a stale translation may still reach them is worse than leaking them.
    fn leak(&mut self) {
        self.frames.clear();
    }
}

impl Drop for OwnedFrames {
    fn drop(&mut self) {
        for physical in self.frames.drain(..) {
            let _ = frame::free_frame(frame::PhysAddr(physical));
            CAPTURE_FRAMES.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// One live capture.
pub struct Capture {
    slot: Option<usize>,
    client: Arc<Process>,
    server: Arc<Process>,
    /// Server-side virtual address of the first requested byte.
    address: u64,
    /// Page-aligned server slot base.
    base: u64,
    /// Page-aligned client address of the first captured byte.
    client_start: u64,
    pages: u64,
    frames: OwnedFrames,
    /// Client physical frames observed at create; copy-back only touches the same frame.
    client_frames: Vec<u64>,
    /// Client address-space mapping generation at create.
    client_generation: u64,
    /// The server may write the capture and the frames are copied back on completion.
    writable: bool,
    /// Copy-back is still owed (cleared by completion or cancellation).
    copy_back_pending: bool,
    /// The frames are mapped into the server at `base`.
    mapped: bool,
}

impl Capture {
    /// Server-side virtual address of the first requested byte.
    pub fn address(&self) -> u64 {
        self.address
    }

    /// Copy the kernel frames back into the client range when it is still the same mapping.
    ///
    /// Pages the client unmapped or remapped during the request are skipped; the server always
    /// wrote to its own frames, so a skipped page never receives bytes from another mapping.
    pub fn copy_back(&mut self) {
        if !self.writable || !self.copy_back_pending || self.pages == 0 {
            self.copy_back_pending = false;
            return;
        }
        self.copy_back_pending = false;
        let mut space = self.client.address_space.lock();
        let Some(space) = space.as_mut() else {
            return;
        };
        if space.generation() != self.client_generation {
            // The client replaced (some of) its mapping while the request was in flight; do not
            // write the server's bytes into a new mapping that may have reused the frames.
            return;
        }
        let Ok(mut mapper) = space.mapper() else {
            return;
        };
        let mut page = 0u64;
        while page < self.pages {
            let Some(virt) =
                VirtAddr::new(self.client_start.saturating_add(page.saturating_mul(PAGE)))
            else {
                break;
            };
            let Some((physical, flags, _)) = mapper.translate(virt) else {
                page = page.saturating_add(1);
                continue;
            };
            if !flags.is_writable() {
                page = page.saturating_add(1);
                continue;
            }
            let Some(original) = self
                .client_frames
                .get(usize::try_from(page).unwrap_or(usize::MAX))
                .copied()
            else {
                page = page.saturating_add(1);
                continue;
            };
            if original != physical.0 {
                // The client replaced the page meanwhile; do not write into the new mapping.
                page = page.saturating_add(1);
                continue;
            }
            let Some(source) = self
                .frames
                .physical(page)
                .and_then(|value| non_null(frame::phys_to_virt(frame::PhysAddr(value))))
            else {
                page = page.saturating_add(1);
                continue;
            };
            let Some(destination) = non_null(frame::phys_to_virt(frame::PhysAddr(physical.0)))
            else {
                page = page.saturating_add(1);
                continue;
            };
            // SAFETY: both pointers refer to whole 4 KiB frames; the source is a live
            // kernel-owned capture frame and the destination is the client's current frame of
            // the original mapping.
            unsafe {
                core::ptr::copy_nonoverlapping(source, destination, PAGE as usize);
            }
            page = page.saturating_add(1);
        }
    }

    /// A cancelled client no longer wants the data; keep the mapping for the server but never
    /// copy back into the dead client's address space.
    pub fn cancel_copy_back(&mut self) {
        self.copy_back_pending = false;
    }
}

/// True when the S40 capture diagnostics are enabled.
fn trace_s40() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s40-teardowncheck")
}

impl Drop for Capture {
    fn drop(&mut self) {
        let mut released = true;
        if self.mapped && self.pages > 0 {
            let mut space = self.server.address_space.lock();
            if let Some(space) = space.as_mut() {
                match space.mapper() {
                    Ok(mut mapper) => {
                        let mut page = 0u64;
                        while page < self.pages {
                            let Some(virt) =
                                VirtAddr::new(self.base.saturating_add(page.saturating_mul(PAGE)))
                            else {
                                released = false;
                                break;
                            };
                            if mapper.unmap(virt).is_err() {
                                released = false;
                            }
                            page = page.saturating_add(1);
                        }
                    }
                    Err(_) => released = false,
                }
            }
        }
        if self.mapped && self.pages > 0 {
            let size = self.pages.saturating_mul(PAGE);
            let flushed = released && crate::sched::smp::shootdown::flush_range(self.base, size);
            if !flushed {
                if trace_s40() {
                    crate::kprintln!(
                        "s40: capture drop leak slot={:?} base={:#x} pages={} released={}",
                        self.slot,
                        self.base,
                        self.pages,
                        released
                    );
                }
                // A stale translation may still reach these frames; retain them and let the
                // capture counter expose the leak instead of recycling them.
                self.frames.leak();
            }
        }
        if let Some(slot) = self.slot.take() {
            self.server.capture_slots.lock().release(slot);
        }
    }
}

fn non_null(pointer: *mut u8) -> Option<*mut u8> {
    if pointer.is_null() {
        None
    } else {
        Some(pointer)
    }
}

/// Map `client_addr..client_addr + len` into `server`.
///
/// `writable` is true when the server writes into the client buffer (`Read`); the kernel frames
/// are copied back on completion if the client range is still the original mapping. For a
/// read-only capture (`Write`/`Call` request) the client range is copied in at create and the
/// server only ever reads the snapshot.
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
            client: Arc::clone(client),
            server: Arc::clone(server),
            address: client_addr,
            base: 0,
            client_start: 0,
            pages: 0,
            frames: OwnedFrames::empty(),
            client_frames: Vec::new(),
            client_generation: 0,
            writable: false,
            copy_back_pending: false,
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
    if pages == 0 || pages.saturating_mul(PAGE) > capture_window::SLOT_SIZE {
        return Err(Errno::E2BIG);
    }

    let mut client_frames = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
    let client_generation;
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
            client_frames.push(frame.0);
            page = page.saturating_add(1);
        }
        client_generation = address_space.generation();
    }

    let mut frames = OwnedFrames::allocate(pages)?;
    let mut page = 0u64;
    while page < pages {
        let Some(source) = client_frames
            .get(usize::try_from(page).unwrap_or(usize::MAX))
            .copied()
            .and_then(|value| non_null(frame::phys_to_virt(frame::PhysAddr(value))))
        else {
            return Err(Errno::EFAULT);
        };
        let Some(destination) = frames
            .physical(page)
            .and_then(|value| non_null(frame::phys_to_virt(frame::PhysAddr(value))))
        else {
            return Err(Errno::EFAULT);
        };
        // SAFETY: both pointers refer to whole 4 KiB frames; the source is the client's mapped
        // page and the destination is a live kernel-owned capture frame.
        unsafe {
            core::ptr::copy_nonoverlapping(source, destination, PAGE as usize);
        }
        page = page.saturating_add(1);
    }

    let slot = match server.capture_slots.lock().acquire(pages) {
        Some(slot) => slot,
        None => {
            if trace_s40() {
                crate::kprintln!("s40: capture no slot pages={pages}");
            }
            return Err(Errno::ENOMEM);
        }
    };
    let base = capture_window::BASE
        .saturating_add((slot as u64).saturating_mul(capture_window::SLOT_SIZE));
    let flags = if writable {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    };

    let mut mapped = 0u64;
    {
        let mut server_space = server.address_space.lock();
        let Some(address_space) = server_space.as_mut() else {
            server.capture_slots.lock().release(slot);
            return Err(Errno::ESRCH);
        };
        let mut mapper = match address_space.mapper() {
            Ok(mapper) => mapper,
            Err(error) => {
                server.capture_slots.lock().release(slot);
                return Err(map_errno(error));
            }
        };
        while mapped < pages {
            let Some(virt) = VirtAddr::new(base.saturating_add(mapped.saturating_mul(PAGE))) else {
                if !unmap_partial(&mut mapper, base, mapped) {
                    frames.leak();
                }
                server.capture_slots.lock().release(slot);
                return Err(Errno::EINVAL);
            };
            let Some(physical) = frames.physical(mapped) else {
                if !unmap_partial(&mut mapper, base, mapped) {
                    frames.leak();
                }
                server.capture_slots.lock().release(slot);
                return Err(Errno::EFAULT);
            };
            if mapper
                .map(virt, PhysAddr(physical), flags, PageSize::Size4K)
                .is_err()
            {
                if trace_s40() {
                    crate::kprintln!(
                        "s40: capture map failed slot={} page={} base={:#x}",
                        slot,
                        mapped,
                        base
                    );
                }
                if !unmap_partial(&mut mapper, base, mapped) {
                    frames.leak();
                }
                server.capture_slots.lock().release(slot);
                return Err(Errno::ENOMEM);
            }
            mapped = mapped.saturating_add(1);
        }
    }

    Ok(Capture {
        slot: Some(slot),
        client: Arc::clone(client),
        server: Arc::clone(server),
        address: base.saturating_add(offset),
        base,
        client_start: start,
        pages,
        frames,
        client_frames,
        client_generation,
        writable,
        copy_back_pending: writable,
        mapped: true,
    })
}

fn unmap_partial(
    mapper: &mut crate::memory::paging::KernelMapper<'_>,
    base: u64,
    pages: u64,
) -> bool {
    let mut clean = true;
    let mut page = 0u64;
    while page < pages {
        let address = base.saturating_add(page.saturating_mul(PAGE));
        if let Some(virt) = VirtAddr::new(address)
            && mapper.unmap(virt).is_err()
        {
            clean = false;
        }
        page = page.saturating_add(1);
    }
    let flushed = crate::sched::smp::shootdown::flush_range(base, pages.saturating_mul(PAGE));
    clean && flushed
}

fn map_errno(error: MapError) -> Errno {
    match error {
        MapError::OutOfFrames => Errno::ENOMEM,
        MapError::NotCanonical | MapError::Misaligned => Errno::EINVAL,
        _ => Errno::ENOMEM,
    }
}

const _: () = {
    assert!(capture_window::SLOT_SIZE == 4 * 1024 * 1024);
    assert!(capture_window::SLOTS > 0);
    assert!(capture_window::BASE < capture_user_end());
};

/// One past the highest user address on the running target.
const fn capture_user_end() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        raven_abi::layout::x86_64::USER_END
    }
    #[cfg(target_arch = "aarch64")]
    {
        raven_abi::layout::aarch64::USER_END
    }
}
