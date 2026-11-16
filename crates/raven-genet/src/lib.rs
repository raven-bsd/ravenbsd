// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET v5 Ethernet core for the Raspberry Pi 4.
//!
//! This crate is the host-testable half of the O1-O3 driver: it owns the register map, the
//! RX/TX descriptor and ring model, the frame ownership/refill path, the interrupt and bounded
//! recovery rules, UMAC filter programming, the DMA bounce rules, the MDIO transaction state
//! machine, the BCM54213PE PHY rules (identifier, debounced link state and RGMII delays) and the
//! Pi 4 device-tree/MAC path. It never touches MMIO or DMA; those live behind the `genet`
//! userspace driver's seam.
//!
//! The hardware facts come from the NetBSD `bcmgenet` driver and the Pi 4 firmware device
//! tree, read as behavioural references only. Every real-board result remains UNVERIFIED
//! until a board log exists; see `docs/pi4-genet.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dma;
pub mod dtb;
pub mod filter;
pub mod frame;
pub mod irq;
pub mod link;
pub mod mac;
pub mod mdio;
pub mod phy;
pub mod recovery;
pub mod regs;
pub mod release;
pub mod ring;

/// The shared B39 frame-ring contract used by the `nic:` service.
pub use raven_proto_net::ring as b39;
