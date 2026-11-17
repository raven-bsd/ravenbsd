// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PL031 real-time clock on aarch64 QEMU `virt`.
//!
//! The PL031 data register holds the wall-clock seconds since the Unix epoch (QEMU initialises it
//! from the host clock). The frame sits at `0x0901_0000`, one page above the PL011 UART; like the
//! UART it is not described in the AAVMF memory map, so this driver maps it with device
//! attributes on first use through [`crate::memory::paging::with_kernel_mapper`] and reads the
//! counter through the HHDM. A machine without the PL031 (or a failed mapping) yields `None`; the
//! caller reports "no RTC" and leaves `CLOCK_REALTIME` at zero.

use core::sync::atomic::{AtomicBool, Ordering};

use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};

/// PL031 physical base on QEMU `virt`.
const PL031_PHYS: u64 = 0x0901_0000;
/// Data register offset.
const DR_OFFSET: u64 = 0x00;

static MAPPED: AtomicBool = AtomicBool::new(false);

/// Map the PL031 page once with device attributes.
fn ensure_mapped() -> Result<(), MapError> {
    if MAPPED.load(Ordering::Acquire) {
        return Ok(());
    }
    let virt = VirtAddr::new(crate::arch::HHDM_BASE + PL031_PHYS).ok_or(MapError::NotCanonical)?;
    let mut outcome = Ok(());
    crate::memory::paging::with_kernel_mapper(|mapper| {
        if mapper.translate(virt).is_none() {
            outcome = mapper.map(
                virt,
                PhysAddr(PL031_PHYS),
                PageFlags::kernel_device(),
                PageSize::Size4K,
            );
        }
    })?;
    outcome?;
    MAPPED.store(true, Ordering::Release);
    Ok(())
}

/// Read the wall clock as epoch nanoseconds, or `None` when the RTC cannot be mapped.
pub fn read() -> Option<u64> {
    ensure_mapped().ok()?;
    let data = (crate::arch::HHDM_BASE + PL031_PHYS + DR_OFFSET) as *const u32;
    // SAFETY: `ensure_mapped` mapped the PL031 page device-nGnRE; the data register is a 32-bit
    // read-only counter, and the volatile read bypasses caching so it observes the live value.
    let seconds = unsafe { core::ptr::read_volatile(data) };
    Some(u64::from(seconds).saturating_mul(1_000_000_000))
}
