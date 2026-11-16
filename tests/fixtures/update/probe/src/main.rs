// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot root service for the update-health qtest.
//!
//! It runs from the UFS root at `/root`, proves that the root filesystem is mounted and the
//! service path works, then exits cleanly. `readiness = "exit"` makes `init` treat the clean exit
//! as service-specific readiness before it writes the health marker.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::println!("update-health: probe ready");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
