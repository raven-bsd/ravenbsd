// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PWM audio userspace driver entry point.
//!
//! This is a skeleton: the real MMIO/DMA mapping, the `platd` GPIO/clock/channel leases, the
//! S21d `DmaBuffer` window and the audiod profile wiring are the board-integration step. The
//! host-testable seam and the typed bring-up live in the library; no hardware address is
//! touched here yet.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!(
            "pwm-audio: skeleton only; MMIO/DMA/platd board wiring is the integration step"
        );
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
