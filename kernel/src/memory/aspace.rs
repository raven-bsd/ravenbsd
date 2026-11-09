// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! User address spaces for S08/S24.
//!
//! An address space copies PML4 entries 256..512 from the kernel root, so all kernel mappings are
//! shared. Dropping it frees only the user-half page-table frames; mapped data frames belong to
//! the address-space owner and are not touched.

#![allow(dead_code)] // S08/S24 construct and use address spaces.

use raven_rmm::{MapError, Mapper, PhysAddr};

use crate::memory::paging::{self, KernelFrameAllocator, KernelPhysMemory, KernelTlb};

/// A user address space root.
pub struct AddressSpace {
    root: PhysAddr,
    allocator: KernelFrameAllocator,
    memory: KernelPhysMemory,
    tlb: KernelTlb,
}

impl AddressSpace {
    /// Create a new address space with the kernel half copied from the active kernel root.
    pub fn new() -> Result<Self, MapError> {
        let source = paging::kernel_root().ok_or(MapError::OutOfFrames)?;
        let root = paging::with_kernel_mapper(|mapper| mapper.new_address_space(source))??;
        Ok(Self {
            root,
            allocator: KernelFrameAllocator,
            memory: KernelPhysMemory,
            tlb: KernelTlb,
        })
    }

    /// The root frame.
    pub const fn root(&self) -> PhysAddr {
        self.root
    }

    /// A mapper over this address space.
    pub fn mapper(
        &mut self,
    ) -> Result<Mapper<'_, KernelFrameAllocator, KernelPhysMemory, KernelTlb>, MapError> {
        Mapper::new(self.root, &mut self.allocator, &self.memory, &mut self.tlb)
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        paging::free_user_tables(self.root);
    }
}
