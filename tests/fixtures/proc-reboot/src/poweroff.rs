// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K517 PROC_REBOOT poweroff fixture (runs as /init).
//!
//! Prints `poweroff=request` and calls `proc_reboot(RB_POWER_OFF)` as root. The
//! call must not return: the kernel enters ACPI S5 and QEMU exits, which the
//! qtest observes. A returned call prints `poweroff=RETURNED` and exits 1.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::errno::Errno;
    use raven_abi::syscall::{PROC_REBOOT, RB_POWER_OFF};
    use raven_rt::entry;
    use raven_rt::sys::syscall1;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::println!("poweroff=request");
        let raw = unsafe { syscall1(PROC_REBOOT, RB_POWER_OFF as usize) };
        match Errno::from_ret(raw) {
            Ok(_) => raven_rt::println!("poweroff=RETURNED"),
            Err(error) => raven_rt::println!("poweroff={}", error.description()),
        }
        raven_rt::println!("poweroff=no-action");
        1
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
