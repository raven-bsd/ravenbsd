// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PSCI CPU_ON/CPU_OFF and system control.
//!
//! QEMU `virt` with AAVMF and the Pi 4 firmware expose PSCI through SMC; a hypervisor may expose
//! it through HVC. Both conduits use the same 64-bit function ids. The secondary entry address is
//! a physical address because PSCI starts the target CPU with the MMU off.

use core::arch::asm;

/// The 64-bit SMC conduit function id for `CPU_ON`.
pub const CPU_ON: u64 = 0xC400_0003;
/// `CPU_OFF` (32-bit function id; no arguments).
#[allow(dead_code)]
pub const CPU_OFF: u64 = 0x8400_0002;
/// `SYSTEM_OFF`.
#[allow(dead_code)]
pub const SYSTEM_OFF: u64 = 0x8400_0008;
/// `SYSTEM_RESET`.
#[allow(dead_code)]
pub const SYSTEM_RESET: u64 = 0x8400_0009;

/// PSCI `INVALID_PARAMETERS`: the target CPU does not exist or the entry is invalid.
#[allow(dead_code)]
pub const INVALID_PARAMETERS: i64 = -2;

/// Result of a PSCI call.
pub type PsciResult = Result<(), i64>;

fn smc(function: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let result: i64;
    // SAFETY: SMC #0 traps to the secure monitor; the firmware validates the function id and
    // arguments. Only EL1 scalar arguments are passed.
    unsafe {
        asm!(
            "smc #0",
            inout("x0") function => result,
            in("x1") arg1,
            in("x2") arg2,
            in("x3") arg3,
            options(nostack),
        );
    }
    result
}

fn hvc(function: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let result: i64;
    // SAFETY: HVC traps to the hypervisor; the firmware validates the function id and arguments.
    unsafe {
        asm!(
            "hvc #0",
            inout("x0") function => result,
            in("x1") arg1,
            in("x2") arg2,
            in("x3") arg3,
            options(nostack),
        );
    }
    result
}

/// Call a PSCI function through the conduit selected by the device tree.
fn call(function: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    if crate::arch::aarch64::fdt::psci_hvc() {
        hvc(function, arg1, arg2, arg3)
    } else {
        smc(function, arg1, arg2, arg3)
    }
}

/// Start `cpu` at the physical `entry` address with `context` in `x0`.
pub fn cpu_on(cpu: u64, entry: u64, context: u64) -> PsciResult {
    match call(CPU_ON, cpu, entry, context) {
        0 => Ok(()),
        error => Err(error),
    }
}

/// Stop the calling CPU. Does not return when the firmware accepts it.
#[allow(dead_code)] // Kept for the next SMP slice.
pub fn cpu_off() -> ! {
    let _ = call(CPU_OFF, 0, 0, 0);
    loop {
        // SAFETY: WFI is valid at EL1; the CPU has no work left.
        unsafe { asm!("wfi", options(nomem, nostack, preserves_flags)) };
    }
}

/// Power the machine off. Does not return when the firmware accepts it.
#[allow(dead_code)] // Kept for the next SMP slice.
pub fn system_off() -> ! {
    let _ = call(SYSTEM_OFF, 0, 0, 0);
    crate::arch::halt()
}

/// Reset the machine. Does not return when the firmware accepts it.
#[allow(dead_code)] // Kept for the next SMP slice.
pub fn system_reset() -> ! {
    let _ = call(SYSTEM_RESET, 0, 0, 0);
    crate::arch::halt()
}
