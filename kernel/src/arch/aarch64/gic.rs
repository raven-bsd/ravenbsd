// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ARM GICv2 distributor and CPU interface (QEMU `virt` and the Pi 4 GIC-400).
//!
//! Both frames are mapped through the kernel's uncached MMIO window (`memory::mmio`), so the
//! driver never touches the HHDM aliases of device memory. The programming sequence follows the
//! GICv2 architecture and the Raspberry Pi OS tutorial's GIC-400 path: disable the distributor,
//! clear the banked SGI/PPI state, accept all priorities, then enable group 1 and the CPU
//! interface. Interrupts are masked by the CPU's DAIF until the handler path is ready.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::memory::mmio::map_mmio;

// Distributor registers.
const GICD_CTLR: u64 = 0x000;
const GICD_ISENABLER: u64 = 0x100;
const GICD_ICENABLER: u64 = 0x180;
const GICD_ICPENDR: u64 = 0x280;
const GICD_ICACTIVER: u64 = 0x380;
const GICD_IPRIORITYR: u64 = 0x400;
const GICD_ITARGETSR: u64 = 0x800;
// CPU interface registers.
const GICC_CTLR: u64 = 0x000;
const GICC_PMR: u64 = 0x004;
const GICC_BPR: u64 = 0x008;
const GICC_IAR: u64 = 0x00c;
const GICC_EOIR: u64 = 0x010;
// Spurious/no-pending interrupt ids are 1020..=1023.
const ID_SPURIOUS: u32 = 1020;
/// First shared peripheral interrupt (SPIs); below this are banked SGIs/PPIs.
const FIRST_SPI: u32 = 32;
/// Priority used for every enabled interrupt (0xa0 is the middle of the 8-bit field).
const PRIORITY: u8 = 0xa0;

static GICD: AtomicU64 = AtomicU64::new(0);
static GICC: AtomicU64 = AtomicU64::new(0);

/// Why GIC initialisation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GicError {
    /// The distributor frame could not be mapped.
    MapDistributor,
    /// The CPU-interface frame could not be mapped.
    MapCpuInterface,
    /// The driver was used before [`init`].
    NotInitialised,
}

impl fmt::Display for GicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GicError::MapDistributor => f.write_str("cannot map the GIC distributor"),
            GicError::MapCpuInterface => f.write_str("cannot map the GIC CPU interface"),
            GicError::NotInitialised => f.write_str("GIC is not initialised"),
        }
    }
}

/// Map and initialise the GICv2 distributor and CPU interface.
pub fn init(distributor_phys: u64, cpu_interface_phys: u64) -> Result<(), GicError> {
    let distributor =
        map_mmio(distributor_phys, 0x1000).map_err(|_| GicError::MapDistributor)? as u64;
    let cpu_interface =
        map_mmio(cpu_interface_phys, 0x1000).map_err(|_| GicError::MapCpuInterface)? as u64;

    // Disable distribution while the banked SGI/PPI state is reset. Firmware can leave a PPI
    // pending or active; clearing it makes the first timer delivery deterministic.
    write32(distributor, GICD_CTLR, 0);
    write32(distributor, GICD_ICENABLER, 0xffff_ffff);
    write32(distributor, GICD_ICPENDR, 0xffff_ffff);
    write32(distributor, GICD_ICACTIVER, 0xffff_ffff);
    let priority_word = u32::from(PRIORITY).saturating_mul(0x0101_0101);
    let mut irq = 0u32;
    while irq < FIRST_SPI {
        write32(
            distributor,
            GICD_IPRIORITYR.saturating_add(u64::from(irq)),
            priority_word,
        );
        irq = irq.saturating_add(4);
    }

    // Accept every priority, no preemption grouping, then enable the interface.
    write32(cpu_interface, GICC_CTLR, 0);
    write32(cpu_interface, GICC_PMR, 0xff);
    write32(cpu_interface, GICC_BPR, 0);
    // SAFETY: the frames are mapped device memory; the DSB makes the configuration visible to the
    // GIC before distribution is enabled.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
    write32(distributor, GICD_CTLR, 1);
    write32(cpu_interface, GICC_CTLR, 1);

    GICD.store(distributor, Ordering::Release);
    GICC.store(cpu_interface, Ordering::Release);
    Ok(())
}

/// Enable one interrupt line (SPI or banked PPI).
pub fn enable(irq: u32) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    write32(
        distributor,
        GICD_ISENABLER.saturating_add(u64::from(irq >> 5).saturating_mul(4)),
        1u32 << (irq & 31),
    );
    Ok(())
}

/// Disable one interrupt line.
pub fn disable(irq: u32) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    write32(
        distributor,
        GICD_ICENABLER.saturating_add(u64::from(irq >> 5).saturating_mul(4)),
        1u32 << (irq & 31),
    );
    Ok(())
}

/// Set the priority of one interrupt line (banked registers for PPIs/SGIs).
#[allow(dead_code)] // Consumed by S21d when drivers register their own lines.
pub fn set_priority(irq: u32, priority: u8) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    let shift = (irq & 3) << 3;
    let value = u32::from(priority) << shift;
    write32(
        distributor,
        GICD_IPRIORITYR.saturating_add(u64::from(irq & !3)),
        value,
    );
    Ok(())
}

/// Route a shared peripheral interrupt to the boot CPU.
///
/// ITARGETSR is banked and read-only for SGIs/PPIs (0..31); setting a target there is ignored by
/// the architecture, so this is only meaningful for SPIs.
#[allow(dead_code)] // Consumed by S21d when drivers register their own lines.
pub fn set_target(irq: u32, cpu: u8) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if !(FIRST_SPI..1020).contains(&irq) {
        return Err(GicError::NotInitialised);
    }
    let shift = (irq & 3) << 3;
    let value = u32::from(cpu) << shift;
    write32(
        distributor,
        GICD_ITARGETSR.saturating_add(u64::from(irq & !3)),
        value,
    );
    Ok(())
}

/// Read the highest-priority pending interrupt (raw IAR value).
pub fn acknowledge() -> u32 {
    let base = GICC.load(Ordering::Acquire);
    if base == 0 {
        return ID_SPURIOUS;
    }
    read32(base, GICC_IAR)
}

/// Signal end-of-interrupt for a raw IAR value.
pub fn eoi(raw: u32) {
    let base = GICC.load(Ordering::Acquire);
    if base != 0 {
        write32(base, GICC_EOIR, raw);
    }
}

/// The interrupt id inside a raw IAR value.
pub const fn interrupt_id(raw: u32) -> u32 {
    raw & 0x3ff
}

/// True for the spurious/no-pending ids.
pub const fn is_spurious(id: u32) -> bool {
    id >= ID_SPURIOUS
}

fn base(register: &AtomicU64) -> Result<u64, GicError> {
    match register.load(Ordering::Acquire) {
        0 => Err(GicError::NotInitialised),
        value => Ok(value),
    }
}

fn write32(base: u64, offset: u64, value: u32) {
    // SAFETY: `base` is the mapped GIC frame and every caller passes a register offset inside its
    // 4 KiB page.
    unsafe {
        (base.saturating_add(offset) as *mut u32).write_volatile(value);
    }
}

fn read32(base: u64, offset: u64) -> u32 {
    // SAFETY: `base` is the mapped GIC frame and every caller passes a register offset inside its
    // 4 KiB page.
    unsafe { (base.saturating_add(offset) as *const u32).read_volatile() }
}
