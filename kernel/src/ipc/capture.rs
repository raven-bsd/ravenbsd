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
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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

/// K147 stage-trace counters (first 64 captures each way).
static K147_CREATE: AtomicU64 = AtomicU64::new(0);
static K147_DROP: AtomicU64 = AtomicU64::new(0);
static K147_COPYBACK: AtomicU64 = AtomicU64::new(0);

/// Number of capture frames currently owned by live captures.
pub fn frames_in_use() -> u64 {
    CAPTURE_FRAMES.load(Ordering::Acquire)
}

/// Pages in one capture-window slot.
const SLOT_PAGES: usize = (capture_window::SLOT_SIZE / PAGE) as usize;
/// 64-bit words in one slot's page bitmap.
const SLOT_WORDS: usize = SLOT_PAGES.div_ceil(64);

const _: () = {
    assert!(SLOT_PAGES > 0);
    assert!(SLOT_PAGES.is_multiple_of(64));
};

/// A contiguous page range inside one capture-window slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SlotRange {
    slot: usize,
    page: usize,
    pages: u64,
}

/// Kernel-wide capture-slot counters.
///
/// A `CALL` holds a request and a reply capture at once. Captures share a 4 MiB slot by page
/// range, so the page counters are the capacity signal and the capture counters are the
/// request-lifetime signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotStats {
    /// Successful capture acquisitions since boot.
    pub acquires: u64,
    /// Capture releases since boot.
    pub releases: u64,
    /// Captures currently held across every server.
    pub active: usize,
    /// Highest `active` value observed.
    pub peak: usize,
    /// Capture pages currently mapped across every server.
    pub pages: usize,
    /// Highest `pages` value observed.
    pub page_peak: usize,
}

/// Per-process capture-pool usage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapturePoolStats {
    /// Live captures.
    pub captures: usize,
    /// Highest number of live captures.
    pub capture_peak: usize,
    /// Live capture pages.
    pub pages: usize,
    /// Highest number of live capture pages.
    pub page_peak: usize,
}

static SLOT_ACQUIRES: AtomicU64 = AtomicU64::new(0);
static SLOT_RELEASES: AtomicU64 = AtomicU64::new(0);
static SLOTS_ACTIVE: AtomicUsize = AtomicUsize::new(0);
static SLOTS_PEAK: AtomicUsize = AtomicUsize::new(0);
static PAGES_ACTIVE: AtomicUsize = AtomicUsize::new(0);
static PAGES_PEAK: AtomicUsize = AtomicUsize::new(0);

/// Snapshot the kernel-wide capture-slot counters.
pub fn slot_stats() -> SlotStats {
    SlotStats {
        acquires: SLOT_ACQUIRES.load(Ordering::Acquire),
        releases: SLOT_RELEASES.load(Ordering::Acquire),
        active: SLOTS_ACTIVE.load(Ordering::Acquire),
        peak: SLOTS_PEAK.load(Ordering::Acquire),
        pages: PAGES_ACTIVE.load(Ordering::Acquire),
        page_peak: PAGES_PEAK.load(Ordering::Acquire),
    }
}

fn slot_page_used(bits: &[u64; SLOT_WORDS], page: usize) -> bool {
    let Some(word) = bits.get(page / 64) else {
        return true;
    };
    word & (1u64 << (page % 64)) != 0
}

fn set_slot_page(bits: &mut [u64; SLOT_WORDS], page: usize, used: bool) {
    let Some(word) = bits.get_mut(page / 64) else {
        return;
    };
    let mask = 1u64 << (page % 64);
    if used {
        *word |= mask;
    } else {
        *word &= !mask;
    }
}

fn find_slot_run(bits: &[u64; SLOT_WORDS], pages: usize) -> Option<usize> {
    let mut run = 0usize;
    let mut start = 0usize;
    for page in 0..SLOT_PAGES {
        if slot_page_used(bits, page) {
            run = 0;
            continue;
        }
        if run == 0 {
            start = page;
        }
        run = run.saturating_add(1);
        if run == pages {
            return Some(start);
        }
    }
    None
}

/// Per-process allocation state for capture slots.
///
/// Captures are packed by page range into the fixed virtual slots. A `CALL` therefore does not
/// need two whole 4 MiB slots for its request and reply captures; both fit in the same slot when
/// their combined pages fit, while the slot bitmap keeps each range exclusive.
pub struct CaptureSlots {
    used: [[u64; SLOT_WORDS]; capture_window::SLOTS],
    live: usize,
    live_pages: usize,
    peak: usize,
    page_peak: usize,
}

impl CaptureSlots {
    /// Empty allocator.
    pub const fn new() -> Self {
        Self {
            used: [[0; SLOT_WORDS]; capture_window::SLOTS],
            live: 0,
            live_pages: 0,
            peak: 0,
            page_peak: 0,
        }
    }

    /// Allocate one contiguous page range large enough for `pages` pages.
    fn acquire(&mut self, pages: u64) -> Option<SlotRange> {
        let pages = usize::try_from(pages).ok()?;
        if pages == 0 || pages > SLOT_PAGES {
            return None;
        }
        for slot in 0..capture_window::SLOTS {
            let Some(bits) = self.used.get(slot) else {
                continue;
            };
            let Some(start) = find_slot_run(bits, pages) else {
                continue;
            };
            let Some(bits) = self.used.get_mut(slot) else {
                continue;
            };
            for page in start..start.saturating_add(pages) {
                set_slot_page(bits, page, true);
            }
            self.live = self.live.saturating_add(1);
            self.live_pages = self.live_pages.saturating_add(pages);
            self.peak = self.peak.max(self.live);
            self.page_peak = self.page_peak.max(self.live_pages);
            SLOT_ACQUIRES.fetch_add(1, Ordering::AcqRel);
            let active = SLOTS_ACTIVE
                .fetch_add(1, Ordering::AcqRel)
                .saturating_add(1);
            SLOTS_PEAK.fetch_max(active, Ordering::AcqRel);
            let pages_active = PAGES_ACTIVE
                .fetch_add(pages, Ordering::AcqRel)
                .saturating_add(pages);
            PAGES_PEAK.fetch_max(pages_active, Ordering::AcqRel);
            return Some(SlotRange {
                slot,
                page: start,
                pages: u64::try_from(pages).unwrap_or(u64::MAX),
            });
        }
        None
    }

    /// Release a page range.
    ///
    /// Releasing an already-free range is a programming error: a `Capture` must release its
    /// range exactly once. In release builds a second release is a no-op, so a latent double
    /// drop cannot underflow the counters or hand the same pages to another capture.
    fn release(&mut self, range: SlotRange) {
        let pages = usize::try_from(range.pages).unwrap_or(usize::MAX);
        let mut released = 0usize;
        if let Some(bits) = self.used.get_mut(range.slot) {
            for page in range.page..range.page.saturating_add(pages) {
                if slot_page_used(bits, page) {
                    set_slot_page(bits, page, false);
                    released = released.saturating_add(1);
                }
            }
        }
        if released == 0 {
            return;
        }
        self.live = self.live.saturating_sub(1);
        self.live_pages = self.live_pages.saturating_sub(released);
        SLOT_RELEASES.fetch_add(1, Ordering::AcqRel);
        let previous = SLOTS_ACTIVE.fetch_sub(1, Ordering::AcqRel);
        if previous == 0 {
            // A release without a matching acquire must not wrap the counter.
            SLOTS_ACTIVE.store(0, Ordering::Release);
        }
        let previous_pages = PAGES_ACTIVE.fetch_sub(released, Ordering::AcqRel);
        if previous_pages < released {
            PAGES_ACTIVE.store(0, Ordering::Release);
        }
    }

    /// This process's capture-pool usage.
    pub fn stats(&self) -> CapturePoolStats {
        CapturePoolStats {
            captures: self.live,
            capture_peak: self.peak,
            pages: self.live_pages,
            page_peak: self.page_peak,
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
    /// First page of this capture's range inside `slot`.
    slot_page: usize,
    client: Arc<Process>,
    server: Arc<Process>,
    /// Server-side virtual address of the first requested byte.
    address: u64,
    /// Page-aligned server address of the first captured page.
    base: u64,
    /// Page-aligned client address of the first captured byte.
    client_start: u64,
    pages: u64,
    frames: OwnedFrames,
    /// Client physical frames observed at create; copy-back only touches the same frame.
    client_frames: Vec<u64>,
    /// Client page-table root at create. A new root means exec replaced the address space.
    client_root: u64,
    /// The server may write the capture and the frames are copied back on completion.
    writable: bool,
    /// Copy-back is still owed (cleared by completion or cancellation).
    copy_back_pending: bool,
    /// The frames are mapped into the server at `base`.
    mapped: bool,
}

/// True when the client still has the page-table root captured at create.
///
/// A normal `MAP`/`UNMAP`/`PROTECT`, including one from another thread in the shared address
/// space, keeps the root. `exec` installs a new root; exit drops the address space entirely.
fn address_space_matches(captured_root: u64, current_root: Option<u64>) -> bool {
    current_root == Some(captured_root)
}

/// True when a captured page may receive copy-back.
///
/// The captured frame must still be the frame translated at the same client address. Unmapped
/// pages and pages made read-only are skipped; a reused virtual address backed by another frame
/// is skipped as a replaced mapping.
fn page_copy_allowed(original_frame: u64, current_frame: Option<u64>, writable: bool) -> bool {
    writable && current_frame == Some(original_frame)
}

impl Capture {
    /// Server-side virtual address of the first requested byte.
    pub fn address(&self) -> u64 {
        self.address
    }

    /// Capture-window slot held by this capture, or `None` for a same-process capture.
    pub fn slot_index(&self) -> Option<usize> {
        self.slot
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
        let trace = crate::ipc::trace_k147() && K147_COPYBACK.fetch_add(1, Ordering::Relaxed) < 64;
        let trace_start = if trace { crate::time::now_ns() } else { 0 };
        let mut space = self.client.address_space.lock();
        let Some(space) = space.as_mut() else {
            return;
        };
        if !address_space_matches(self.client_root, Some(space.root().0)) {
            // Exec installed a new page-table root after the capture started; never copy into
            // the replacement image. Ordinary MAP/UNMAP/PROTECT on another thread keeps the same
            // root and is handled per page below.
            if trace_s125() {
                crate::kprintln!(
                    "s125: copy_back root changed captured={:#x} current={:#x} generation={}",
                    self.client_root,
                    space.root().0,
                    space.generation()
                );
            }
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
            let Some(original) = self
                .client_frames
                .get(usize::try_from(page).unwrap_or(usize::MAX))
                .copied()
            else {
                page = page.saturating_add(1);
                continue;
            };
            if !page_copy_allowed(original, Some(physical.0), flags.is_writable()) {
                // The page was unmapped, made read-only, or remapped to another frame.
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
        if trace {
            crate::kprintln!(
                "k147 copyback pages={} total={}",
                self.pages,
                crate::time::now_ns().saturating_sub(trace_start)
            );
        }
    }

    /// A cancelled client no longer wants the data; keep the mapping for the server but never
    /// copy back into the dead client's address space.
    pub fn cancel_copy_back(&mut self) {
        self.copy_back_pending = false;
    }
}

/// True when the S125 capture-pool diagnostics are enabled.
pub(crate) fn trace_s125() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s125-capturecheck")
}

/// True when the S40 capture diagnostics are enabled.
fn trace_s40() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s40-teardowncheck")
}

impl Drop for Capture {
    fn drop(&mut self) {
        let trace = crate::ipc::trace_k147() && K147_DROP.fetch_add(1, Ordering::Relaxed) < 64;
        let trace_start = if trace { crate::time::now_ns() } else { 0 };
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
        let trace_unmap = if trace { crate::time::now_ns() } else { 0 };
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
        let trace_flush = if trace { crate::time::now_ns() } else { 0 };
        if let Some(slot) = self.slot.take() {
            self.server.capture_slots.lock().release(SlotRange {
                slot,
                page: self.slot_page,
                pages: self.pages,
            });
        }
        if trace {
            let trace_end = crate::time::now_ns();
            crate::kprintln!(
                "k147 cap drop pages={} unmap={} flush={} total={}",
                self.pages,
                trace_unmap.saturating_sub(trace_start),
                trace_flush.saturating_sub(trace_unmap),
                trace_end.saturating_sub(trace_start)
            );
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
            slot_page: 0,
            client: Arc::clone(client),
            server: Arc::clone(server),
            address: client_addr,
            base: 0,
            client_start: 0,
            pages: 0,
            frames: OwnedFrames::empty(),
            client_frames: Vec::new(),
            client_root: 0,
            writable: false,
            copy_back_pending: false,
            mapped: false,
        });
    }

    let trace = crate::ipc::trace_k147() && K147_CREATE.fetch_add(1, Ordering::Relaxed) < 64;
    let trace_start = if trace { crate::time::now_ns() } else { 0 };
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
    let client_root;
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
        client_root = address_space.root().0;
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
    let trace_copy = if trace { crate::time::now_ns() } else { 0 };

    let allocation = match server.capture_slots.lock().acquire(pages) {
        Some(allocation) => allocation,
        None => {
            if trace_s40() {
                crate::kprintln!("s40: capture no slot pages={pages}");
            }
            if trace_s125() {
                let stats = slot_stats();
                let pool = server.capture_slots.lock().stats();
                crate::kprintln!(
                    "s125: capture no slot server={}:{} pages={} used={} server_peak={} pages_used={} server_page_peak={} active={} global_peak={} page_active={} page_peak={} acquires={} releases={}",
                    server.path,
                    server.id,
                    pages,
                    pool.captures,
                    pool.capture_peak,
                    pool.pages,
                    pool.page_peak,
                    stats.active,
                    stats.peak,
                    stats.pages,
                    stats.page_peak,
                    stats.acquires,
                    stats.releases
                );
            }
            return Err(Errno::ENOMEM);
        }
    };
    let slot = allocation.slot;
    let slot_page = allocation.page;
    let trace_slot = if trace { crate::time::now_ns() } else { 0 };
    let base = capture_window::BASE
        .saturating_add((slot as u64).saturating_mul(capture_window::SLOT_SIZE))
        .saturating_add((slot_page as u64).saturating_mul(PAGE));
    let flags = if writable {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    };

    let mut mapped = 0u64;
    {
        let mut server_space = server.address_space.lock();
        let Some(address_space) = server_space.as_mut() else {
            server.capture_slots.lock().release(SlotRange {
                slot,
                page: slot_page,
                pages,
            });
            return Err(Errno::ESRCH);
        };
        let mut mapper = match address_space.mapper() {
            Ok(mapper) => mapper,
            Err(error) => {
                server.capture_slots.lock().release(SlotRange {
                    slot,
                    page: slot_page,
                    pages,
                });
                return Err(map_errno(error));
            }
        };
        while mapped < pages {
            let Some(virt) = VirtAddr::new(base.saturating_add(mapped.saturating_mul(PAGE))) else {
                crate::kprintln!("capture: virt invalid base={:#x} mapped={}", base, mapped);
                if !unmap_partial(&mut mapper, base, mapped) {
                    frames.leak();
                }
                server.capture_slots.lock().release(SlotRange {
                    slot,
                    page: slot_page,
                    pages,
                });
                return Err(Errno::EINVAL);
            };
            let Some(physical) = frames.physical(mapped) else {
                if !unmap_partial(&mut mapper, base, mapped) {
                    frames.leak();
                }
                server.capture_slots.lock().release(SlotRange {
                    slot,
                    page: slot_page,
                    pages,
                });
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
                server.capture_slots.lock().release(SlotRange {
                    slot,
                    page: slot_page,
                    pages,
                });
                return Err(Errno::ENOMEM);
            }
            mapped = mapped.saturating_add(1);
        }
    }
    let trace_map = if trace { crate::time::now_ns() } else { 0 };
    if trace {
        crate::kprintln!(
            "k147 cap create pages={} copy={} slot={} map={} total={}",
            pages,
            trace_copy.saturating_sub(trace_start),
            trace_slot.saturating_sub(trace_copy),
            trace_map.saturating_sub(trace_slot),
            trace_map.saturating_sub(trace_start)
        );
    }

    Ok(Capture {
        slot: Some(slot),
        slot_page,
        client: Arc::clone(client),
        server: Arc::clone(server),
        address: base.saturating_add(offset),
        base,
        client_start: start,
        pages,
        frames,
        client_frames,
        client_root,
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

#[cfg(test)]
#[path = "capture_ut.rs"]
mod tests;
