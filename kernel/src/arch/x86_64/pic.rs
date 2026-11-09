// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 8259 PIC and a PIT channel-2 reference delay.
//!
//! The PIC is remapped out of the exception range and then fully masked; the APIC is the only
//! interrupt controller that may deliver after this call. PIT channel 2 is used only as a
//! one-shot ~10 ms reference when CPUID does not report a TSC frequency.

/// PIC master command port.
const PIC1_CMD: u16 = 0x20;
/// PIC master data port.
const PIC1_DATA: u16 = 0x21;
/// PIC slave command port.
const PIC2_CMD: u16 = 0xA0;
/// PIC slave data port.
const PIC2_DATA: u16 = 0xA1;
/// POST diagnostic port used for the traditional I/O delay.
const IO_WAIT_PORT: u16 = 0x80;
/// PIT channel 2 data port.
const PIT_CH2: u16 = 0x42;
/// PIT command port.
const PIT_CMD: u16 = 0x43;
/// NMI status and control port; bit 0 gates channel 2, bit 5 is its output.
const PORT_61: u16 = 0x61;
/// PIT input clock (1.193182 MHz).
const PIT_HZ: u64 = 1_193_182;
/// Divisor for a nominal 10 ms one-shot.
pub const PIT_10MS_DIVISOR: u16 = 11_932;

/// Write one byte to an I/O port.
#[inline]
pub fn outb(port: u16, value: u8) {
    // SAFETY: `out` is a ring-0 port write with no memory side effects.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read one byte from an I/O port.
#[inline]
pub fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` is a ring-0 port read with no memory side effects.
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

fn io_wait() {
    outb(IO_WAIT_PORT, 0);
}

/// Remap the 8259 PIC to vectors 0x20-0x2F and mask every line.
pub fn remap_and_mask() {
    outb(PIC1_CMD, 0x11);
    io_wait();
    outb(PIC2_CMD, 0x11);
    io_wait();
    outb(PIC1_DATA, 0x20);
    io_wait();
    outb(PIC2_DATA, 0x28);
    io_wait();
    outb(PIC1_DATA, 0x04);
    io_wait();
    outb(PIC2_DATA, 0x02);
    io_wait();
    outb(PIC1_DATA, 0x01);
    io_wait();
    outb(PIC2_DATA, 0x01);
    io_wait();
    outb(PIC1_DATA, 0xFF);
    outb(PIC2_DATA, 0xFF);
}

/// Wait for one PIT channel-2 one-shot period, returning false on timeout.
///
/// The period is `PIT_10MS_DIVISOR / PIT_HZ` seconds (~10 ms). The caller uses the measured TSC
/// delta, so the small divisor rounding does not accumulate into the clock.
pub fn pit_wait_10ms() -> bool {
    let original = inb(PORT_61);
    outb(PIT_CMD, 0xB0);
    outb(PIT_CH2, PIT_10MS_DIVISOR as u8);
    outb(PIT_CH2, (PIT_10MS_DIVISOR >> 8) as u8);
    outb(PORT_61, (original | 0x01) & !0x02);

    let mut spins = 0u32;
    while inb(PORT_61) & 0x20 == 0 {
        spins = spins.saturating_add(1);
        if spins >= 20_000_000 {
            outb(PORT_61, original);
            return false;
        }
        core::hint::spin_loop();
    }
    outb(PORT_61, original);
    true
}

/// Nominal duration of [`pit_wait_10ms`] in nanoseconds.
#[allow(clippy::arithmetic_side_effects)] // Both operands are compile-time constants.
pub const fn pit_window_ns() -> u64 {
    ((PIT_10MS_DIVISOR as u64) * 1_000_000_000) / PIT_HZ
}

const _: () = {
    assert!(PIT_10MS_DIVISOR > 0);
    assert!(pit_window_ns() > 9_900_000);
    assert!(pit_window_ns() < 10_100_000);
};
