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
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};
use raven_vma::{Backing, PAGE_SIZE, Vma, VmaError, Vmas};

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
#[cfg(target_arch = "x86_64")]
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
    /// Bumped on every mapping change. A capture records it at create and only copies back when
    /// the client's mapping has not been replaced meanwhile.
    mapping_generation: u64,
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
        #[cfg(target_arch = "x86_64")]
        let vmas = {
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
            vmas
        };
        #[cfg(not(target_arch = "x86_64"))]
        let vmas = Vmas::new(USER_MIN..USER_END);
        Self {
            aspace,
            vmas,
            objects: Vec::new(),
            mmio: Vec::new(),
            dma: Vec::new(),
            mapping_generation: 0,
        }
    }

    /// Monotonic mapping generation, bumped by every map/unmap/protect.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S32 captures are x86-64 only.
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
                match mapper.map(virt, PhysAddr(physical.0), flags, PageSize::Size4K) {
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
                match mapper.map(virt, PhysAddr(physical), flags, PageSize::Size4K) {
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
    pub fn unmap(&mut self, start: u64, len: u64) -> Result<(), Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        #[cfg(target_arch = "x86_64")]
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
                let owns =
                    !in_device_range && page_owned(vma, &self.objects, virt.data(), physical.0);
                owned.push((virt, physical.0, owns));
                page = page.saturating_add(1);
            }
            for (virt, physical, owns) in owned {
                mapper.unmap(virt).map_err(map_errno)?;
                if owns {
                    to_free.push(physical);
                }
            }
        }
        self.vmas.remove(start, len).map_err(vma_errno)?;
        self.remove_object_range(start, len);
        self.remove_mmio_range(start, len);
        self.remove_dma_range(start, len);
        // Do not recycle frames until every CPU has dropped its translation: a stale TLB entry
        // could otherwise reach a frame that was already handed to another address space.
        let flushed = crate::sched::smp::shootdown::flush_range(start, len);
        if flushed {
            for physical in to_free {
                let _ = frame::free_frame(frame::PhysAddr(physical));
            }
        }
        self.bump_generation();
        Ok(())
    }

    /// Change protections over a fully mapped range.
    pub fn protect(&mut self, start: u64, len: u64, new_prot: u8) -> Result<(), Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        #[cfg(target_arch = "x86_64")]
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
                let writable_now = if new_prot & (prot::WRITE as u8) == 0 {
                    false
                } else {
                    match vma.backing() {
                        Backing::Initfs { .. } => !initfs_contains(physical.0),
                        Backing::Anonymous => {
                            if let Some(mapping) = self
                                .objects
                                .iter()
                                .find(|mapping| mapping.contains(virt.data()))
                            {
                                mapping.shared
                                    || mapping.object.frame_at(
                                        mapping.offset.saturating_add(
                                            virt.data().saturating_sub(mapping.start),
                                        ) / PAGE_SIZE,
                                    ) != Some(physical.0)
                            } else {
                                true
                            }
                        }
                    }
                };
                plans.push((virt, leaf_flags(new_prot, writable_now)));
                page = page.saturating_add(1);
            }
            for (virt, flags) in plans {
                mapper.protect(virt, flags).map_err(map_errno)?;
            }
        }
        self.vmas.protect(start, len, new_prot).map_err(vma_errno)?;
        let _ = crate::sched::smp::shootdown::flush_range(start, len);
        self.bump_generation();
        Ok(())
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
        let already_private = match vma.backing() {
            Backing::Initfs { .. } => !initfs_contains(physical.0),
            Backing::Anonymous => {
                if let Some(mapping) = self.objects.iter().find(|mapping| mapping.contains(cr2)) {
                    if mapping.shared {
                        false
                    } else {
                        mapping.object.frame_at(
                            mapping
                                .offset
                                .saturating_add(cr2.saturating_sub(mapping.start))
                                / PAGE_SIZE,
                        ) != Some(physical.0)
                    }
                } else {
                    true
                }
            }
        };
        if already_private {
            let flags = leaf_flags(vma.prot(), true);
            let result = mapper.protect(virt, flags).map_err(map_errno);
            result.map_err(FaultError::Errno)?;
            let _ = crate::sched::smp::shootdown::flush_range(page, PAGE_SIZE);
            self.bump_generation();
            return Ok(());
        }
        // Copy-on-write: the faulting page still points into the initfs or the memory object.
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
                mapper
                    .map(virt, PhysAddr(physical), flags, PageSize::Size4K)
                    .map_err(map_errno)?;
            }
            None => mapper
                .map(virt, PhysAddr(physical), flags, PageSize::Size4K)
                .map_err(map_errno)?,
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

    /// Free every data frame owned by this address space and clear the VMA tree.
    pub fn free_user_frames(&mut self) {
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
                        && page_owned(vma, &objects, address, physical.0)
                    {
                        let _ = frame::free_frame(frame::PhysAddr(physical.0));
                    }
                    page = page.saturating_add(1);
                }
            }
        }
        self.vmas = Vmas::new(USER_MIN..USER_END);
        self.objects.clear();
        self.mmio.clear();
        self.dma.clear();
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

fn page_owned(vma: Vma, objects: &[ObjectMapping], address: u64, physical: u64) -> bool {
    match vma.backing() {
        Backing::Initfs { .. } => !initfs_contains(physical),
        Backing::Anonymous => {
            if let Some(mapping) = objects.iter().find(|mapping| mapping.contains(address)) {
                if mapping.shared {
                    false
                } else {
                    mapping.object.frame_at(
                        mapping
                            .offset
                            .saturating_add(address.saturating_sub(mapping.start))
                            / PAGE_SIZE,
                    ) != Some(physical)
                }
            } else {
                true
            }
        }
    }
}

fn pages_in(len: u64) -> Result<u64, Errno> {
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    Ok(len / PAGE_SIZE)
}

fn leaf_flags(prot: u8, writable: bool) -> PageFlags {
    if prot & (prot::EXEC as u8) != 0 {
        PageFlags::user_rx()
    } else if writable && prot & (prot::WRITE as u8) != 0 {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    }
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

#[cfg(target_arch = "x86_64")]
fn is_capture_address(address: u64) -> bool {
    address >= capture_window::BASE
        && address < capture_window::BASE.saturating_add(capture_window::SIZE)
}

#[cfg(not(target_arch = "x86_64"))]
fn is_capture_address(_address: u64) -> bool {
    false
}

#[cfg(target_arch = "x86_64")]
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
