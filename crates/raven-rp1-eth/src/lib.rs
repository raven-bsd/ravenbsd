// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable RP1 Ethernet model.
//!
//! The Pi 5 Ethernet function is a Cadence GEM-compatible MAC (`raspberrypi,rp1-gem`,
//! `cdns,macb`) behind the RP1 PCIe endpoint, with an RGMII PHY on MDIO and DMA descriptor rings.
//! This crate carries the parts a host test can prove without the board:
//!
//! * [`device`] reads the Ethernet node from the firmware device tree: window, interrupt, PHY
//!   address/mode, reset GPIO, clock references and the `cdns` properties;
//! * [`mac`] is the MAC address and filter model (exact entries, hash bits, promiscuous and
//!   all-multicast modes);
//! * [`mdio`] is the IEEE 802.3 clause 22 frame codec, the PHY transaction state machine and the
//!   standard BMSR/BMCR status decode;
//! * [`ring`] is the TX/RX descriptor ring with ownership, wrap and capacity rules;
//! * [`link`] is the link/autonegotiation state machine and the interrupt cause/mask model;
//! * [`regs`] is the named register map as caller-supplied offsets.
//!
//! **No hardware claim.** The RP1 GEM register offsets and descriptor bit encoding are not public
//! in this workspace, so the register map and the descriptor encoding are data supplied by the
//! board bring-up, never guessed here. The device-tree values are OBSERVED; every register write
//! and every real link result is UNVERIFIED until a Pi 5 board log exists.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod device;
pub mod link;
pub mod mac;
pub mod mdio;
pub mod regs;
pub mod ring;

pub use device::{
    ClockRef, EthError, EthernetNode, InterruptRef, MAX_ETH_CLOCKS, PhyMode, PhyRef, ResetGpio,
};
pub use link::{
    Duplex, InterruptState, Interrupts, LinkAction, LinkError, LinkEvent, LinkMachine, LinkState,
    Speed,
};
pub use mac::{FilterMode, MAX_EXACT, MacAddress, MacError, MacFilter, crc32};
pub use mdio::{
    Action, BMCR, BMSR, Clause22, Mdio, MdioError, MdioState, PhyStatus, Transaction, decode_bmsr,
    decode_read, read_command, write_command,
};
pub use regs::{REGISTER_COUNT, Register, RegisterError, RegisterMap, WINDOW_BYTES};
pub use ring::{DescFlags, Descriptor, MAX_ENTRIES, Owner, Ring, RingError};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
