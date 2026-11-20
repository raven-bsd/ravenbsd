// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 ACPI power and reset.
//!
//! The FADT gives the PM1a/PM1b control ports and (when the feature flag is set) a reset
//! register. QEMU q35 exposes both; the code falls back to the conventional 0xcf9 reset port
//! only when the firmware does not advertise a reset register.

use crate::arch::x86_64::{acpi, pic};

/// ACPI S5 sleep type in the PM1 control register.
const SLP_TYP_S5: u16 = 5 << 10;
/// ACPI SLP_EN bit.
const SLP_EN: u16 = 1 << 13;

/// Enter ACPI S5 or halt if the firmware exposes no PM1 control port.
pub fn poweroff() -> ! {
    let info = acpi::info();
    if info.pm1a_control != 0 {
        let value = SLP_TYP_S5 | SLP_EN;
        outw(info.pm1a_control, value);
        if info.pm1b_control != 0 {
            outw(info.pm1b_control, value);
        }
    }
    // A firmware without a usable PM1 block cannot be powered off from the kernel. Halting is
    // honest: the qtest observes a timeout instead of a fake success.
    crate::arch::halt()
}

/// Reset through the FADT reset register, or the conventional 0xcf9 hard-reset fallback.
pub fn reset() -> ! {
    let info = acpi::info();
    if info.reset_supported && info.reset_register != 0 {
        pic::outb(info.reset_register, info.reset_value);
    } else {
        // NetBSD x86_reset fallback: select a hard reset, then assert bit 2.
        pic::outb(0xcf9, 0x02);
        pic::outb(0xcf9, 0x06);
    }
    crate::arch::halt()
}

fn outw(port: u16, value: u16) {
    // SAFETY: PM1 control is an I/O port supplied by the firmware; the write has no memory effect.
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags),
        );
    }
}
