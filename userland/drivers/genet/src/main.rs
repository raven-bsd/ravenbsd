// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET userspace driver entry point.
//!
//! This is a skeleton: the real MMIO/DMA mapping, `devd` resource handles, IRQ delivery and the
//! B39 frame pump are the board-integration step. The host-testable seam and typed bring-up live
//! in the library; no hardware address is touched here yet.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!(
            "genet: skeleton only; MMIO/DMA/IRQ board wiring is the integration step"
        );
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
