// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Global physical frame allocator: the `raven-frames` buddy over the validated memory map.
//!
//! The buddy (A5a) manages the power-of-two range starting at the lowest usable frame; every
//! non-usable range from the boot map (kernel, loader, MMIO, ACPI, firmware) and every hole is
//! reserved. The per-CPU frame magazines (A5b) sit in front of it, indexed by the S75 CPU id, with
//! `frames_percpu=off` disabling them for debugging. The heap (`raven-kmem`) is independent and
//! never uses this path.
//!
//! The buddy's fixed descriptor storage is `MAX_BLOCKS` order-0 blocks; the kernel caps the
//! managed span there (512 MiB) and reports the honest managed total. Growing the cap is a
//! storage-size change, not an allocator change.

use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::kprintln;
use crate::log::SpinMutex;
use raven_bootinfo::{MemoryKind, MemoryRegion};
use spin::Mutex;

#[path = "../../../crates/raven-frames/src/buddy.rs"]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod buddy;
#[path = "../../../crates/raven-frames/src/cache.rs"]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod cache;

use buddy::{Block, BuddyAllocator, FrameState, NoPager};
use cache::FrameCache;

/// Per-CPU magazine statistics.
pub use cache::CpuFrameStats;

/// Physical page size.
pub const PAGE_SIZE: u64 = 4096;
/// Frames below this physical address are never allocatable (low-memory rule).
const LOW_MEMORY_END: u64 = 0x10_0000;

/// Number of buddy orders; the top order is `ORDERS - 1`.
const ORDERS: usize = 18;
/// Maximum managed order-0 blocks (512 MiB). The inline descriptor array is ~1 MiB.
const MAX_BLOCKS: usize = 1 << 17;
/// Per-CPU magazines: one cache per possible CPU.
const NCPUS: usize = crate::sched::smp::MAX_CPUS;
/// Frames each CPU's cache holds.
const CACHE_CAP: usize = 64;
/// Frames in each CPU's remote-free queue.
const REMOTE_CAP: usize = 32;
/// Refill when a cache drops below this many frames.
const LOW_WATERMARK: usize = 16;
/// Drain when a cache grows above this many frames.
const HIGH_WATERMARK: usize = 48;
/// Frames pulled from the buddy per refill.
const REFILL_BATCH: usize = 32;
/// Frames pushed to the buddy per drain.
const DRAIN_BATCH: usize = 32;

type Buddy<'a> = BuddyAllocator<'a, ORDERS, NoPager>;
type Cache = FrameCache<NCPUS, CACHE_CAP, REMOTE_CAP>;

/// Order-0 descriptor storage for the buddy. Kept in `.bss` so the allocator construction never
/// puts a megabyte-sized array on the 8 MiB boot stack.
static mut FRAME_BLOCKS: [Block; MAX_BLOCKS] = [Block::zeroed(); MAX_BLOCKS];

/// A physical address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

/// Summary returned by [`stats`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Managed physical bytes.
    pub total_bytes: u64,
    /// Currently free physical bytes (buddy free plus per-CPU cached frames).
    pub free_bytes: u64,
    /// Physical bytes held in the per-CPU caches.
    pub cached_bytes: u64,
    /// Physical bytes reserved from the boot map.
    pub reserved_bytes: u64,
    /// Physical bytes currently handed out to callers.
    pub allocated_bytes: u64,
    /// Number of Usable regions in the map.
    pub regions: u64,
    /// Whether the per-CPU magazines are enabled.
    pub percpu: bool,
}

/// Why the frame allocator could not be initialised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameInitError {
    /// The memory map is empty.
    EmptyMap,
    /// No Usable region above 1 MiB is large enough.
    NoUsableRegion,
    /// Sizes or addresses overflow.
    Overflow,
    /// The HHDM base is invalid.
    BadHhdm,
}

impl fmt::Display for FrameInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameInitError::EmptyMap => f.write_str("memory map is empty"),
            FrameInitError::NoUsableRegion => {
                f.write_str("no usable region for the frame allocator")
            }
            FrameInitError::Overflow => f.write_str("frame-allocator size overflow"),
            FrameInitError::BadHhdm => f.write_str("invalid HHDM base"),
        }
    }
}

/// Why a frame could not be freed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeError {
    /// The frame is outside the allocator's base/count range.
    OutOfRange,
    /// The frame was already free.
    DoubleFree,
}

struct State {
    buddy: Buddy<'static>,
    cache: Cache,
    percpu: bool,
    usable_regions: u64,
}

static ALLOCATOR: SpinMutex<Option<State>> = SpinMutex::new(None);
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);
static PERCPU_ENABLED: AtomicBool = AtomicBool::new(true);

/// Physical frames mapped read-only by more than one address space.
///
/// A frame enters the registry with a count of two on the first fork and leaves it when the
/// count drops back to one. Frames owned by a single address space are absent, so the common
/// case pays nothing.
static SHARED_FRAMES: Mutex<Vec<(u64, u32)>> = Mutex::new(Vec::new());

/// Build the global allocator from `map` and return its initial statistics.
///
/// Calling this again after a successful initialisation returns the current statistics; boot
/// stages that need the allocator early (S06 paging) rely on this idempotent behaviour.
/// Reserve the absolute frame span `[from, to)` in the buddy (helper for `init`).
#[allow(clippy::arithmetic_side_effects)]
fn reserve_span(buddy: &mut Buddy<'static>, from: u64, to: u64) -> Result<(), FrameInitError> {
    if from < to {
        buddy
            .reserve(from, to - from)
            .map_err(|_| FrameInitError::Overflow)?;
    }
    Ok(())
}

#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
pub fn init(hhdm_offset: u64, map: &[MemoryRegion]) -> Result<FrameStats, FrameInitError> {
    if !hhdm_offset.is_multiple_of(PAGE_SIZE) || hhdm_offset == 0 {
        return Err(FrameInitError::BadHhdm);
    }
    if ALLOCATOR.lock().is_some() {
        return Ok(stats());
    }
    if map.is_empty() {
        return Err(FrameInitError::EmptyMap);
    }

    // Only usable RAM is managed. Base the buddy at the lowest usable frame so a platform whose
    // RAM starts well above zero (aarch64 `virt` places RAM at 1 GiB) is not excluded by the
    // descriptor-sized cap, and span the usable interval rounded up to a power of two. The cap
    // only drops the high end of an over-large map; it never excludes the low usable base.
    let lowest_usable_base = map
        .iter()
        .filter(|region| region.kind() == Some(MemoryKind::Usable))
        .map(|region| region.base)
        .min()
        .ok_or(FrameInitError::NoUsableRegion)?;
    let highest_usable_end = map
        .iter()
        .filter(|region| region.kind() == Some(MemoryKind::Usable))
        .filter_map(MemoryRegion::end)
        .max()
        .ok_or(FrameInitError::NoUsableRegion)?;
    let base_frame = lowest_usable_base / PAGE_SIZE;
    let span_frames = highest_usable_end
        .checked_sub(lowest_usable_base)
        .ok_or(FrameInitError::Overflow)?
        / PAGE_SIZE;
    if span_frames == 0 {
        return Err(FrameInitError::EmptyMap);
    }
    let managed_frames = span_frames.next_power_of_two().min(MAX_BLOCKS as u64);
    if managed_frames == 0 {
        return Err(FrameInitError::Overflow);
    }
    let managed_end = base_frame.saturating_add(managed_frames);

    // SAFETY: boot is single-threaded and `frame::init` runs once; the storage is borrowed for the
    // kernel's lifetime by the one global allocator.
    let blocks: &'static mut [Block] = unsafe { &mut *core::ptr::addr_of_mut!(FRAME_BLOCKS) };
    let mut buddy = Buddy::with_pager(blocks, base_frame, managed_frames, NoPager)
        .map_err(|_| FrameInitError::Overflow)?;

    // Reserve every frame that is not `Usable`: the non-usable regions themselves and any gap
    // between regions, plus the padding from the last region up to the power-of-two range end.
    let mut reserved_upto = base_frame;
    for region in map {
        let start = (region.base / PAGE_SIZE).max(base_frame).min(managed_end);
        let end = (region.end().unwrap_or(u64::MAX) / PAGE_SIZE).min(managed_end);
        if end <= base_frame {
            continue;
        }
        if start > reserved_upto {
            reserve_span(&mut buddy, reserved_upto, start)?;
        }
        if region.kind() != Some(MemoryKind::Usable) && start < end {
            reserve_span(&mut buddy, start, end)?;
        }
        reserved_upto = reserved_upto.max(end);
        if reserved_upto >= managed_end {
            break;
        }
    }
    if reserved_upto < managed_end {
        reserve_span(&mut buddy, reserved_upto, managed_end)?;
    }
    let initial = buddy.stats();
    kprintln!(
        "frames: base={} count={} free={} reserved={}",
        base_frame,
        managed_frames,
        initial.free,
        initial.reserved
    );

    let percpu = PERCPU_ENABLED.load(Ordering::Acquire);
    let cache = Cache::new(
        NCPUS,
        LOW_WATERMARK,
        HIGH_WATERMARK,
        REFILL_BATCH,
        DRAIN_BATCH,
    );
    let usable_regions = map
        .iter()
        .filter(|region| region.kind() == Some(MemoryKind::Usable))
        .count() as u64;
    HHDM_OFFSET.store(hhdm_offset, Ordering::Release);
    *ALLOCATOR.lock() = Some(State {
        buddy,
        cache,
        percpu,
        usable_regions,
    });
    Ok(stats())
}

/// Enable or disable the per-CPU frame magazines (`frames_percpu=off` disables them).
pub fn set_percpu_enabled(enabled: bool) {
    PERCPU_ENABLED.store(enabled, Ordering::Release);
    let mut state = ALLOCATOR.lock();
    if let Some(state) = state.as_mut() {
        // Drain anything already cached so the disabled path starts from the buddy.
        while let Some(frame) = state.cache.pop_any() {
            let _ = state.buddy.free(frame, 0);
        }
        state.percpu = enabled;
    }
}

/// Allocate one frame.
pub fn alloc_frame() -> Option<PhysAddr> {
    let mut state = ALLOCATOR.lock();
    let state = state.as_mut()?;
    let frame = if state.percpu {
        let cpu = crate::sched::smp::percpu::this_cpu();
        state.cache.alloc(&mut state.buddy, cpu).ok()?
    } else {
        state.buddy.alloc(0).ok()?
    };
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

/// Allocate `pages` contiguous frames whose first frame is `align_pages`-aligned and whose last
/// frame is below `max_frame` (the DMA mask expressed in frame numbers).
pub fn alloc_contiguous(pages: u64, align_pages: u64, max_frame: u64) -> Option<PhysAddr> {
    let mut state = ALLOCATOR.lock();
    let state = state.as_mut()?;
    state
        .buddy
        .alloc_contiguous(pages, align_pages, max_frame)
        .ok()
        .map(|frame| PhysAddr(frame.saturating_mul(PAGE_SIZE)))
}

/// Free one frame.
pub fn free_frame(phys: PhysAddr) -> Result<(), FreeError> {
    if !phys.0.is_multiple_of(PAGE_SIZE) {
        return Err(FreeError::OutOfRange);
    }
    let mut state = ALLOCATOR.lock();
    let state = state.as_mut().ok_or(FreeError::OutOfRange)?;
    state.free(phys.0 / PAGE_SIZE)
}

impl State {
    fn free(&mut self, frame: u64) -> Result<(), FreeError> {
        match self.buddy.state(frame) {
            None => return Err(FreeError::OutOfRange),
            Some(FrameState::Free) => return Err(FreeError::DoubleFree),
            Some(FrameState::Reserved) => {
                // Loader/ACPI reclaim: free the reservation straight in the buddy.
                return self.buddy.release(frame).map_err(|_| FreeError::OutOfRange);
            }
            Some(FrameState::Allocated) => {}
        }
        if self.percpu {
            let cpu = crate::sched::smp::percpu::this_cpu();
            self.cache.free(&mut self.buddy, cpu, frame);
        } else {
            let _ = self.buddy.free(frame, 0);
        }
        Ok(())
    }
}

/// Mark `phys` as shared by one more address space.
///
/// The first share records two owners (the existing one and the new one); later shares
/// increment the count. The frame stays allocated until the count returns to one and the last
/// owner releases it, or until that owner frees it directly.
pub fn share_frame(phys: PhysAddr) {
    let mut shared = SHARED_FRAMES.lock();
    if let Some(entry) = shared.iter_mut().find(|entry| entry.0 == phys.0) {
        entry.1 = entry.1.saturating_add(1);
    } else {
        shared.push((phys.0, 2));
    }
}

/// True when more than one address space maps `phys`.
pub fn is_shared(phys: PhysAddr) -> bool {
    SHARED_FRAMES.lock().iter().any(|entry| entry.0 == phys.0)
}

/// Drop one shared reference to `phys`.
///
/// A count of two means one owner remains: the entry is removed and the frame is *not* freed.
/// The remaining owner frees it through its normal teardown or unmap path.
pub fn release_shared(phys: PhysAddr) {
    let mut shared = SHARED_FRAMES.lock();
    let Some(index) = shared.iter().position(|entry| entry.0 == phys.0) else {
        return;
    };
    if let Some(entry) = shared.get_mut(index)
        && entry.1 > 2
    {
        entry.1 = entry.1.saturating_sub(1);
        return;
    }
    let _ = shared.swap_remove(index);
}

/// Current allocator statistics; zeros before initialisation.
pub fn stats() -> FrameStats {
    let state = ALLOCATOR.lock();
    match state.as_ref() {
        Some(state) => {
            let buddy = state.buddy.stats();
            let cached = state
                .cache
                .cached_total()
                .saturating_add(state.cache.remote_total()) as u64;
            let free = buddy.free.saturating_add(cached);
            let accounted = buddy
                .free
                .saturating_add(buddy.reserved)
                .saturating_add(cached);
            FrameStats {
                total_bytes: buddy.total.saturating_mul(PAGE_SIZE),
                free_bytes: free.saturating_mul(PAGE_SIZE),
                cached_bytes: cached.saturating_mul(PAGE_SIZE),
                reserved_bytes: buddy.reserved.saturating_mul(PAGE_SIZE),
                allocated_bytes: buddy
                    .total
                    .saturating_sub(accounted)
                    .saturating_mul(PAGE_SIZE),
                regions: state.usable_regions,
                percpu: state.percpu,
            }
        }
        None => FrameStats::default(),
    }
}

/// Per-CPU magazine statistics; zeros before initialisation or for an unused CPU.
pub fn cpu_stats(cpu: usize) -> CpuFrameStats {
    let state = ALLOCATOR.lock();
    match state.as_ref() {
        Some(state) if cpu < NCPUS => state.cache.cpu_stats(cpu),
        _ => CpuFrameStats::default(),
    }
}

/// Convert a physical address to its HHDM virtual pointer; null when the HHDM is unavailable.
pub fn phys_to_virt(phys: PhysAddr) -> *mut u8 {
    let hhdm = HHDM_OFFSET.load(Ordering::Acquire);
    hhdm.checked_add(phys.0)
        .map(|address| address as *mut u8)
        .unwrap_or(core::ptr::null_mut())
}

/// The HHDM base recorded at initialisation; zero before initialisation.
pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Acquire)
}

/// Free every frame in `base..base + len`, returning how many frames became free.
///
/// Frames that are already free are skipped. The range is expected to be page aligned; a
/// misaligned base frees nothing. Used to reclaim the loader's reclaimable regions.
pub fn free_range(base: u64, len: u64) -> u64 {
    if !base.is_multiple_of(PAGE_SIZE) {
        return 0;
    }
    let frames = len >> 12;
    let mut freed = 0u64;
    let mut state = ALLOCATOR.lock();
    let Some(state) = state.as_mut() else {
        return 0;
    };
    let mut offset = 0u64;
    while offset < frames {
        let frame = (base >> 12).saturating_add(offset);
        if state.buddy.release(frame).is_ok() {
            freed = freed.saturating_add(1);
        }
        offset = offset.saturating_add(1);
    }
    freed
}

/// A boot-time soak of the frame allocator, run by the paging init when `frames_selftest=1`.
///
/// Exercises single-frame allocation and free with the per-CPU caches on and off, a contiguous
/// run, an out-of-memory probe, and conservation after draining. Allocation-free so it can run
/// before the heap exists.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
pub fn selftest() -> bool {
    const SOAK_FRAMES: usize = 256;
    let mut frames = [0u64; SOAK_FRAMES];
    let mut ok = true;

    for enabled in [true, false] {
        set_percpu_enabled(enabled);
        let mut taken = 0usize;
        while taken < SOAK_FRAMES {
            match alloc_frame() {
                Some(phys) => {
                    frames[taken] = phys.0;
                    taken += 1;
                }
                None => break,
            }
        }
        // All frames distinct and page aligned.
        for i in 0..taken {
            if !frames[i].is_multiple_of(PAGE_SIZE) {
                ok = false;
            }
            for j in (i + 1)..taken {
                if frames[i] == frames[j] {
                    ok = false;
                }
            }
        }
        for frame in frames.iter().take(taken) {
            if free_frame(PhysAddr(*frame)).is_err() {
                ok = false;
            }
        }
        kprintln!(
            "frames: soak percpu={} allocated={} ok={}",
            if enabled { "on" } else { "off" },
            taken,
            ok
        );
    }

    // Contiguous run: 32 frames, 8-frame aligned, freed frame by frame as DMA teardown does.
    if let Some(run) = alloc_contiguous(32, 8, u64::MAX / PAGE_SIZE) {
        if run.0 % (8 * PAGE_SIZE) != 0 {
            ok = false;
        }
        for page in 0..32u64 {
            if free_frame(PhysAddr(run.0 + page * PAGE_SIZE)).is_err() {
                ok = false;
            }
        }
        kprintln!("frames: contiguous ok");
    } else {
        ok = false;
        kprintln!("frames: contiguous failed");
    }

    // Drain every CPU cache so the conservation check sees the buddy's real free count.
    set_percpu_enabled(false);
    let before = stats();
    let mut probe = 0usize;
    while probe < 4096 {
        let Some(phys) = alloc_frame() else {
            break;
        };
        frames[0] = phys.0;
        if free_frame(phys).is_err() {
            ok = false;
        }
        probe += 1;
    }
    let after = stats();
    if before.free_bytes != after.free_bytes {
        ok = false;
    }
    kprintln!(
        "frames: oom probe={} free_before={} free_after={} ok={}",
        probe,
        before.free_bytes,
        after.free_bytes,
        ok
    );
    set_percpu_enabled(true);
    ok
}
