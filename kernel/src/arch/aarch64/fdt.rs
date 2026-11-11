// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device-tree access and platform discovery.
//!
//! The UEFI loader forwards `BootInfo.dtb` when firmware exposes the DTB config table. The Pi 4
//! path always has one. The reader (`raven-fdt-lite`) and the BootInfo/platform builder
//! (`raven-bootinfo-fdt`) are compiled into the kernel with `#[path]` so the kernel manifest needs
//! no new dependency.

#[path = "../../../../crates/raven-fdt-lite/src/imp.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own host tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The shared source is audited and linted in raven-fdt-lite's own host build.
mod imp;
pub use imp::{Fdt, Node};

#[path = "../../../../crates/raven-bootinfo-fdt/src/imp.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own host tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod bootinfo;
#[cfg(raven_pi4)]
pub use bootinfo::{CMDLINE_MAX, MAX_REGIONS, Params, build_into};
pub use bootinfo::{Platform, platform as platform_from_fdt};

use raven_rmm::MapError;

use crate::arch::HHDM_BASE;
use crate::memory::mmio::map_mmio;

/// Largest device tree accepted from firmware.
const MAX_DTB_BYTES: u64 = 4 * 1024 * 1024;

/// One `reg` window mapped through the kernel's device-MMIO window.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // Consumed by S21d drivers; the Pi platform record logs it today.
pub struct MappedReg {
    /// CPU physical address of the window start.
    pub physical: u64,
    /// Window length in bytes.
    pub length: u64,
    /// Device-nGnRE mapping in the kernel MMIO window.
    pub virtual_address: *mut u8,
}

impl MappedReg {
    /// A zeroed record, used as an array initialiser.
    #[allow(dead_code)] // Used by the Pi platform record; the UEFI build has no user yet.
    pub const EMPTY: Self = Self {
        physical: 0,
        length: 0,
        virtual_address: core::ptr::null_mut(),
    };
}

/// Map every `reg` window of `node` through the device-MMIO window.
///
/// The addresses are translated through the parent `ranges`; [`map_mmio`] selects the
/// device-nGnRE attribute and refuses usable RAM. Returns the number of windows written.
#[allow(dead_code)] // Consumed by S21d drivers; the Pi platform record logs it today.
pub fn map_node_regs(node: &Node<'_>, out: &mut [MappedReg]) -> Result<usize, MapError> {
    let mut count = 0usize;
    for (physical, length) in node.reg_translated() {
        if length == 0 {
            continue;
        }
        let len = usize::try_from(length).map_err(|_| MapError::NotCanonical)?;
        let virtual_address = map_mmio(physical, len)?;
        let slot = out.get_mut(count).ok_or(MapError::OutOfFrames)?;
        *slot = MappedReg {
            physical,
            length,
            virtual_address,
        };
        count = count.saturating_add(1);
    }
    Ok(count)
}

/// Parse a device tree at physical `base`/`len` through the HHDM.
///
/// The returned view borrows firmware memory for the lifetime of the kernel; it must be called
/// while the DTB range is mapped. Invalid pointers and truncated blobs return `None`.
pub fn dtb(base: u64, len: u64) -> Option<Fdt<'static>> {
    if base == 0 || len == 0 || len > MAX_DTB_BYTES {
        return None;
    }
    let virtual_base = HHDM_BASE.checked_add(base)?;
    let length = usize::try_from(len).ok()?;
    // SAFETY: `base..base+len` is the firmware DTB range; the boot path mapped it through the
    // HHDM as normal memory. The parser only reads through the slice.
    let bytes = unsafe { core::slice::from_raw_parts(virtual_base as *const u8, length) };
    Fdt::parse(bytes).ok()
}

/// Parse the platform devices from the DTB at `base`/`len`.
///
/// A missing or invalid DTB leaves the QEMU `virt` defaults in place.
pub fn platform(base: u64, len: u64) -> Platform {
    match dtb(base, len) {
        Some(fdt) => platform_from_fdt(&fdt),
        None => Platform::qemu_virt(),
    }
}
