// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! User address space: VMA tree, mapping, copy-on-write and teardown.
//!
//! The `raven-vma` tree owns the bookkeeping; this module links it to the `raven-rmm` mapper and
//! the global frame allocator. Each `UserAddressSpace` owns one `AddressSpace` (the page-table
//! root) plus the memory-object mappings that keep shared or private object frames alive.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::prot;
use raven_pager::PageState;
#[cfg(target_arch = "x86_64")]
use raven_rmm::Mapper;
use raven_rmm::page::{NO_EXECUTE, PRESENT};
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};
use raven_vma::{Backing, PAGE_SIZE, Vma, VmaError, Vmas};

use crate::memory::aslr;
use crate::memory::aspace::AddressSpace;
use crate::memory::frame;
use crate::memory::paging;
use crate::process::handle::MemObject;

/// A memory-object mapping kept alongside the VMA tree for frame ownership.
#[derive(Clone)]
pub struct ObjectMapping {
    /// First virtual address.
    pub start: u64,
    /// Length in bytes.
    pub len: u64,
    /// Byte offset inside the memory object.
    pub offset: u64,
    /// The object whose frames back this mapping.
    pub object: Arc<MemObject>,
    /// True for a shared mapping; false for a private copy-on-write mapping.
    pub shared: bool,
}

/// How a mapping is inherited by `PROC_FORK`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapInherit {
    /// The child gets a copy-on-write mapping.
    Inherit,
    /// The mapping and its pages are absent in the child (PG-14).
    NoFork,
    /// The child gets zero-filled pages at the same range (PG-14).
    ZeroOnFork,
}

/// The object a mapping was created from, kept for the `NOFORK` handle rule.
#[derive(Clone, Copy, Debug, Default)]
pub struct InheritIdentity {
    /// Raw handle value passed to `MAP` (`Handle::NONE` for anonymous memory).
    pub handle: u32,
    /// Memory-object id when the handle is a `Memory` or `BootBlob` object.
    pub object: Option<u64>,
}

/// One `NOFORK` handle the child must not receive.
#[derive(Clone, Copy, Debug)]
pub struct NoforkHandle {
    /// Raw handle value the parent used for the mapping.
    pub handle: u32,
    /// Memory-object id; `None` for objects that cannot be verified safely.
    pub object: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct InheritRange {
    start: u64,
    len: u64,
    mode: MapInherit,
    identity: InheritIdentity,
}

impl InheritRange {
    fn contains(&self, address: u64) -> bool {
        address >= self.start && address < self.start.saturating_add(self.len)
    }
}

/// A fault that must be resolved outside the address-space lock.
#[allow(dead_code)] // the `Errno` payload is diagnostic; only the pager variant is actionable
pub enum FaultError {
    /// A normal fault failure.
    Errno(Errno),
    /// A pager-backed page needs a PageIn wait.
    Pager(PagerFault),
}

/// The token `process::handle_page_fault` consumes after releasing the address-space lock.
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
pub struct PagerFault {
    /// The pager-backed object.
    pub object: Arc<MemObject>,
    /// Page index inside the object.
    pub page: u64,
    /// Faulting user address.
    pub cr2: u64,
}

/// One device-window mapping: the physical pages belong to hardware and are never freed.
#[derive(Clone, Copy)]
struct MmioMapping {
    start: u64,
    len: u64,
}

impl MmioMapping {
    fn contains(&self, address: u64) -> bool {
        address >= self.start && address < self.start.saturating_add(self.len)
    }
}

/// One DMA-buffer mapping: the frames are owned by the buffer object, which the mapping keeps
/// alive until it is unmapped.
struct DmaMapping {
    start: u64,
    len: u64,
    offset: u64,
    buffer: Arc<crate::driver::DmaBuffer>,
}

/// Reserved virtual window for kernel-owned IPC capture buffers.
///
/// The range is inserted as a zero-protection VMA in every user address space, so the normal
/// `MAP` allocator never hands it out and `UNMAP`/`PROTECT` reject it. The kernel maps its own
/// frames into individual slots on demand; those frames are not owned by the address space.
pub mod capture_window {
    /// First address of the window.
    pub const BASE: u64 = 0x0000_6000_0000_0000;
    /// Size of one capture slot (the largest capture is `MAX_TRANSFER`).
    pub const SLOT_SIZE: u64 = 4 * 1024 * 1024;
    /// Number of concurrent captures per server process.
    pub const SLOTS: usize = 8;
    /// Total reserved size.
    pub const SIZE: u64 = SLOT_SIZE * SLOTS as u64;

    const _: () = {
        assert!(BASE + SIZE <= super::USER_END);
        assert!(SLOT_SIZE.is_multiple_of(super::PAGE_SIZE));
    };
}

/// A user address space: page tables, VMAs and memory-object ownership.
pub struct UserAddressSpace {
    aspace: AddressSpace,
    vmas: Vmas,
    objects: Vec<ObjectMapping>,
    mmio: Vec<MmioMapping>,
    dma: Vec<DmaMapping>,
    /// Per-range fork inheritance flags and the handle the mapping came from.
    inherit: Vec<InheritRange>,
    /// Bumped on every mapping change. A capture records it at create and only copies back when
    /// the client's mapping has not been replaced meanwhile.
    mapping_generation: u64,
    /// Randomized first anonymous mmap hint for this process image.
    mmap_base: u64,
    /// Randomized initial stack VMA base for this process image.
    stack_base: u64,
}

/// Deferred TLB shootdown and frame recycling after an `unmap`.
///
/// `UserAddressSpace::unmap` runs under the caller's address-space mutex. It must not wait for a
/// remote TLB acknowledgement there: a peer blocked on that mutex would never reach its SGI
/// handler. The caller drops the mutex and then calls [`UnmapDeferred::finish`].
pub struct UnmapDeferred {
    start: u64,
    len: u64,
    frames: Vec<u64>,
}

impl UnmapDeferred {
    /// Flush every remote TLB, then recycle the frames that were owned by the mapping.
    ///
    /// Returns false when a remote CPU timed out; in that case the frames are deliberately
    /// leaked rather than recycled under a potentially stale translation.
    pub fn finish(self) -> bool {
        let flushed = crate::sched::smp::shootdown::flush_range(self.start, self.len);
        if flushed {
            for physical in self.frames {
                let physical = frame::PhysAddr(physical);
                if frame::is_shared(physical) {
                    frame::release_shared(physical);
                } else {
                    let _ = frame::free_frame(physical);
                }
            }
        }
        flushed
    }
}

/// Deferred TLB shootdown after a `protect` under the address-space mutex.
pub struct ProtectDeferred {
    start: u64,
    len: u64,
}

impl ProtectDeferred {
    /// Flush the range after the caller releases the address-space mutex.
    pub fn finish(self) {
        let _ = crate::sched::smp::shootdown::flush_range(self.start, self.len);
    }
}

/// A normal fault error can be returned from code that still speaks `Errno`.
impl From<Errno> for FaultError {
    fn from(error: Errno) -> Self {
        FaultError::Errno(error)
    }
}

impl UserAddressSpace {
    /// Wrap a fresh page-table root with an empty VMA tree.
    pub fn new(aspace: AddressSpace) -> Self {
        let mut vmas = Vmas::new(USER_MIN..USER_END);
        // The window is a compile-time constant inside `USER_MIN..USER_END` and the tree is
        // empty, so the insert cannot fail; keeping the VMA makes `MAP` skip and `UNMAP`/
        // `PROTECT` reject the range.
        let _ = vmas.insert(
            capture_window::BASE,
            capture_window::SIZE,
            0,
            Backing::Anonymous,
        );
        Self {
            aspace,
            vmas,
            objects: Vec::new(),
            mmio: Vec::new(),
            dma: Vec::new(),
            inherit: Vec::new(),
            mapping_generation: 0,
            mmap_base: aslr::mmap_base(),
            stack_base: aslr::stack_base(),
        }
    }

    /// Randomized anonymous-mmap hint for this process image.
    #[must_use]
    pub const fn mmap_base(&self) -> u64 {
        self.mmap_base
    }

    /// Randomized initial stack VMA base for this process image.
    #[must_use]
    pub const fn stack_base(&self) -> u64 {
        self.stack_base
    }

    /// Monotonic mapping generation, bumped by every map/unmap/protect.
    pub fn generation(&self) -> u64 {
        self.mapping_generation
    }

    fn bump_generation(&mut self) {
        self.mapping_generation = self.mapping_generation.wrapping_add(1);
    }

    /// Address-space root for the scheduler.
    pub fn root(&self) -> PhysAddr {
        self.aspace.root()
    }

    /// A mapper over this address space's root.
    pub fn mapper(&mut self) -> Result<paging::KernelMapper<'_>, MapError> {
        self.aspace.mapper()
    }

    /// Read-only VMA view.
    pub fn vmas(&self) -> &Vmas {
        &self.vmas
    }

    /// Mutable VMA view.
    pub fn vmas_mut(&mut self) -> &mut Vmas {
        &mut self.vmas
    }

    /// True when every page of `start..start + len` is a `PROT_NONE` reservation.
    ///
    /// `map::REPLACE` may only overwrite such reservations (CR-24); a range with a hole or any
    /// page carrying a real protection is not replaceable.
    pub fn is_reservation(&self, start: u64, len: u64) -> bool {
        if len == 0
            || !start.is_multiple_of(PAGE_SIZE)
            || !len.is_multiple_of(PAGE_SIZE)
            || !self.vmas.is_fully_mapped(start, len)
        {
            return false;
        }
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        // Only anonymous reservations are replaceable. A file-backed or pager-backed mapping
        // carries dirty pages that the replace path does not page out under the lock.
        if self
            .objects
            .iter()
            .any(|mapping| mapping.start < end && start < mapping.start.saturating_add(mapping.len))
        {
            return false;
        }
        let mut page = start;
        while page < end {
            let Some(vma) = self.vmas.find(page) else {
                return false;
            };
            if vma.prot() != 0 || vma.backing() != Backing::Anonymous {
                return false;
            }
            let Some(next) = page.checked_add(PAGE_SIZE) else {
                return false;
            };
            page = next;
        }
        true
    }

    /// Record how the range just mapped is inherited by a forked child.
    ///
    /// The record is stored outside the VMA tree so `PROTECT` and partial `UNMAP` keep the
    /// mapping's fork behaviour while they split the tree. `identity` names the object handle
    /// when the mapping came from one; a `NOFORK` mapping with a verifiable object handle is
    /// dropped from the child's table as well.
    pub fn set_inherit(
        &mut self,
        start: u64,
        len: u64,
        mode: MapInherit,
        identity: InheritIdentity,
    ) {
        self.inherit.push(InheritRange {
            start,
            len,
            mode,
            identity,
        });
    }

    /// `NOFORK` mappings whose object handle must not exist in the child.
    pub fn nofork_handles(&self) -> Vec<NoforkHandle> {
        let mut handles: Vec<NoforkHandle> = Vec::new();
        for range in &self.inherit {
            if range.mode != MapInherit::NoFork
                || range.identity.handle == raven_abi::types::Handle::NONE.0
                || range.identity.object.is_none()
            {
                continue;
            }
            let entry = NoforkHandle {
                handle: range.identity.handle,
                object: range.identity.object,
            };
            if !handles
                .iter()
                .any(|existing| existing.handle == entry.handle)
            {
                handles.push(entry);
            }
        }
        handles
    }

    /// Handles referenced by mappings that *are* inherited (kept in the child).
    pub fn inherited_handles(&self) -> Vec<u32> {
        let mut handles = Vec::new();
        for range in &self.inherit {
            if range.mode != MapInherit::NoFork
                && range.identity.handle != raven_abi::types::Handle::NONE.0
                && !handles.contains(&range.identity.handle)
            {
                handles.push(range.identity.handle);
            }
        }
        handles
    }

    /// Map a zero-filled anonymous range. The range must already be free.
    pub fn map_anonymous(&mut self, vaddr: u64, len: u64, prot: u8) -> Result<(), Errno> {
        self.vmas
            .insert(vaddr, len, prot, Backing::Anonymous)
            .map_err(vma_errno)?;
        let pages = pages_in(len)?;
        let flags = leaf_flags(prot, prot & (prot::WRITE as u8) != 0);
        let mut mapped = 0u64;
        let mut failure = None;
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                let Some(physical) = frame::alloc_zeroed_frame() else {
                    failure = Some(Errno::ENOMEM);
                    break;
                };
                match map_user_page(&mut mapper, virt, PhysAddr(physical.0), prot, flags) {
                    Ok(()) => mapped = mapped.saturating_add(1),
                    Err(error) => {
                        let _ = frame::free_frame(physical);
                        failure = Some(map_errno(error));
                        break;
                    }
                }
                page = page.saturating_add(1);
            }
        } else {
            failure = Some(Errno::ENOMEM);
        }
        if let Some(error) = failure {
            self.rollback_anonymous(vaddr, mapped);
            let _ = self.vmas.remove(vaddr, len);
            return Err(error);
        }
        self.bump_generation();
        Ok(())
    }

    /// Map a private copy-on-write view of initfs bytes at `offset`.
    pub fn map_initfs(&mut self, vaddr: u64, len: u64, offset: u64, prot: u8) -> Result<(), Errno> {
        let (base, image_len) = initfs_physical_range().ok_or(Errno::EINVAL)?;
        let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
        if end > image_len {
            return Err(Errno::EINVAL);
        }
        self.vmas
            .insert(vaddr, len, prot, Backing::Initfs { offset })
            .map_err(vma_errno)?;
        let pages = pages_in(len)?;
        // Writable file pages start read-only; the first write fault copies the page.
        let flags = leaf_flags(prot, false);
        let mut mapped = 0u64;
        let mut failure = None;
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                let physical = base
                    .checked_add(offset)
                    .and_then(|value| value.checked_add(page.saturating_mul(PAGE_SIZE)))
                    .ok_or(Errno::EINVAL);
                let physical = match physical {
                    Ok(physical) => physical,
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                };
                match map_user_page(&mut mapper, virt, PhysAddr(physical), prot, flags) {
                    Ok(()) => mapped = mapped.saturating_add(1),
                    Err(error) => {
                        failure = Some(map_errno(error));
                        break;
                    }
                }
                page = page.saturating_add(1);
            }
        } else {
            failure = Some(Errno::ENOMEM);
        }
        if let Some(error) = failure {
            self.rollback_foreign(vaddr, mapped);
            let _ = self.vmas.remove(vaddr, len);
            return Err(error);
        }
        self.bump_generation();
        Ok(())
    }

    /// Map a memory object, shared or private copy-on-write.
    pub fn map_memobj(
        &mut self,
        vaddr: u64,
        len: u64,
        offset: u64,
        prot: u8,
        object: Arc<MemObject>,
        shared: bool,
    ) -> Result<(), Errno> {
        let object_len = object.len();
        let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
        if end > object_len {
            return Err(Errno::EINVAL);
        }
        self.vmas
            .insert(vaddr, len, prot, Backing::Anonymous)
            .map_err(vma_errno)?;
        let pages = pages_in(len)?;
        let writable = shared && prot & (prot::WRITE as u8) != 0;
        let flags = leaf_flags(prot, writable);
        let mut failure = None;
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                let object_page = (offset / PAGE_SIZE).saturating_add(page);
                let Some(physical) = object.frame_at(object_page) else {
                    if object.is_paged() {
                        page = page.saturating_add(1);
                        continue;
                    }
                    failure = Some(Errno::EINVAL);
                    break;
                };
                match map_user_page(&mut mapper, virt, PhysAddr(physical), prot, flags) {
                    Ok(()) => {
                        if writable {
                            // C18: a shared writable mapping is conservatively dirty.
                            object.mark_dirty(object_page);
                        }
                    }
                    Err(error) => {
                        failure = Some(map_errno(error));
                        break;
                    }
                }
                page = page.saturating_add(1);
            }
        } else {
            failure = Some(Errno::ENOMEM);
        }
        if let Some(error) = failure {
            // A pager object may have present pages after absent ones, so unmap every page in
            // the attempted range instead of assuming a contiguous prefix was mapped.
            self.rollback_foreign(vaddr, pages);
            let _ = self.vmas.remove(vaddr, len);
            return Err(error);
        }
        self.objects.push(ObjectMapping {
            start: vaddr,
            len,
            offset,
            object,
            shared,
        });
        self.bump_generation();
        Ok(())
    }

    /// Clone `source` into this empty address space for `PROC_FORK`.
    ///
    /// Anonymous private frames (including private copies of initfs and pager pages) are mapped
    /// read-only in both spaces and registered in the shared-frame table. The parent's writable
    /// mappings become read-only so a write on either side takes the COW fault. `NOFORK` ranges
    /// are skipped, `ZERO_ON_FORK` ranges get fresh zeroed frames, shared object mappings keep
    /// their shared writability, and absent pager pages stay absent. Device windows (MMIO and
    /// DMA) are not inherited; the child keeps no mapping for them.
    pub fn fork_clone_from(&mut self, source: &mut UserAddressSpace) -> Result<(), Errno> {
        if self.vmas.len() != 1 || !self.objects.is_empty() {
            return Err(Errno::EINVAL);
        }
        // fork inherits the parent layout exactly; only exec draws a new one.
        self.mmap_base = source.mmap_base;
        self.stack_base = source.stack_base;
        let vmas: Vec<Vma> = source.vmas.iter().copied().collect();
        let objects = source.objects.clone();
        let inherit = source.inherit.clone();
        let mmio = source.mmio.clone();
        let dma: Vec<(u64, u64)> = source
            .dma
            .iter()
            .map(|mapping| (mapping.start, mapping.len))
            .collect();
        // Install the object side table before mapping any page: a later failure drops this
        // address space and `free_user_frames` must be able to tell object-owned pages from
        // private copies even when only part of the clone was built.
        for mapping in &objects {
            let mode = inherit
                .iter()
                .find(|range| range.contains(mapping.start))
                .map_or(MapInherit::Inherit, |range| range.mode);
            if mode == MapInherit::Inherit {
                self.objects.push(mapping.clone());
            }
        }

        let mut parent_mapper = source.aspace.mapper().map_err(map_errno)?;

        for vma in vmas {
            if overlaps_capture_window(vma.start(), vma.len())
                || in_device_ranges(vma.start(), vma.len(), &mmio, &dma)
            {
                continue;
            }
            let entry = inherit
                .iter()
                .find(|range| range.contains(vma.start()))
                .copied();
            let mode = entry.map_or(MapInherit::Inherit, |range| range.mode);
            if mode == MapInherit::NoFork {
                continue;
            }
            let backing = if mode == MapInherit::ZeroOnFork {
                Backing::Anonymous
            } else {
                vma.backing()
            };
            self.vmas
                .insert(vma.start(), vma.len(), vma.prot(), backing)
                .map_err(vma_errno)?;

            let pages = pages_in(vma.len())?;
            let mut child_mapper = self.aspace.mapper().map_err(map_errno)?;
            let mut parent_changed = false;
            let mut page = 0u64;
            while page < pages {
                let Some(address) = vma.start().checked_add(page.saturating_mul(PAGE_SIZE)) else {
                    return Err(Errno::EINVAL);
                };
                let virt = VirtAddr::new(address).ok_or(Errno::EINVAL)?;
                if mode == MapInherit::ZeroOnFork {
                    let physical = frame::alloc_zeroed_frame().ok_or(Errno::ENOMEM)?;
                    let flags = leaf_flags(vma.prot(), vma.prot() & (prot::WRITE as u8) != 0);
                    if let Err(error) = map_user_page(
                        &mut child_mapper,
                        virt,
                        PhysAddr(physical.0),
                        vma.prot(),
                        flags,
                    ) {
                        let _ = frame::free_frame(physical);
                        return Err(map_errno(error));
                    }
                    page = page.saturating_add(1);
                    continue;
                }
                let Some((physical, pte_flags, _)) = parent_mapper.translate(virt) else {
                    // A pager-backed page that was never faulted has no PTE. Its object mapping
                    // is copied below, so the child faults it in through the pager path.
                    if objects
                        .iter()
                        .any(|mapping| mapping.contains(address) && mapping.object.is_paged())
                    {
                        page = page.saturating_add(1);
                        continue;
                    }
                    return Err(Errno::EFAULT);
                };
                let private = private_frame(vma, &objects, address, physical.0);
                let writable = !private
                    && vma.prot() & (prot::WRITE as u8) != 0
                    && objects
                        .iter()
                        .find(|mapping| mapping.contains(address))
                        .is_some_and(|mapping| mapping.shared);
                if private {
                    frame::share_frame(frame::PhysAddr(physical.0));
                }
                if let Err(error) = map_user_page(
                    &mut child_mapper,
                    virt,
                    PhysAddr(physical.0),
                    vma.prot(),
                    leaf_flags(vma.prot(), writable),
                ) {
                    if private {
                        frame::release_shared(frame::PhysAddr(physical.0));
                    }
                    return Err(map_errno(error));
                }
                if private && pte_flags.is_writable() {
                    if let Err(error) = parent_mapper.protect(virt, leaf_flags(vma.prot(), false)) {
                        return Err(map_errno(error));
                    }
                    parent_changed = true;
                }
                page = page.saturating_add(1);
            }
            if parent_changed {
                let _ = crate::sched::smp::shootdown::flush_range(vma.start(), vma.len());
            }
            self.inherit.push(InheritRange {
                start: vma.start(),
                len: vma.len(),
                mode,
                identity: entry.map_or_else(InheritIdentity::default, |range| range.identity),
            });
        }

        self.bump_generation();
        Ok(())
    }

    /// Resolve a futex address to a shared `(object id, byte offset)` key.
    ///
    /// Returns `Ok(None)` for a private or anonymous VMA (the caller keeps the
    /// `(pid, virtual address)` key), and `EINVAL` when the address is outside
    /// the VMA tree. A shared object mapping uses its object id and offset so
    /// two mappings of the same object at different VAs share a queue.
    pub fn shared_futex_key(&self, address: u64) -> Result<Option<(u64, u64)>, Errno> {
        if self.vmas.find(address).is_none() {
            return Err(Errno::EINVAL);
        }
        let Some(mapping) = self
            .objects
            .iter()
            .find(|mapping| mapping.contains(address))
        else {
            return Ok(None);
        };
        if !mapping.shared {
            return Ok(None);
        }
        let offset = address
            .checked_sub(mapping.start)
            .and_then(|delta| mapping.offset.checked_add(delta))
            .ok_or(Errno::EINVAL)?;
        Ok(Some((mapping.object.id(), offset)))
    }

    /// Shared-futex byte range covered by `[start, start + len)`, if shared.
    pub fn shared_futex_range(&self, start: u64, len: u64) -> Option<(u64, u64, u64)> {
        let end = start.checked_add(len)?;
        let mapping = self.objects.iter().find(|mapping| {
            mapping.shared
                && mapping.start <= start
                && mapping.start.saturating_add(mapping.len) >= end
        })?;
        let offset_start = mapping
            .offset
            .checked_add(start.checked_sub(mapping.start)?)?;
        let offset_end = mapping
            .offset
            .checked_add(end.checked_sub(mapping.start)?)?;
        Some((mapping.object.id(), offset_start, offset_end))
    }

    /// Map a device window at `physical` for `len` bytes, uncached and never executable.
    ///
    /// The physical pages belong to hardware: unmapping releases the page-table entries but never
    /// returns them to the frame allocator.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a device mappings are x86-64.
    pub fn map_mmio(
        &mut self,
        vaddr: u64,
        len: u64,
        physical: u64,
        writable: bool,
    ) -> Result<(), Errno> {
        if len == 0 || !vaddr.is_multiple_of(PAGE_SIZE) || !physical.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        let protection = if writable {
            (prot::READ | prot::WRITE) as u8
        } else {
            prot::READ as u8
        };
        self.vmas
            .insert(vaddr, len, protection, Backing::Anonymous)
            .map_err(vma_errno)?;
        let pages = pages_in(len)?;
        let flags = if writable {
            PageFlags::user_rw().union(PageFlags::mmio_attribute())
        } else {
            PageFlags::user_ro().union(PageFlags::mmio_attribute())
        };
        let mut mapped = 0u64;
        let mut failure = None;
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                let Some(address) = physical.checked_add(page.saturating_mul(PAGE_SIZE)) else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                match mapper.map(virt, PhysAddr(address), flags, PageSize::Size4K) {
                    Ok(()) => mapped = mapped.saturating_add(1),
                    Err(error) => {
                        failure = Some(map_errno(error));
                        break;
                    }
                }
                page = page.saturating_add(1);
            }
        } else {
            failure = Some(Errno::ENOMEM);
        }
        if let Some(error) = failure {
            self.rollback_foreign(vaddr, mapped);
            let _ = self.vmas.remove(vaddr, len);
            return Err(error);
        }
        self.mmio.push(MmioMapping { start: vaddr, len });
        self.bump_generation();
        Ok(())
    }

    /// Map a DMA buffer at `offset` for `len` bytes.
    ///
    /// The frames are owned by the buffer object; the mapping keeps the buffer alive and unmapping
    /// never returns the frames to the frame allocator. x86-64 uses cached write-back unless the
    /// buffer was created with the non-cacheable flag; aarch64 always maps Normal-NC (MAIR attr2)
    pub fn map_dma(
        &mut self,
        vaddr: u64,
        len: u64,
        offset: u64,
        writable: bool,
        non_cacheable: bool,
        buffer: Arc<crate::driver::DmaBuffer>,
    ) -> Result<(), Errno> {
        if len == 0 || !vaddr.is_multiple_of(PAGE_SIZE) || !offset.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        let end = offset.checked_add(len).ok_or(Errno::EINVAL)?;
        if end > buffer.len() {
            return Err(Errno::EINVAL);
        }
        let protection = if writable {
            (prot::READ | prot::WRITE) as u8
        } else {
            prot::READ as u8
        };
        self.vmas
            .insert(vaddr, len, protection, Backing::Anonymous)
            .map_err(vma_errno)?;
        let pages = pages_in(len)?;
        let mut flags = if writable {
            PageFlags::user_rw()
        } else {
            PageFlags::user_ro()
        };
        #[cfg(target_arch = "x86_64")]
        if non_cacheable {
            flags = flags.union(PageFlags::mmio_attribute());
        }
        #[cfg(target_arch = "aarch64")]
        {
            // aarch64 devices are not guaranteed to snoop the CPU cache and the ABI has no
            // cache-maintenance syscall, so every DMA buffer is Normal-NC (MAIR attr2).
            let _ = non_cacheable;
            flags = flags.union(PageFlags::from_bits(raven_rmm::page::NON_CACHEABLE));
        }
        let mut mapped = 0u64;
        let mut failure = None;
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                let Some(physical) = buffer.frame_at((offset / PAGE_SIZE).saturating_add(page))
                else {
                    failure = Some(Errno::EINVAL);
                    break;
                };
                match mapper.map(virt, PhysAddr(physical), flags, PageSize::Size4K) {
                    Ok(()) => mapped = mapped.saturating_add(1),
                    Err(error) => {
                        failure = Some(map_errno(error));
                        break;
                    }
                }
                page = page.saturating_add(1);
            }
        } else {
            failure = Some(Errno::ENOMEM);
        }
        if let Some(error) = failure {
            self.rollback_foreign(vaddr, mapped);
            let _ = self.vmas.remove(vaddr, len);
            return Err(error);
        }
        self.dma.push(DmaMapping {
            start: vaddr,
            len,
            offset,
            buffer,
        });
        self.bump_generation();
        Ok(())
    }

    /// Remove a fully mapped range, splitting VMAs and releasing object references.
    pub fn unmap(&mut self, start: u64, len: u64) -> Result<UnmapDeferred, Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        if overlaps_capture_window(start, len) {
            // The window is kernel-owned; a user unmap would free capture frames the IPC layer
            // still owns and could recycle them under a live server.
            return Err(Errno::EINVAL);
        }
        if !self.vmas.is_fully_mapped(start, len) {
            return Err(Errno::EINVAL);
        }
        let pages = pages_in(len)?;
        let mmio = self.mmio.clone();
        let dma = self
            .dma
            .iter()
            .map(|mapping| (mapping.start, mapping.len))
            .collect::<Vec<_>>();
        let mut to_free: Vec<u64> = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
        {
            let mut mapper = self.aspace.mapper().map_err(map_errno)?;
            let mut owned = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(start.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    return Err(Errno::EINVAL);
                };
                let Some((physical, _, _)) = mapper.translate(virt) else {
                    // A pager-backed page that was never faulted has no PTE yet; it is still
                    // part of the VMA and must be dropped with the range (C18).
                    let pager_page = self
                        .objects
                        .iter()
                        .any(|mapping| mapping.contains(virt.data()) && mapping.object.is_paged());
                    if pager_page {
                        page = page.saturating_add(1);
                        continue;
                    }
                    return Err(Errno::EINVAL);
                };
                let vma = self.vmas.find(virt.data()).ok_or(Errno::EINVAL)?;
                let in_device_range = mmio.iter().any(|m| m.contains(virt.data()))
                    || dma.iter().any(|(map_start, map_len)| {
                        virt.data() >= *map_start
                            && virt.data() < map_start.saturating_add(*map_len)
                    });
                let private =
                    !in_device_range && private_frame(vma, &self.objects, virt.data(), physical.0);
                let owns = private && page_owned(vma, &self.objects, virt.data(), physical.0);
                let shared = private && frame::is_shared(frame::PhysAddr(physical.0));
                owned.push((virt, physical.0, owns || shared));
                page = page.saturating_add(1);
            }
            for (virt, physical, releasable) in owned {
                mapper.unmap(virt).map_err(map_errno)?;
                if releasable {
                    to_free.push(physical);
                }
            }
        }
        self.vmas.remove(start, len).map_err(vma_errno)?;
        self.remove_object_range(start, len);
        self.remove_inherit_range(start, len);
        self.remove_mmio_range(start, len);
        self.remove_dma_range(start, len);
        self.bump_generation();
        // The caller drops the address-space mutex before `finish`; waiting for a remote TLB
        // acknowledgement under that mutex can starve an aarch64 peer that is blocked on it.
        Ok(UnmapDeferred {
            start,
            len,
            frames: to_free,
        })
    }

    /// Change protections over a fully mapped range.
    pub fn protect(
        &mut self,
        start: u64,
        len: u64,
        new_prot: u8,
    ) -> Result<ProtectDeferred, Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        if overlaps_capture_window(start, len) {
            return Err(Errno::EINVAL);
        }
        if self.overlaps_mmio(start, len) {
            // Device windows carry cache attributes and rights semantics that `protect` must
            // not silently change.
            return Err(Errno::EINVAL);
        }
        if new_prot & !(prot::MASK as u8) != 0
            || (new_prot & (prot::WRITE as u8) != 0 && new_prot & (prot::EXEC as u8) != 0)
        {
            return Err(Errno::EINVAL);
        }
        if !self.vmas.is_fully_mapped(start, len) {
            return Err(Errno::ENOMEM);
        }
        let pages = pages_in(len)?;
        {
            let mut mapper = self.aspace.mapper().map_err(map_errno)?;
            let mut plans = Vec::with_capacity(usize::try_from(pages).unwrap_or(0));
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(start.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    return Err(Errno::EINVAL);
                };
                let vma = self.vmas.find(virt.data()).ok_or(Errno::ENOMEM)?;
                // Initfs pages are an immutable source image. A mapping may start writable
                // (private COW), but PROTECT must never turn a read-only initfs mapping into a
                // writable-in-place one: that would bypass the COW copy and expose the archive.
                if vma.prot() & (prot::WRITE as u8) == 0
                    && new_prot & (prot::WRITE as u8) != 0
                    && matches!(vma.backing(), Backing::Initfs { .. })
                {
                    return Err(Errno::EACCES);
                }
                let Some((physical, _, _)) = mapper.translate(virt) else {
                    let pager_page = self
                        .objects
                        .iter()
                        .any(|mapping| mapping.contains(virt.data()) && mapping.object.is_paged());
                    if pager_page {
                        page = page.saturating_add(1);
                        continue;
                    }
                    return Err(Errno::ENOMEM);
                };
                let writable_now = new_prot & (prot::WRITE as u8) != 0
                    && private_frame(vma, &self.objects, virt.data(), physical.0)
                    && !frame::is_shared(frame::PhysAddr(physical.0));
                plans.push((virt, leaf_flags(new_prot, writable_now)));
                page = page.saturating_add(1);
            }
            for (virt, flags) in plans {
                mapper.protect(virt, flags).map_err(map_errno)?;
            }
        }
        self.vmas.protect(start, len, new_prot).map_err(vma_errno)?;
        self.bump_generation();
        Ok(ProtectDeferred { start, len })
    }

    /// Handle a user page fault at `cr2`.
    ///
    /// A resolved COW fault returns `Ok(())`; a pager-backed absent page returns a `PagerFault`
    /// token so the caller can drop the address-space lock before the PageIn wait (C18).
    pub fn handle_fault(&mut self, cr2: u64, write: bool) -> Result<(), FaultError> {
        if !(USER_MIN..USER_END).contains(&cr2) {
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        let vma = self
            .vmas
            .find(cr2)
            .ok_or(FaultError::Errno(Errno::EFAULT))?;
        let writable_vma = vma.prot() & (prot::WRITE as u8) != 0;
        if let Some(mapping) = self
            .objects
            .iter()
            .find(|mapping| mapping.contains(cr2))
            .cloned()
            && mapping.object.is_paged()
        {
            // A write to a read-only VMA is a protection fault, not a page-in: never map the
            // page read-only here or the retried store would fault forever.
            if write && !writable_vma {
                return Err(FaultError::Errno(Errno::EFAULT));
            }
            let object_page = mapping
                .offset
                .saturating_add(cr2.saturating_sub(mapping.start))
                / PAGE_SIZE;
            match mapping.object.page_state(object_page) {
                Some(PageState::Present | PageState::Dirty | PageState::Writing) => {
                    // A read fault, or any fault on a shared mapping, only needs the object
                    // frame in the PTE. Shared writeback marks the page dirty when it maps it
                    // writable.
                    if !write || mapping.shared {
                        return self.map_pager_page(cr2).map_err(FaultError::Errno);
                    }
                    let pte_writable = {
                        let virt = VirtAddr::new(cr2 & !(PAGE_SIZE - 1))
                            .ok_or(FaultError::Errno(Errno::EFAULT))?;
                        self.aspace
                            .mapper()
                            .ok()
                            .and_then(|mut mapper| mapper.translate(virt))
                            .is_some_and(|(_, flags, _)| flags.is_writable())
                    };
                    if pte_writable {
                        // A protect() callback may make an already-private COW page writable
                        // again. Remapping the object frame here would discard the private copy.
                        return Ok(());
                    }
                    // A write to a private pager page must COW the present frame below.
                }
                Some(PageState::Failed) => return Err(FaultError::Errno(Errno::EIO)),
                Some(PageState::Absent | PageState::InFlight) | None => {
                    return Err(FaultError::Pager(PagerFault {
                        object: Arc::clone(&mapping.object),
                        page: object_page,
                        cr2,
                    }));
                }
            }
        }
        if !writable_vma {
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        let page = cr2 & !(PAGE_SIZE - 1);
        let virt = VirtAddr::new(page).ok_or(FaultError::Errno(Errno::EFAULT))?;
        let mut mapper = self.aspace.mapper().map_err(map_errno)?;
        let Some((physical, flags, _)) = mapper.translate(virt) else {
            return Err(FaultError::Errno(Errno::EFAULT));
        };
        if flags.is_writable() {
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        let private = private_frame(vma, &self.objects, cr2, physical.0);
        let shared = private && frame::is_shared(frame::PhysAddr(physical.0));
        if private && !shared {
            let flags = leaf_flags(vma.prot(), true);
            let result = mapper.protect(virt, flags).map_err(map_errno);
            result.map_err(FaultError::Errno)?;
            let _ = crate::sched::smp::shootdown::flush_range(page, PAGE_SIZE);
            self.bump_generation();
            return Ok(());
        }
        // Copy-on-write: the faulting page points into the initfs, the memory object, or a
        // frame still shared with a forked child.
        let Some(new_frame) = frame::alloc_zeroed_frame() else {
            return Err(FaultError::Errno(Errno::ENOMEM));
        };
        let source = frame::phys_to_virt(frame::PhysAddr(physical.0));
        let destination = frame::phys_to_virt(new_frame);
        if source.is_null() || destination.is_null() {
            let _ = frame::free_frame(new_frame);
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        // SAFETY: both pointers refer to whole 4 KiB frames; the source belongs to the initfs or
        // memory object and the destination is a fresh frame.
        unsafe {
            core::ptr::copy_nonoverlapping(source, destination, PAGE_SIZE as usize);
        }
        if mapper.unmap(virt).is_err() {
            let _ = frame::free_frame(new_frame);
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        let flags = leaf_flags(vma.prot(), true);
        if mapper
            .map(virt, PhysAddr(new_frame.0), flags, PageSize::Size4K)
            .is_err()
        {
            let _ = frame::free_frame(new_frame);
            return Err(FaultError::Errno(Errno::EFAULT));
        }
        if shared {
            frame::release_shared(frame::PhysAddr(physical.0));
        }
        let _ = crate::sched::smp::shootdown::flush_range(page, PAGE_SIZE);
        self.bump_generation();
        Ok(())
    }

    /// Map a supplied pager page into the faulting address space.
    pub fn map_pager_page(&mut self, cr2: u64) -> Result<(), Errno> {
        let mapping = self
            .objects
            .iter()
            .find(|mapping| mapping.contains(cr2))
            .cloned()
            .ok_or(Errno::EFAULT)?;
        let object_page = mapping
            .offset
            .saturating_add(cr2.saturating_sub(mapping.start))
            / PAGE_SIZE;
        let physical = mapping.object.frame_at(object_page).ok_or(Errno::ENOENT)?;
        let vma = self.vmas.find(cr2).ok_or(Errno::EFAULT)?;
        let writable = mapping.shared && vma.prot() & (prot::WRITE as u8) != 0;
        let flags = leaf_flags(vma.prot(), writable);
        let virt = VirtAddr::new(cr2 & !(PAGE_SIZE - 1)).ok_or(Errno::EFAULT)?;
        let mut mapper = self.aspace.mapper().map_err(map_errno)?;
        match mapper.translate(virt) {
            Some((current, _, _)) if current.0 == physical => {
                mapper.protect(virt, flags).map_err(map_errno)?;
            }
            Some(_) => {
                let _ = mapper.unmap(virt);
                map_user_page(&mut mapper, virt, PhysAddr(physical), vma.prot(), flags)
                    .map_err(map_errno)?;
            }
            None => {
                map_user_page(&mut mapper, virt, PhysAddr(physical), vma.prot(), flags)
                    .map_err(map_errno)?;
            }
        }
        if writable {
            mapping.object.mark_dirty(object_page);
        }
        let _ = crate::sched::smp::shootdown::flush_range(virt.data(), PAGE_SIZE);
        self.bump_generation();
        Ok(())
    }

    /// Dirty pager pages touched by an unmap range, for PageOut after the address-space lock is
    /// released.
    pub fn collect_pager_dirty(&self, start: u64, len: u64) -> Vec<(Arc<MemObject>, u64)> {
        let end = start.saturating_add(len);
        let mut dirty = Vec::new();
        for mapping in &self.objects {
            if !mapping.object.is_paged() || !mapping.shared {
                continue;
            }
            let from = start.max(mapping.start);
            let to = end.min(mapping.start.saturating_add(mapping.len));
            if from >= to {
                continue;
            }
            let first = mapping
                .offset
                .saturating_add(from.saturating_sub(mapping.start))
                / PAGE_SIZE;
            let last = mapping
                .offset
                .saturating_add(to.saturating_sub(1).saturating_sub(mapping.start))
                / PAGE_SIZE;
            let mut page = first;
            while page <= last {
                if mapping.object.page_state(page) == Some(PageState::Dirty) {
                    dirty.push((Arc::clone(&mapping.object), page));
                }
                page = page.saturating_add(1);
            }
        }
        dirty
    }

    /// Count the present user pages in this address space (K449 test-only metric).
    ///
    /// Walks every VMA and counts translated 4 KiB leaves, including initfs and memory-object
    /// pages. The kernel-owned capture window is skipped. This is diagnostic and changes no
    /// mapping.
    pub fn mapped_pages(&mut self) -> u64 {
        let vmas: Vec<Vma> = self.vmas.iter().copied().collect();
        let mut pages = 0u64;
        if let Ok(mut mapper) = self.aspace.mapper() {
            for vma in vmas {
                if overlaps_capture_window(vma.start(), vma.len()) {
                    continue;
                }
                let count = pages_in(vma.len()).unwrap_or(0);
                let mut page = 0u64;
                while page < count {
                    let address = vma.start().saturating_add(page.saturating_mul(PAGE_SIZE));
                    if let Some(virt) = VirtAddr::new(address)
                        && mapper.translate(virt).is_some()
                    {
                        pages = pages.saturating_add(1);
                    }
                    page = page.saturating_add(1);
                }
            }
        }
        pages
    }

    /// Free every data frame owned by this address space and clear the VMA tree.
    pub fn free_user_frames(&mut self) {
        if paging::cmdline()
            .split_ascii_whitespace()
            .any(|token| token == "s198-uspacetrack")
        {
            crate::kprintln!(
                "s198: uspace release vmas={} inherit={}/{} objects={} mmio={} dma={}",
                self.vmas.len(),
                self.inherit.len(),
                self.inherit.capacity(),
                self.objects.len(),
                self.mmio.len(),
                self.dma.len()
            );
        }
        let vmas: Vec<Vma> = self.vmas.iter().copied().collect();
        let objects = self.objects.clone();
        let mmio = self.mmio.clone();
        let dma = self
            .dma
            .iter()
            .map(|mapping| (mapping.start, mapping.len))
            .collect::<Vec<_>>();
        if let Ok(mut mapper) = self.aspace.mapper() {
            for vma in vmas {
                let pages = pages_in(vma.len()).unwrap_or(0);
                let mut page = 0u64;
                while page < pages {
                    let address = vma.start().saturating_add(page.saturating_mul(PAGE_SIZE));
                    let Some(virt) = VirtAddr::new(address) else {
                        break;
                    };
                    let in_device_range = mmio.iter().any(|mapping| mapping.contains(address))
                        || dma.iter().any(|(map_start, map_len)| {
                            address >= *map_start && address < map_start.saturating_add(*map_len)
                        })
                        || is_capture_address(address);
                    if let Some((physical, _, _)) = mapper.translate(virt)
                        && !in_device_range
                        && private_frame(vma, &objects, address, physical.0)
                    {
                        let physical = frame::PhysAddr(physical.0);
                        if frame::is_shared(physical) {
                            frame::release_shared(physical);
                        } else if page_owned(vma, &objects, address, physical.0) {
                            let _ = frame::free_frame(physical);
                        }
                    }
                    page = page.saturating_add(1);
                }
            }
        }
        self.vmas = Vmas::new(USER_MIN..USER_END);
        self.objects.clear();
        self.mmio.clear();
        self.dma.clear();
        self.inherit.clear();
    }

    /// Release an address space that is currently active: free its data frames, switch back to the
    /// kernel address space and then free its page tables while running on the kernel stack.
    pub fn release(mut self) {
        self.free_user_frames();
        activate_kernel_space();
        drop(self);
    }

    fn rollback_anonymous(&mut self, vaddr: u64, pages: u64) {
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    break;
                };
                if let Ok(physical) = mapper.unmap(virt) {
                    let _ = frame::free_frame(frame::PhysAddr(physical.0));
                }
                page = page.saturating_add(1);
            }
        }
    }

    fn rollback_foreign(&mut self, vaddr: u64, pages: u64) {
        if let Ok(mut mapper) = self.aspace.mapper() {
            let mut page = 0u64;
            while page < pages {
                let Some(virt) =
                    VirtAddr::new(vaddr.saturating_add(page.saturating_mul(PAGE_SIZE)))
                else {
                    break;
                };
                let _ = mapper.unmap(virt);
                page = page.saturating_add(1);
            }
        }
    }

    fn remove_object_range(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let mut next = Vec::with_capacity(self.objects.len().saturating_add(2));
        for mapping in &self.objects {
            let mapping_end = mapping.start.saturating_add(mapping.len);
            if mapping_end <= start || mapping.start >= end {
                next.push(mapping.clone());
                continue;
            }
            if mapping.start < start {
                next.push(ObjectMapping {
                    len: start.saturating_sub(mapping.start),
                    ..mapping.clone()
                });
            }
            if mapping_end > end {
                next.push(ObjectMapping {
                    start: end,
                    len: mapping_end.saturating_sub(end),
                    offset: mapping
                        .offset
                        .saturating_add(end.saturating_sub(mapping.start)),
                    ..mapping.clone()
                });
            }
        }
        self.objects = next;
    }

    fn remove_inherit_range(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let mut next = Vec::with_capacity(self.inherit.len().saturating_add(2));
        for range in &self.inherit {
            let range_end = range.start.saturating_add(range.len);
            if range_end <= start || range.start >= end {
                next.push(*range);
                continue;
            }
            if range.start < start {
                next.push(InheritRange {
                    len: start.saturating_sub(range.start),
                    ..*range
                });
            }
            if range_end > end {
                next.push(InheritRange {
                    start: end,
                    len: range_end.saturating_sub(end),
                    ..*range
                });
            }
        }
        self.inherit = next;
    }

    fn overlaps_mmio(&self, start: u64, len: u64) -> bool {
        let end = start.saturating_add(len);
        self.mmio
            .iter()
            .any(|mapping| start < mapping.start.saturating_add(mapping.len) && mapping.start < end)
    }

    fn remove_mmio_range(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let mut next = Vec::with_capacity(self.mmio.len().saturating_add(2));
        for mapping in &self.mmio {
            let mapping_end = mapping.start.saturating_add(mapping.len);
            if mapping_end <= start || mapping.start >= end {
                next.push(*mapping);
                continue;
            }
            if mapping.start < start {
                next.push(MmioMapping {
                    start: mapping.start,
                    len: start.saturating_sub(mapping.start),
                });
            }
            if mapping_end > end {
                next.push(MmioMapping {
                    start: end,
                    len: mapping_end.saturating_sub(end),
                });
            }
        }
        self.mmio = next;
    }

    fn remove_dma_range(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let mut next = Vec::with_capacity(self.dma.len().saturating_add(2));
        for mapping in self.dma.drain(..) {
            let mapping_end = mapping.start.saturating_add(mapping.len);
            if mapping_end <= start || mapping.start >= end {
                next.push(mapping);
                continue;
            }
            if mapping.start < start {
                next.push(DmaMapping {
                    start: mapping.start,
                    len: start.saturating_sub(mapping.start),
                    offset: mapping.offset,
                    buffer: Arc::clone(&mapping.buffer),
                });
            }
            if mapping_end > end {
                next.push(DmaMapping {
                    start: end,
                    len: mapping_end.saturating_sub(end),
                    offset: mapping
                        .offset
                        .saturating_add(end.saturating_sub(mapping.start)),
                    buffer: mapping.buffer,
                });
            }
        }
        self.dma = next;
    }
}

impl Drop for UserAddressSpace {
    fn drop(&mut self) {
        self.free_user_frames();
    }
}

impl ObjectMapping {
    fn contains(&self, address: u64) -> bool {
        address >= self.start && address < self.start.saturating_add(self.len)
    }
}

/// True when this address space owns a private frame at `address`.
///
/// Initfs image pages and shared or not-yet-copied object pages belong to another owner. A
/// private copy of either is process-owned even when a fork made it shared.
fn private_frame(vma: Vma, objects: &[ObjectMapping], address: u64, physical: u64) -> bool {
    match vma.backing() {
        Backing::Initfs { .. } => !initfs_contains(physical),
        Backing::Anonymous => {
            if let Some(mapping) = objects.iter().find(|mapping| mapping.contains(address)) {
                !mapping.shared
                    && mapping.object.frame_at(
                        mapping
                            .offset
                            .saturating_add(address.saturating_sub(mapping.start))
                            / PAGE_SIZE,
                    ) != Some(physical)
            } else {
                true
            }
        }
    }
}

/// True when this address space is the sole owner of a private frame.
///
/// A frame shared with a forked child is released through [`frame::release_shared`] instead of
/// being freed, so teardown cannot free a frame the child still maps.
fn page_owned(vma: Vma, objects: &[ObjectMapping], address: u64, physical: u64) -> bool {
    private_frame(vma, objects, address, physical) && !frame::is_shared(frame::PhysAddr(physical))
}

fn in_device_ranges(start: u64, len: u64, mmio: &[MmioMapping], dma: &[(u64, u64)]) -> bool {
    let end = start.saturating_add(len);
    mmio.iter()
        .any(|mapping| start < mapping.start.saturating_add(mapping.len) && mapping.start < end)
        || dma.iter().any(|(map_start, map_len)| {
            start < map_start.saturating_add(*map_len) && *map_start < end
        })
}

fn pages_in(len: u64) -> Result<u64, Errno> {
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    Ok(len / PAGE_SIZE)
}

fn leaf_flags(prot: u8, writable: bool) -> PageFlags {
    if prot == 0 {
        none_leaf_flags()
    } else if prot & (prot::EXEC as u8) != 0 {
        PageFlags::user_rx()
    } else if writable && prot & (prot::WRITE as u8) != 0 {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    }
}

/// A present but supervisor-only leaf, the x86-64 encoding of `PROT_NONE`.
///
/// The hardware has no separate "no read" bit, so clearing the user bit is what makes every
/// ring-3 access fault. The frame stays present so `UNMAP` and `PROTECT` can find and re-use it.
fn none_leaf_flags() -> PageFlags {
    PageFlags::from_bits(PRESENT | NO_EXECUTE)
}

/// True when every present leaf in `[address, address + length)` is user-accessible.
///
/// x86-64 `rep movsb` at CPL0 ignores the U/S bit on reads, so `PROT_NONE`'s present
/// supervisor-only leaf is readable by `copy_from_user` unless the kernel checks it in software.
/// A not-present page is allowed through: the copy's fault fixup/VM fault path is responsible for
/// resolving it or returning EFAULT. A present supervisor leaf is rejected with EFAULT here.
///
/// The walk is one page-table traversal per 4 KiB page in the range; `copy_to_user` does not need
/// it because a user write to the supervisor leaf faults in hardware.
#[cfg(target_arch = "x86_64")]
pub fn user_range_user_accessible(root: u64, address: u64, length: u64) -> bool {
    let Some(end) = address.checked_add(length) else {
        return false;
    };
    let mut allocator = paging::KernelFrameAllocator;
    let memory = paging::KernelPhysMemory;
    let mut tlb = paging::KernelTlb;
    let Ok(mut mapper) = Mapper::new(PhysAddr(root), &mut allocator, &memory, &mut tlb) else {
        return false;
    };
    let mut page = address & !(PAGE_SIZE - 1);
    while page < end {
        let Some(virt) = VirtAddr::new(page) else {
            return false;
        };
        if let Some((_, flags, _)) = mapper.translate(virt)
            && flags.is_present()
            && !flags.is_user()
        {
            return false;
        }
        page = page.saturating_add(PAGE_SIZE);
    }
    true
}

/// Map one 4 KiB user leaf, seeding the intermediate tables as user-owned for `PROT_NONE`.
///
/// The mapper derives the user bit of every intermediate table from the flags of the *first*
/// mapping under it. A reservation therefore has to be created with a user leaf and narrowed to
/// supervisor-only afterwards; otherwise a later `PROTECT` could change the leaf but the user
/// access would still stop at a supervisor-only table.
fn map_user_page(
    mapper: &mut paging::KernelMapper<'_>,
    virt: VirtAddr,
    physical: PhysAddr,
    prot: u8,
    flags: PageFlags,
) -> Result<(), MapError> {
    let seed = if prot == 0 {
        PageFlags::user_ro()
    } else {
        flags
    };
    mapper.map(virt, physical, seed, PageSize::Size4K)?;
    if prot == 0 {
        mapper.protect(virt, none_leaf_flags())?;
    }
    Ok(())
}

fn initfs_physical_range() -> Option<(u64, u64)> {
    // SAFETY: paging published the copied BootInfo before user processes exist and it is
    // immutable afterwards.
    let info = unsafe { &*paging::boot_info_ptr() };
    if info.initfs.base == 0 || info.initfs.len == 0 {
        None
    } else {
        Some((info.initfs.base, info.initfs.len))
    }
}

fn initfs_contains(physical: u64) -> bool {
    initfs_physical_range()
        .is_some_and(|(base, len)| physical >= base && physical < base.saturating_add(len))
}

fn is_capture_address(address: u64) -> bool {
    address >= capture_window::BASE
        && address < capture_window::BASE.saturating_add(capture_window::SIZE)
}

fn overlaps_capture_window(start: u64, len: u64) -> bool {
    let end = start.saturating_add(len);
    start < capture_window::BASE.saturating_add(capture_window::SIZE) && capture_window::BASE < end
}

fn vma_errno(error: VmaError) -> Errno {
    match error {
        VmaError::Overlap => Errno::EEXIST,
        VmaError::Unmapped => Errno::EINVAL,
        _ => Errno::EINVAL,
    }
}

fn map_errno(error: MapError) -> Errno {
    match error {
        MapError::OutOfFrames => Errno::ENOMEM,
        MapError::WriteExecute => Errno::EINVAL,
        MapError::AlreadyMapped => Errno::EEXIST,
        _ => Errno::EINVAL,
    }
}

/// Activate a user page-table root on the current CPU.
///
/// # Safety
///
/// `root` must be the live root of an `AddressSpace` owned by the caller, and this code and its
/// kernel stack must be mapped in the shared kernel half of both the old and the new root. The
/// caller must not touch the old user mappings after the switch.
pub unsafe fn activate_user_root(root: u64, asid: u16) {
    #[cfg(target_arch = "x86_64")]
    {
        let _ = asid;
        // SAFETY: the caller guarantees `root` is a live page-table root; reloading CR3 also
        // flushes non-global TLB entries for the old user mappings.
        unsafe {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) root,
                options(nostack, preserves_flags),
            );
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::mmu::switch_ttbr0(root, asid);
    }
}

/// Switch to the kernel address space (x86-64: the kernel CR3).
#[cfg(target_arch = "x86_64")]
fn activate_kernel_space() {
    let Some(root) = paging::kernel_root() else {
        crate::arch::halt();
    };
    // SAFETY: `root` is the kernel page-table root; the current kernel stack and code are mapped
    // in the kernel half of every address space.
    unsafe {
        activate_root(root.0);
    }
}

/// Switch to the kernel address space (aarch64: the empty TTBR0 root; TTBR1 is always kernel).
#[cfg(target_arch = "aarch64")]
fn activate_kernel_space() {
    let Some(root) = paging::empty_user_root() else {
        crate::arch::halt();
    };
    crate::arch::mmu::switch_ttbr0(root.0, 0);
}

#[cfg(target_arch = "x86_64")]
unsafe fn activate_root(root: u64) {
    // SAFETY: `root` is the kernel page-table root; the current kernel stack and code are mapped
    // in the kernel half of every address space.
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) root,
            options(nostack, preserves_flags),
        );
    }
}
