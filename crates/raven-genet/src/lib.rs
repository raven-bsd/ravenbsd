// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET v5 Ethernet core for the Raspberry Pi 4.
//!
//! This crate is the host-testable half of the O1 driver: it owns the register map, the
//! RX/TX descriptor and ring model, the MDIO transaction state machine, the BCM54213PE PHY
//! rules (identifier, link resolution and RGMII delays) and the Pi 4 device-tree/MAC path.
//! It never touches MMIO or DMA; those live behind the `genet` userspace driver's seam.
//!
//! The hardware facts come from the NetBSD `bcmgenet` driver and the Pi 4 firmware device
//! tree, read as behavioural references only. Every real-board result remains UNVERIFIED
//! until a board log exists; see `docs/pi4-genet.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dtb;
pub mod mac;
pub mod mdio;
pub mod phy;
pub mod regs;
pub mod ring;
