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
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::prot;
use raven_rmm::{MapError, Mapper, PageFlags, PageSize, PhysAddr, VirtAddr};
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

/// A user address space: page tables, VMAs and memory-object ownership.
pub struct UserAddressSpace {
    aspace: AddressSpace,
    vmas: Vmas,
    objects: Vec<ObjectMapping>,
}

impl UserAddressSpace {
    /// Wrap a fresh page-table root with an empty VMA tree.
    pub fn new(aspace: AddressSpace) -> Self {
        Self {
            aspace,
            vmas: Vmas::new(USER_MIN..USER_END),
            objects: Vec::new(),
        }
    }

    /// Address-space root for the scheduler.
    pub fn root(&self) -> PhysAddr {
        self.aspace.root()
    }

    /// A mapper over this address space's root.
    pub fn mapper(
        &mut self,
    ) -> Result<
        Mapper<
            '_,
            crate::memory::paging::KernelFrameAllocator,
            crate::memory::paging::KernelPhysMemory,
            crate::memory::paging::KernelTlb,
        >,
        MapError,
    > {
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
                let Some(physical) = object.frame_at((offset / PAGE_SIZE).saturating_add(page))
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
        self.objects.push(ObjectMapping {
            start: vaddr,
            len,
            offset,
            object,
            shared,
        });
        Ok(())
    }

    /// Remove a fully mapped range, splitting VMAs and releasing object references.
    pub fn unmap(&mut self, start: u64, len: u64) -> Result<(), Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(Errno::EINVAL);
        }
        if !self.vmas.is_fully_mapped(start, len) {
            return Err(Errno::EINVAL);
        }
        let pages = pages_in(len)?;
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
                    return Err(Errno::EINVAL);
                };
                let vma = self.vmas.find(virt.data()).ok_or(Errno::EINVAL)?;
                let owns = page_owned(vma, &self.objects, virt.data(), physical.0);
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
        // Do not recycle frames until every CPU has dropped its translation: a stale TLB entry
        // could otherwise reach a frame that was already handed to another address space.
        let flushed = crate::sched::smp::shootdown::flush_range(start, len);
        if flushed {
            for physical in to_free {
                let _ = frame::free_frame(frame::PhysAddr(physical));
            }
        }
        Ok(())
    }

    /// Change protections over a fully mapped range.
    pub fn protect(&mut self, start: u64, len: u64, new_prot: u8) -> Result<(), Errno> {
        if len == 0 || !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
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
        Ok(())
    }

    /// Handle a user page fault at `cr2`. A successful copy-on-write returns `Ok(())` and the
    /// faulting instruction is retried.
    pub fn handle_fault(&mut self, cr2: u64) -> Result<(), Errno> {
        if !(USER_MIN..USER_END).contains(&cr2) {
            return Err(Errno::EFAULT);
        }
        let vma = self.vmas.find(cr2).ok_or(Errno::EFAULT)?;
        if vma.prot() & (prot::WRITE as u8) == 0 {
            return Err(Errno::EFAULT);
        }
        let page = cr2 & !(PAGE_SIZE - 1);
        let virt = VirtAddr::new(page).ok_or(Errno::EFAULT)?;
        let mut mapper = self.aspace.mapper().map_err(map_errno)?;
        let Some((physical, flags, _)) = mapper.translate(virt) else {
            return Err(Errno::EFAULT);
        };
        if flags.is_writable() {
            return Err(Errno::EFAULT);
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
            if result.is_ok() {
                let _ = crate::sched::smp::shootdown::flush_range(page, PAGE_SIZE);
            }
            return result;
        }
        // Copy-on-write: the faulting page still points into the initfs or the memory object.
        let Some(new_frame) = frame::alloc_zeroed_frame() else {
            return Err(Errno::ENOMEM);
        };
        let source = frame::phys_to_virt(frame::PhysAddr(physical.0));
        let destination = frame::phys_to_virt(new_frame);
        if source.is_null() || destination.is_null() {
            let _ = frame::free_frame(new_frame);
            return Err(Errno::EFAULT);
        }
        // SAFETY: both pointers refer to whole 4 KiB frames; the source belongs to the initfs or
        // memory object and the destination is a fresh frame.
        unsafe {
            core::ptr::copy_nonoverlapping(source, destination, PAGE_SIZE as usize);
        }
        if mapper.unmap(virt).is_err() {
            let _ = frame::free_frame(new_frame);
            return Err(Errno::EFAULT);
        }
        let flags = leaf_flags(vma.prot(), true);
        if mapper
            .map(virt, PhysAddr(new_frame.0), flags, PageSize::Size4K)
            .is_err()
        {
            let _ = frame::free_frame(new_frame);
            return Err(Errno::EFAULT);
        }
        let _ = crate::sched::smp::shootdown::flush_range(page, PAGE_SIZE);
        Ok(())
    }

    /// Free every data frame owned by this address space and clear the VMA tree.
    pub fn free_user_frames(&mut self) {
        let vmas: Vec<Vma> = self.vmas.iter().copied().collect();
        let objects = self.objects.clone();
        if let Ok(mut mapper) = self.aspace.mapper() {
            for vma in vmas {
                let pages = pages_in(vma.len()).unwrap_or(0);
                let mut page = 0u64;
                while page < pages {
                    let address = vma.start().saturating_add(page.saturating_mul(PAGE_SIZE));
                    let Some(virt) = VirtAddr::new(address) else {
                        break;
                    };
                    if let Some((physical, _, _)) = mapper.translate(virt)
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
    }

    /// Release an address space that is currently active: free its data frames, switch back to the
    /// kernel root and then free its page tables while running on the kernel stack.
    pub fn release(mut self) {
        self.free_user_frames();
        match paging::kernel_root() {
            // SAFETY: `active_root` is the kernel page-table root; the current kernel stack and
            // code are mapped in the kernel half of every address space.
            Some(root) => unsafe { activate_root(root.0) },
            None => crate::arch::halt(),
        }
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
