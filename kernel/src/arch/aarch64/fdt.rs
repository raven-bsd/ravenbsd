// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Platform discovery from the loader-provided device tree.
//!
//! The S12 loader forwards `BootInfo.dtb` when UEFI exposes the DTB config table. AAVMF does not
//! on QEMU `virt`, so the QEMU `virt` addresses are the documented fallback: GICD `0x0800_0000`,
//! GICC `0x0801_0000`, non-secure physical timer PPI 30. The reader itself is `raven-fdt-lite`,
//! compiled into the kernel with `#[path]` so the kernel manifest needs no new dependency.

#[path = "../../../../crates/raven-fdt-lite/src/imp.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own host tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The shared source is audited and linted in raven-fdt-lite's own host build.
mod imp;

use crate::arch::HHDM_BASE;

/// Largest device tree accepted from firmware.
const MAX_DTB_BYTES: u64 = 2 * 1024 * 1024;
/// QEMU `virt` GIC distributor (also the Pi 4 GIC-400 base is different; S15 supplies it).
const QEMU_VIRT_GICD: u64 = 0x0800_0000;
/// QEMU `virt` GIC CPU interface.
const QEMU_VIRT_GICC: u64 = 0x0801_0000;
/// QEMU `virt` non-secure physical timer PPI.
const QEMU_VIRT_TIMER_PPI: u32 = 30;
/// Device-tree PPI entries are numbered `architectural PPI - 16`.
const PPI_OFFSET: u32 = 16;

/// Platform addresses the kernel needs before drivers can probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    /// GIC distributor physical base.
    pub gicd: u64,
    /// GIC CPU-interface physical base.
    pub gicc: u64,
    /// Non-secure physical timer PPI number.
    pub timer_ppi: u32,
    /// True when the values came from the device tree rather than the QEMU fallbacks.
    pub from_dtb: bool,
}

impl Platform {
    /// The QEMU `virt` defaults.
    pub const fn qemu_virt() -> Self {
        Self {
            gicd: QEMU_VIRT_GICD,
            gicc: QEMU_VIRT_GICC,
            timer_ppi: QEMU_VIRT_TIMER_PPI,
            from_dtb: false,
        }
    }
}

/// Parse the device tree at physical `base`/`len` through the HHDM.
///
/// Any invalid pointer, truncated blob or missing node leaves the corresponding value at the QEMU
/// fallback; this function never panics and never trusts the firmware beyond the bounds checks.
/// It must be called before paging reclaims loader memory (the DTB lives in loader-owned frames).
pub fn platform(base: u64, len: u64) -> Platform {
    let mut platform = Platform::qemu_virt();
    if base == 0 || len == 0 || len > MAX_DTB_BYTES {
        return platform;
    }
    let Some(virtual_base) = HHDM_BASE.checked_add(base) else {
        return platform;
    };
    let Ok(length) = usize::try_from(len) else {
        return platform;
    };
    // SAFETY: `base..base+len` is the loader's DTB range. The loader mapped all memory it
    // described into the HHDM; the range is bounded and the parser only reads through the slice.
    let bytes = unsafe { core::slice::from_raw_parts(virtual_base as *const u8, length) };
    let Ok(fdt) = imp::Fdt::parse(bytes) else {
        return platform;
    };

    let address_cells = fdt
        .root()
        .and_then(|root| root.u32_property("#address-cells"))
        .unwrap_or(2);
    let size_cells = fdt
        .root()
        .and_then(|root| root.u32_property("#size-cells"))
        .unwrap_or(2);

    if let Some(intc) = fdt.find_compatible(&["arm,cortex-a15-gic", "arm,gic-400"])
        && let Ok(mut reg) = intc.reg_checked(address_cells, size_cells)
        && let Some((gicd, gicd_len)) = reg.next()
        && let Some((gicc, gicc_len)) = reg.next()
        && gicd_len != 0
        && gicc_len != 0
    {
        platform.gicd = gicd;
        platform.gicc = gicc;
        platform.from_dtb = true;
    }

    if let Some(timer) = fdt.find_compatible(&["arm,armv8-timer", "arm,armv7-timer"]) {
        // Each entry is <type, number, flags>; entry 1 is the non-secure physical timer.
        let mut cells = timer.cells("interrupts", 4);
        let mut index = 0usize;
        while index < 6 {
            let Some(kind) = cells.next() else { break };
            let Some(number) = cells.next() else { break };
            let Some(_flags) = cells.next() else { break };
            if index == 1 && kind == 1 && number < u64::from(PPI_OFFSET) {
                platform.timer_ppi = u32::try_from(number)
                    .unwrap_or(0)
                    .saturating_add(PPI_OFFSET);
                platform.from_dtb = true;
            }
            index = index.saturating_add(1);
        }
    }

    platform
}
