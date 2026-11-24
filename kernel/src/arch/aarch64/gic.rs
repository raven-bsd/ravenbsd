// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ARM GICv2 distributor and CPU interface (QEMU `virt` and the Pi 4 GIC-400).
//!
//! Both frames are mapped through the kernel's uncached MMIO window (`memory::mmio`), so the
//! driver never touches the HHDM aliases of device memory. The programming sequence follows the
//! GICv2 architecture and the Raspberry Pi OS tutorial's GIC-400 path: disable the distributor,
//! clear the banked SGI/PPI state, accept all priorities, then enable both interrupt groups and
//! the CPU interface. Interrupts are masked by the CPU's DAIF until the handler path is ready.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::memory::mmio::map_mmio;

// Distributor registers.
const GICD_CTLR: u64 = 0x000;
const GICD_IGROUPR: u64 = 0x080;
const GICD_ISENABLER: u64 = 0x100;
const GICD_ICENABLER: u64 = 0x180;
const GICD_ICPENDR: u64 = 0x280;
const GICD_ICACTIVER: u64 = 0x380;
const GICD_IPRIORITYR: u64 = 0x400;
const GICD_ITARGETSR: u64 = 0x800;
const GICD_ICFGR: u64 = 0xc00;
const GICD_SGIR: u64 = 0xf00;
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
    /// A target CPU index is outside the GICv2 eight-CPU target list.
    BadTarget,
}

impl fmt::Display for GicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GicError::MapDistributor => f.write_str("cannot map the GIC distributor"),
            GicError::MapCpuInterface => f.write_str("cannot map the GIC CPU interface"),
            GicError::NotInitialised => f.write_str("GIC is not initialised"),
            GicError::BadTarget => f.write_str("interrupt target CPU is outside the GICv2 list"),
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
    // Enable every group the CPU interface can signal. `GICC_CTLR` bit 0 is EnableGrp0, bit 1 is
    // EnableGrp1NS and bit 2 is EnableGrp1S: a GIC with security extensions splits Group 1 into
    // Secure and Non-secure, and firmware may leave the timer PPI in Group 0 (QEMU `virt`) while
    // the SGIs below are Group 1. Enabling only bits 0..1 leaves a Group 1 Secure SGI pending but
    // unsignalled: `GICC_IAR` then reads 1022 (spurious) forever while the CPU keeps taking the
    // IRQ, which is the AP timer/IPI storm S76 round 3 fixed. A GIC without security extensions
    // implements only bit 0 and ignores the rest.
    write32(distributor, GICD_CTLR, 0b11);
    write32(cpu_interface, GICC_CTLR, 0b111);

    GICD.store(distributor, Ordering::Release);
    GICC.store(cpu_interface, Ordering::Release);
    Ok(())
}

/// Configure this CPU's banked CPU interface and SGI lines.
///
/// The distributor frames are global, but `GICC_*` and the SGI/PPI enable, priority and group
/// registers are banked per CPU. Application processors call this before entering the scheduler.
pub fn init_cpu() -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    let cpu_interface = base(&GICC)?;
    write32(cpu_interface, GICC_PMR, 0xff);
    write32(cpu_interface, GICC_BPR, 0);
    // Same enable set as `init`; see the Group 1 Secure note there. The SGI/IPI lines below are
    // Group 1, so this CPU must signal Group 1 Secure as well or the reschedule IPI stays pending.
    write32(cpu_interface, GICC_CTLR, 0b111);
    // SGIs 0..2: priority, Group 1 and enable for this CPU. The writes are banked.
    let priority_word = u32::from(PRIORITY).saturating_mul(0x0101_0101);
    write32(distributor, GICD_IPRIORITYR, priority_word);
    write32(distributor, GICD_IGROUPR, 0x7);
    write32(distributor, GICD_ISENABLER, 0x7);
    barrier();
    Ok(())
}

/// Send software interrupt `irq` (0..15) to `cpu`'s CPU interface.
///
/// GICv2's target list has one bit per CPU interface; the QEMU `virt` and Pi 4 topologies use
/// that numbering. CPUs above the 8-bit list are not addressable and return an error.
pub fn send_sgi(cpu: usize, irq: u32) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 16 || cpu >= 8 {
        return Err(GicError::NotInitialised);
    }
    // NSATT (bit 15) selects a non-secure SGI for the Group-1 SGI we configure; without it a
    // secure-only Group-0 SGI is not delivered to EL1.
    let value = (1u32 << 15) | ((1u32 << cpu) << 16) | (irq & 0xf);
    write32(distributor, GICD_SGIR, value);
    barrier();
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
    barrier();
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
    barrier();
    Ok(())
}

/// Make the distributor writes visible before the caller signals EOI or returns.
fn barrier() {
    // SAFETY: DSB is a valid EL1 barrier and touches no memory.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// Set the priority of one interrupt line (banked registers for PPIs/SGIs).
pub fn set_priority(irq: u32, priority: u8) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    let offset = GICD_IPRIORITYR.saturating_add(u64::from(irq & !3));
    let shift = (irq & 3) << 3;
    let mut value = read32(distributor, offset);
    value &= !(0xff << shift);
    value |= u32::from(priority) << shift;
    write32(distributor, offset, value);
    Ok(())
}

/// Route a shared peripheral interrupt to `cpu`, a zero-based CPU index.
///
/// ITARGETSR is banked and read-only for SGIs/PPIs (0..31); setting a target there is ignored by
/// the architecture, so this is only meaningful for SPIs. The register stores one bit per CPU
/// interface, not a CPU number: CPU 0 is mask `0b0000_0001`, so the index is shifted here and
/// callers keep passing the same index used by [`send_sgi`]. GICv2 addresses at most eight CPUs.
pub fn set_target(irq: u32, cpu: u8) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if !is_spi(irq) {
        return Err(GicError::NotInitialised);
    }
    let mask = target_mask(cpu).ok_or(GicError::BadTarget)?;
    let offset = GICD_ITARGETSR.saturating_add(u64::from(irq & !3));
    let shift = (irq & 3) << 3;
    let mut value = read32(distributor, offset);
    value &= !(0xff << shift);
    value |= u32::from(mask) << shift;
    write32(distributor, offset, value);
    Ok(())
}

/// The GICv2 ITARGETSR byte for a zero-based CPU index, or `None` above CPU 7.
const fn target_mask(cpu: u8) -> Option<u8> {
    if cpu < 8 { Some(1u8 << cpu) } else { None }
}

/// Put one interrupt in Group 1, the non-secure group the kernel enables in `init`.
///
/// On a GIC without security extensions the register is RAZ/WI, so the write is harmless; on the
/// Pi 4 GIC-400 it is required for a non-secure EL1 kernel to receive the SPI. Kept for the Pi
/// platform wiring; the QEMU `virt` gate uses the default Group 0.
#[allow(dead_code)] // Consumed by the Pi 4 platform follow-up.
pub fn set_group1(irq: u32) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    write32(
        distributor,
        GICD_IGROUPR.saturating_add(u64::from(irq >> 5).saturating_mul(4)),
        1u32 << (irq & 31),
    );
    Ok(())
}

/// Configure one SPI as level-sensitive or edge-triggered.
///
/// GICv2 has two configuration bits per interrupt; for SPIs bit 0 is reserved and bit 1 selects
/// edge (`1`) or level (`0`). GIC-400 SPI polarity is fixed active-high.
pub fn configure_spi(irq: u32, level: bool) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if !is_spi(irq) {
        return Err(GicError::NotInitialised);
    }
    let offset = GICD_ICFGR.saturating_add(u64::from(irq >> 4).saturating_mul(4));
    let shift = (irq & 15) << 1;
    let mut value = read32(distributor, offset);
    value &= !(0b11 << shift);
    if !level {
        value |= 0b10 << shift;
    }
    write32(distributor, offset, value);
    // SAFETY: the DSB orders the configuration write before the line is enabled.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
    Ok(())
}

/// Clear the pending state of one interrupt line.
pub fn clear_pending(irq: u32) -> Result<(), GicError> {
    let distributor = base(&GICD)?;
    if irq >= 1020 {
        return Err(GicError::NotInitialised);
    }
    write32(
        distributor,
        GICD_ICPENDR.saturating_add(u64::from(irq >> 5).saturating_mul(4)),
        1u32 << (irq & 31),
    );
    Ok(())
}

/// True for a shared peripheral interrupt id (32..1019).
pub const fn is_spi(irq: u32) -> bool {
    irq >= FIRST_SPI && irq < 1020
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
