// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! User address spaces.
//!
//! x86-64 copies PML4 entries 256..512 from the kernel root, so all kernel mappings are shared.
//! aarch64 keeps the kernel in TTBR1 and allocates a zeroed TTBR0 root: the two roots are
//! separate registers, so the user root only ever contains user entries. Dropping an address
//! space frees only its user-half page-table frames; mapped data frames belong to the
//! address-space owner and are not touched.

#![allow(dead_code)] // S08/S24 construct and use address spaces.

use raven_rmm::{MapError, PhysAddr};

use crate::memory::paging::{
    self, KernelFrameAllocator, KernelMapper, KernelPhysMemory, KernelTlb,
};

/// A user address space root.
pub struct AddressSpace {
    root: PhysAddr,
    #[allow(dead_code)] // The mappers borrow these owners for their lifetimes.
    allocator: KernelFrameAllocator,
    #[allow(dead_code)]
    memory: KernelPhysMemory,
    #[allow(dead_code)]
    tlb: KernelTlb,
}

impl AddressSpace {
    /// Create a new empty address space.
    #[cfg(target_arch = "x86_64")]
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

    /// Create a new empty address space with its own zeroed TTBR0 root.
    #[cfg(target_arch = "aarch64")]
    pub fn new() -> Result<Self, MapError> {
        let root = crate::memory::frame::alloc_zeroed_frame().ok_or(MapError::OutOfFrames)?;
        Ok(Self {
            root: PhysAddr(root.0),
            allocator: KernelFrameAllocator,
            memory: KernelPhysMemory,
            tlb: KernelTlb,
        })
    }

    /// The root frame.
    pub const fn root(&self) -> PhysAddr {
        self.root
    }

    /// A mapper over this address space with the architecture's descriptor encoding.
    pub fn mapper(&mut self) -> Result<KernelMapper<'_>, MapError> {
        #[cfg(target_arch = "x86_64")]
        {
            raven_rmm::Mapper::new(self.root, &mut self.allocator, &self.memory, &mut self.tlb)
        }
        #[cfg(target_arch = "aarch64")]
        {
            raven_rmm::Mapper::with_encoding(
                self.root,
                &mut self.allocator,
                &self.memory,
                &mut self.tlb,
            )
        }
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        paging::free_user_tables(self.root);
    }
}
