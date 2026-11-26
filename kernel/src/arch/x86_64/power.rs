// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 ACPI power and reset.
//!
//! The FADT gives the PM1a/PM1b control ports and (when the feature flag is set) a reset
//! register. The S5 sleep type is firmware data: it comes from the `\_S5` package parsed out of
//! the DSDT/SSDTs, so a platform that declares a non-zero SLP_TYP is powered off correctly.

use crate::arch::x86_64::{acpi, pic};
use crate::kprintln;

/// ACPI SLP_EN bit.
const SLP_EN: u16 = 1 << 13;
/// Position of the SLP_TYP field in the PM1 control register.
const SLP_TYP_SHIFT: u16 = 10;

/// Enter ACPI S5 or halt if the firmware exposes no usable PM1 control port or `\_S5`.
pub fn poweroff() -> ! {
    let info = acpi::info();
    if info.pm1a_control != 0 {
        let Some(sleep_a) = info.s5_sleep_type_a else {
            kprintln!("power: firmware does not define an ACPI _S5 sleep type");
            crate::arch::halt();
        };
        outw(
            info.pm1a_control,
            (u16::from(sleep_a) << SLP_TYP_SHIFT) | SLP_EN,
        );
        if info.pm1b_control != 0 {
            let sleep_b = info.s5_sleep_type_b.unwrap_or(sleep_a);
            outw(
                info.pm1b_control,
                (u16::from(sleep_b) << SLP_TYP_SHIFT) | SLP_EN,
            );
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
