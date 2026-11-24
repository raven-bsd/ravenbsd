// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pi 4 xHCI userspace driver.
//!
//! The `test` argument runs the host-backed fake keyboard path used by the `pi4-usb` qtest: the
//! D1 `raven-pi4-xhci` ring seam plus S61 enumeration and S62 boot-keyboard service. The default
//! mode maps BAR0, the configuration page, DMA and the legacy IRQ and drives the standard xHCI
//! controller through `raven-xhci`; the Pi 4 VL805 board path remains UNVERIFIED.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod fake;
#[cfg(target_os = "none")]
mod kbd;
#[cfg(target_os = "none")]
mod real;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        if args.get(1) == Some("test") {
            return match super::fake::run() {
                Ok(()) => 0,
                Err(message) => {
                    raven_rt::eprintln!("pi4-xhci: {message}");
                    1
                }
            };
        }
        match super::real::run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("pi4-xhci: {message}");
                1
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
