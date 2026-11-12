// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD Raspberry Pi 4 platform server (`plat:` scheme).
//!
//! `platd` owns the VideoCore mailbox, the GPIO/pinmux arbiter and the RNG200. It is the only
//! server that touches those platform registers, so EMMC2, GENET and audio clients ask it instead
//! of poking the same blocks. The QEMU/host path uses the deterministic fake backend; the
//! hardware path accepts mapped MMIO/DMA capabilities from the supervisor.
//!
//! The crate is `#![forbid(unsafe_code)]`: register access goes through `raven-driver`'s bounded
//! `MmioRegion`/`DmaRegion`, and the RNG/mailbox codecs are pure `raven-plat` code.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod handler;
#[cfg(target_os = "none")]
mod hardware;
#[cfg(target_os = "none")]
mod program;

#[cfg(not(target_os = "none"))]
fn main() {}
