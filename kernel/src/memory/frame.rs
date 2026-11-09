// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Global physical frame allocator built from the validated memory map.
//!
//! The bitmap lives in the first large-enough Usable region above 1 MiB. Only `Usable` regions
//! are marked allocatable; LoaderReclaimable, KernelAndModules, ACPI, Reserved, MMIO and
//! BadMemory frames stay reserved. The bitmap's own frames are reserved after the usable regions
//! are marked.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_bootinfo::{MemoryKind, MemoryRegion};
use raven_frames::FrameAllocator;

pub use raven_frames::FreeError;
use spin::Mutex;

/// Physical page size.
pub const PAGE_SIZE: u64 = 4096;
/// Frames below this physical address are never allocatable (low-memory rule).
const LOW_MEMORY_END: u64 = 0x10_0000;

/// A physical address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

/// Summary returned by [`stats`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Managed physical bytes.
    pub total_bytes: u64,
    /// Currently free physical bytes.
    pub free_bytes: u64,
    /// Number of Usable regions in the map.
    pub regions: u64,
}

/// Why the frame allocator could not be initialised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameInitError {
    /// The allocator has already been initialised.
    AlreadyInitialized,
    /// The memory map is empty.
    EmptyMap,
    /// No Usable region above 1 MiB is large enough for the bitmap.
    NoUsableRegion,
    /// Sizes or addresses overflow.
    Overflow,
    /// The HHDM base is invalid.
    BadHhdm,
}

impl fmt::Display for FrameInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameInitError::AlreadyInitialized => {
                f.write_str("frame allocator already initialised")
            }
            FrameInitError::EmptyMap => f.write_str("memory map is empty"),
            FrameInitError::NoUsableRegion => {
                f.write_str("no Usable region is large enough for the frame bitmap")
            }
            FrameInitError::Overflow => f.write_str("frame-allocator size overflow"),
            FrameInitError::BadHhdm => f.write_str("invalid HHDM base"),
        }
    }
}

struct State {
    allocator: FrameAllocator<'static>,
    usable_regions: u64,
}

static ALLOCATOR: Mutex<Option<State>> = Mutex::new(None);
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Build the global allocator from `map` and return its initial statistics.
pub fn init(hhdm_offset: u64, map: &[MemoryRegion]) -> Result<FrameStats, FrameInitError> {
    if !hhdm_offset.is_multiple_of(PAGE_SIZE) || hhdm_offset == 0 {
        return Err(FrameInitError::BadHhdm);
    }
    if ALLOCATOR.lock().is_some() {
        return Err(FrameInitError::AlreadyInitialized);
    }
    if map.is_empty() {
        return Err(FrameInitError::EmptyMap);
    }

    let highest_end = map
        .iter()
        .filter_map(MemoryRegion::end)
        .max()
        .ok_or(FrameInitError::Overflow)?;
    let frame_count = highest_end / PAGE_SIZE;
    if frame_count == 0 {
        return Err(FrameInitError::EmptyMap);
    }
    let words = frame_count
        .checked_add(63)
        .ok_or(FrameInitError::Overflow)?
        / 64;
    let bitmap_bytes = words.checked_mul(8).ok_or(FrameInitError::Overflow)?;
    let bitmap_words = usize::try_from(words).map_err(|_| FrameInitError::Overflow)?;
    let bitmap_length = usize::try_from(bitmap_bytes).map_err(|_| FrameInitError::Overflow)?;

    let bitmap = map
        .iter()
        .find(|region| {
            region.kind() == Some(MemoryKind::Usable)
                && region.base >= LOW_MEMORY_END
                && region.len >= bitmap_bytes
        })
        .ok_or(FrameInitError::NoUsableRegion)?;

    let bitmap_virtual = hhdm_offset
        .checked_add(bitmap.base)
        .ok_or(FrameInitError::Overflow)?;
    let bitmap_pointer = bitmap_virtual as *mut u64;

    // SAFETY: the chosen region is a validated Usable range at least `bitmap_bytes` long and the
    // HHDM maps all physical RAM. The kernel owns this region from here on; the allocation is
    // page-aligned, so the u64 array is aligned as well.
    unsafe {
        core::ptr::write_bytes(bitmap_pointer.cast::<u8>(), 0, bitmap_length);
    }
    // SAFETY: same range and alignment; the slice lives for the kernel's lifetime and is the
    // only mutable alias to these bitmap bytes.
    let bitmap_slice: &'static mut [u64] =
        unsafe { core::slice::from_raw_parts_mut(bitmap_pointer, bitmap_words) };

    let mut allocator = FrameAllocator::new(bitmap_slice, 0, frame_count);
    let mut usable_regions = 0u64;
    for region in map {
        if region.kind() == Some(MemoryKind::Usable) {
            allocator.mark_usable(region.base / PAGE_SIZE, region.len / PAGE_SIZE);
            usable_regions = usable_regions.saturating_add(1);
        }
    }
    let bitmap_frames = bitmap_bytes.div_ceil(PAGE_SIZE);
    allocator.mark_reserved(bitmap.base / PAGE_SIZE, bitmap_frames);

    let stats = stats_for(&allocator, usable_regions);
    HHDM_OFFSET.store(hhdm_offset, Ordering::Release);
    *ALLOCATOR.lock() = Some(State {
        allocator,
        usable_regions,
    });
    Ok(stats)
}

/// Allocate one frame.
pub fn alloc_frame() -> Option<PhysAddr> {
    let mut state = ALLOCATOR.lock();
    let allocator = state.as_mut()?;
    let frame = allocator.allocator.alloc()?;
    frame.checked_mul(PAGE_SIZE).map(PhysAddr)
}

/// Allocate one frame and zero it through the HHDM.
pub fn alloc_zeroed_frame() -> Option<PhysAddr> {
    let phys = alloc_frame()?;
    let pointer = phys_to_virt(phys);
    if pointer.is_null() {
        let _ = free_frame(phys);
        return None;
    }
    // SAFETY: `phys` is a freshly allocated frame and the HHDM maps it writable.
    unsafe {
        core::ptr::write_bytes(pointer, 0, PAGE_SIZE as usize);
    }
    Some(phys)
}

/// Free one frame.
pub fn free_frame(phys: PhysAddr) -> Result<(), FreeError> {
    if !phys.0.is_multiple_of(PAGE_SIZE) {
        return Err(FreeError::OutOfRange);
    }
    let mut state = ALLOCATOR.lock();
    let allocator = state.as_mut().ok_or(FreeError::OutOfRange)?;
    allocator.allocator.free(phys.0 / PAGE_SIZE)
}

/// Current allocator statistics; zeros before initialisation.
pub fn stats() -> FrameStats {
    let state = ALLOCATOR.lock();
    match state.as_ref() {
        Some(state) => stats_for(&state.allocator, state.usable_regions),
        None => FrameStats::default(),
    }
}

/// Convert a physical address to its HHDM virtual pointer; null when the HHDM is unavailable.
pub fn phys_to_virt(phys: PhysAddr) -> *mut u8 {
    let hhdm = HHDM_OFFSET.load(Ordering::Acquire);
    hhdm.checked_add(phys.0)
        .map(|address| address as *mut u8)
        .unwrap_or(core::ptr::null_mut())
}

fn stats_for(allocator: &FrameAllocator<'_>, usable_regions: u64) -> FrameStats {
    FrameStats {
        total_bytes: allocator.total_frames().saturating_mul(PAGE_SIZE),
        free_bytes: allocator.free_frames().saturating_mul(PAGE_SIZE),
        regions: usable_regions,
    }
}
