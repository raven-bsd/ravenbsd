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
pub use imp::{DmaWindow, Fdt, Node};

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

/// The firmware DTB bytes through the HHDM, if the boot handoff carried one.
///
/// The Pi path re-reads the DTB after paging init (the RAM copy is reserved), so this is the
/// same range [`dtb`] accepts. A missing or oversized DTB returns `None`.
pub fn dtb_slice() -> Option<&'static [u8]> {
    // SAFETY: paging published the copied BootInfo during init and it is immutable afterwards.
    let info = unsafe { &*crate::memory::paging::boot_info_ptr() };
    if info.dtb.base == 0 || info.dtb.len == 0 || info.dtb.len > MAX_DTB_BYTES {
        return None;
    }
    let virtual_base = HHDM_BASE.checked_add(info.dtb.base)?;
    let length = usize::try_from(info.dtb.len).ok()?;
    // SAFETY: the firmware DTB range is mapped through the HHDM and only read through the slice.
    Some(unsafe { core::slice::from_raw_parts(virtual_base as *const u8, length) })
}

/// The nearest DMA window of the Pi 4 `/soc` bus.
///
/// QEMU `virt` has no `dma-ranges`, and AAVMF exposes no DTB, so both return `None` and the
/// identity domain stays in force.
pub fn platform_dma_window() -> Option<DmaWindow> {
    // SAFETY: paging published the copied BootInfo during init and it is immutable afterwards.
    let boot_info = unsafe { &*crate::memory::paging::boot_info_ptr() };
    let fdt = dtb(boot_info.dtb.base, boot_info.dtb.len)?;
    for path in [
        "/soc/mailbox@7e00b880",
        "/soc/rng@7e104000",
        "/emmc2bus/mmc@7e340000",
        "/scb/ethernet@7d580000",
    ] {
        if let Some(window) = fdt.dma_window_for(path)
            && window.size != 0
        {
            return Some(window);
        }
    }
    None
}

/// The trigger/polarity of one GIC SPI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpiRoute {
    /// True for a level-sensitive line, false for an edge-triggered one.
    pub level: bool,
    /// True when the device drives the line active-low.
    pub active_low: bool,
}

/// Find the route of the GIC interrupt `spi` in the live device tree, or the `virt` fallback.
///
/// `spi` is the GIC interrupt id (32..1019). The device tree numbers SPIs from zero, so the
/// lookup matches `spi - 32` against the binding's second cell. The fallback covers the UEFI boot
/// path, where AAVMF exposes no DTB: UART0/RTC/GPIO use level lines and the virtio-mmio slots are
/// edge-triggered. An unknown line defaults to level, which is the GIC reset configuration.
pub fn spi_route(spi: u32) -> SpiRoute {
    // SAFETY: paging published the copied BootInfo during init and it is immutable afterwards.
    let boot_info = unsafe { &*crate::memory::paging::boot_info_ptr() };
    if let Some(fdt) = dtb(boot_info.dtb.base, boot_info.dtb.len)
        && let Some(root) = fdt.root()
        && let Some(route) = scan_spi(&root, spi)
    {
        return route;
    }
    fallback_spi_route(spi)
}

fn fallback_spi_route(spi: u32) -> SpiRoute {
    let spi_number = spi.saturating_sub(32);
    let level = !(16..48).contains(&spi_number);
    SpiRoute {
        level,
        active_low: false,
    }
}

fn scan_spi(node: &Node<'_>, spi: u32) -> Option<SpiRoute> {
    if let Some(value) = node.property("interrupts") {
        // The GIC binding uses three cells per interrupt: type (0 = SPI, 1 = PPI), the zero-based
        // SPI number, and flags. The caller passes the GIC id, which is SPI + 32.
        let wanted = spi.saturating_sub(32);
        for chunk in value.chunks_exact(12) {
            let kind = be_u32(chunk, 0)?;
            let number = be_u32(chunk, 4)?;
            let flags = be_u32(chunk, 8)?;
            if kind == 0 && number == wanted {
                return Some(SpiRoute {
                    level: flags & 1 == 0,
                    active_low: flags & 2 != 0,
                });
            }
        }
    }
    for child in node.children() {
        if let Some(route) = scan_spi(&child, spi) {
            return Some(route);
        }
    }
    None
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = bytes.get(offset..end)?;
    let mut word = [0u8; 4];
    word.copy_from_slice(slice);
    Some(u32::from_be_bytes(word))
}
