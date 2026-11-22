// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K517 PROC_REBOOT denied/invalid fixture (runs as /init).
//!
//! Rows: unknown flag bits return EINVAL (root caller); an unprivileged caller
//! (euid 1000 after `setresuid`) gets EPERM and the machine keeps running. The
//! program never acts successfully, so the guest exits cleanly via PID 1.

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
    use raven_rt::proc;
    use raven_rt::sys::syscall1;

    entry!(main);

    fn name(error: Errno) -> &'static str {
        match error {
            Errno::EINVAL => "EINVAL",
            Errno::EPERM => "EPERM",
            Errno::ENOSYS => "ENOSYS",
            _ => "OTHER",
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        // Unknown flag bits from a root caller: EINVAL, nothing acted on.
        let raw = unsafe { syscall1(PROC_REBOOT, 0xdead) };
        match Errno::from_ret(raw) {
            Ok(_) => {
                raven_rt::println!("invalid=RETURNED");
                return 1;
            }
            Err(error) => raven_rt::println!("invalid={}", name(error)),
        }

        // Drop to the regular user id: the same poweroff request must be EPERM.
        match proc::setresuid(1000) {
            Ok(()) => {}
            Err(error) => {
                raven_rt::println!("setuid={}", name(error));
                return 2;
            }
        }
        let raw = unsafe { syscall1(PROC_REBOOT, RB_POWER_OFF as usize) };
        match Errno::from_ret(raw) {
            Ok(_) => raven_rt::println!("denied=RETURNED"),
            Err(error) => raven_rt::println!("denied={}", name(error)),
        }
        raven_rt::println!("denied-done");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
