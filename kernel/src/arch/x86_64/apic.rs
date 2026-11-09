// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Local APIC (xAPIC MMIO or x2APIC MSRs).

use core::arch::x86_64::__cpuid;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_rmm::MapError;

use crate::arch::x86_64::acpi;
use crate::kprintln;
use crate::memory::mmio;

/// APIC base MSR.
const IA32_APIC_BASE: u32 = 0x1B;
/// x2APIC id MSR.
const X2_APIC_ID: u32 = 0x802;
/// x2APIC task priority MSR.
const X2_TPR: u32 = 0x808;
/// x2APIC EOI MSR.
const X2_EOI: u32 = 0x80B;
/// x2APIC spurious-vector MSR.
const X2_SIVR: u32 = 0x80F;
/// x2APIC ICR MSR.
#[allow(dead_code)] // S10 sends IPIs.
const X2_ICR: u32 = 0x830;
/// x2APIC LVT timer MSR.
const X2_LVT_TIMER: u32 = 0x832;
/// x2APIC LVT LINT0 MSR.
const X2_LVT_LINT0: u32 = 0x835;
/// x2APIC LVT LINT1 MSR.
const X2_LVT_LINT1: u32 = 0x836;
/// x2APIC LVT error MSR.
const X2_LVT_ERROR: u32 = 0x837;
/// x2APIC initial-count MSR.
const X2_INIT_COUNT: u32 = 0x838;
/// x2APIC current-count MSR.
const X2_CUR_COUNT: u32 = 0x839;
/// x2APIC divide-configuration MSR.
const X2_DIV_CONF: u32 = 0x83E;

const XAPIC_ID: u32 = 0x20;
const XAPIC_TPR: u32 = 0x80;
const XAPIC_EOI: u32 = 0xB0;
const XAPIC_SVR: u32 = 0xF0;
#[allow(dead_code)] // S10 sends IPIs.
const XAPIC_ICR_LOW: u32 = 0x300;
#[allow(dead_code)] // S10 sends IPIs.
const XAPIC_ICR_HIGH: u32 = 0x310;
const XAPIC_LVT_TIMER: u32 = 0x320;
const XAPIC_LVT_LINT0: u32 = 0x350;
const XAPIC_LVT_LINT1: u32 = 0x360;
const XAPIC_LVT_ERROR: u32 = 0x370;
const XAPIC_INIT_COUNT: u32 = 0x380;
const XAPIC_CUR_COUNT: u32 = 0x390;
const XAPIC_DIV_CONF: u32 = 0x3E0;

/// LVT mask bit.
pub const LVT_MASK: u32 = 1 << 16;
/// Periodic timer mode bits in the LVT timer entry.
pub const LVT_PERIODIC: u32 = 0b01 << 17;

static MMIO_BASE: AtomicU64 = AtomicU64::new(0);
static APIC_ID: AtomicU32 = AtomicU32::new(0);
static X2APIC: AtomicBool = AtomicBool::new(false);

/// Why APIC initialisation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApicError {
    /// The xAPIC MMIO page could not be mapped.
    Map(MapError),
}

/// True when the local APIC is in x2APIC mode.
pub fn is_x2apic() -> bool {
    X2APIC.load(Ordering::Acquire)
}

/// The local APIC id.
pub fn id() -> u32 {
    APIC_ID.load(Ordering::Acquire)
}

/// Initialise the boot processor's local APIC.
pub fn init_bsp() -> Result<(), ApicError> {
    let features = __cpuid(1);
    let x2_supported = features.ecx & (1 << 21) != 0;
    let mut base = rdmsr(IA32_APIC_BASE);
    base |= 1 << 11;
    if x2_supported {
        base |= 1 << 10;
    } else {
        base &= !(1 << 10);
    }
    wrmsr(IA32_APIC_BASE, base);
    X2APIC.store(x2_supported, Ordering::Release);

    if x2_supported {
        wrmsr(X2_TPR, 0);
        wrmsr(X2_SIVR, 0x1FF);
        wrmsr(X2_LVT_LINT0, u64::from(LVT_MASK));
        wrmsr(X2_LVT_LINT1, u64::from(LVT_MASK));
        wrmsr(X2_LVT_ERROR, u64::from(LVT_MASK));
        wrmsr(X2_LVT_TIMER, u64::from(LVT_MASK));
        APIC_ID.store(rdmsr(X2_APIC_ID) as u32, Ordering::Release);
    } else {
        let address = acpi::info().local_apic_address;
        let pointer = mmio::map_mmio(address, 0x1000).map_err(ApicError::Map)?;
        MMIO_BASE.store(pointer as u64, Ordering::Release);
        xapic_write(XAPIC_TPR, 0);
        xapic_write(XAPIC_SVR, 0x1FF);
        xapic_write(XAPIC_LVT_LINT0, LVT_MASK);
        xapic_write(XAPIC_LVT_LINT1, LVT_MASK);
        xapic_write(XAPIC_LVT_ERROR, LVT_MASK);
        xapic_write(XAPIC_LVT_TIMER, LVT_MASK);
        APIC_ID.store(xapic_read(XAPIC_ID) >> 24, Ordering::Release);
    }

    kprintln!(
        "apic: id {} ({})",
        id(),
        if is_x2apic() { "x2apic" } else { "xapic" }
    );
    Ok(())
}

/// Send an end-of-interrupt to the local APIC.
pub fn eoi() {
    if is_x2apic() {
        wrmsr(X2_EOI, 0);
    } else {
        xapic_write(XAPIC_EOI, 0);
    }
}

/// Handle a spurious interrupt.
///
/// The spurious vector is not an in-service interrupt, so it deliberately does not send an EOI.
pub fn on_spurious() {}

/// Send a fixed delivery-mode IPI.
#[allow(dead_code)] // S10 sends IPIs between CPUs.
pub fn send_ipi(destination: u32, vector: u8) {
    if is_x2apic() {
        wrmsr(X2_ICR, (u64::from(destination) << 32) | u64::from(vector));
    } else {
        let mut spins = 0u32;
        while xapic_read(XAPIC_ICR_LOW) & (1 << 12) != 0 {
            spins = spins.saturating_add(1);
            if spins >= 1_000_000 {
                break;
            }
            core::hint::spin_loop();
        }
        xapic_write(XAPIC_ICR_HIGH, (destination & 0xFF) << 24);
        xapic_write(XAPIC_ICR_LOW, u32::from(vector));
    }
}

/// Program the LAPIC timer divide configuration.
pub fn timer_set_divisor(bits: u8) {
    let bits = u32::from(bits & 0xF);
    if is_x2apic() {
        wrmsr(X2_DIV_CONF, u64::from(bits));
    } else {
        xapic_write(XAPIC_DIV_CONF, bits);
    }
}

/// Write the LVT timer entry.
pub fn timer_set_lvt(value: u32) {
    if is_x2apic() {
        wrmsr(X2_LVT_TIMER, u64::from(value));
    } else {
        xapic_write(XAPIC_LVT_TIMER, value);
    }
}

/// Write the timer initial count.
pub fn timer_set_initial(count: u32) {
    if is_x2apic() {
        wrmsr(X2_INIT_COUNT, u64::from(count));
    } else {
        xapic_write(XAPIC_INIT_COUNT, count);
    }
}

/// Read the current timer count.
pub fn timer_current() -> u32 {
    if is_x2apic() {
        rdmsr(X2_CUR_COUNT) as u32
    } else {
        xapic_read(XAPIC_CUR_COUNT)
    }
}

/// Stop and mask the LAPIC timer.
pub fn timer_stop() {
    timer_set_initial(0);
    timer_set_lvt(LVT_MASK);
}

/// Start the periodic LAPIC timer at the calibrated initial count.
pub fn timer_start_periodic(vector: u8, initial: u32) {
    timer_set_lvt(u32::from(vector) | LVT_PERIODIC);
    timer_set_initial(initial);
}

fn xapic_read(offset: u32) -> u32 {
    let base = MMIO_BASE.load(Ordering::Acquire) as usize;
    if base == 0 {
        return 0;
    }
    let pointer = base.saturating_add(offset as usize) as *const u32;
    // SAFETY: the APIC page is mapped uncached and `offset` is one of the architectural
    // registers, all within the mapped 4 KiB page.
    unsafe { core::ptr::read_volatile(pointer) }
}

fn xapic_write(offset: u32, value: u32) {
    let base = MMIO_BASE.load(Ordering::Acquire) as usize;
    if base == 0 {
        return;
    }
    let pointer = base.saturating_add(offset as usize) as *mut u32;
    // SAFETY: the APIC page is mapped uncached and `offset` is one of the architectural
    // registers, all within the mapped 4 KiB page.
    unsafe {
        core::ptr::write_volatile(pointer, value);
    }
}

fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: `rdmsr` is a ring-0 register read; the caller supplies architectural MSR numbers.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

fn wrmsr(msr: u32, value: u64) {
    // SAFETY: `wrmsr` is a ring-0 register write; the caller supplies architectural MSR numbers.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags),
        );
    }
}
