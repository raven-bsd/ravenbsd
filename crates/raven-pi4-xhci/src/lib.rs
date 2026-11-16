// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Pi 4 xHCI host-controller model.
//!
//! The Pi 4 USB 3 host function is an xHCI controller behind the BCM2711 PCIe bridge (the VL805
//! function in the firmware description). This crate carries the parts a host test can prove
//! without a board:
//!
//! * [`regs`] is the standard xHCI 1.1 capability/operational/runtime/doorbell register map and
//!   the `PORTSC`/`USBCMD`/`USBSTS` decoding;
//! * [`trb`] encodes and decodes Transfer Request Blocks;
//! * [`ring`] models the command, event and transfer rings with cycle state and wrap rules;
//! * [`port`] is the port/roothub state machine and change bits;
//! * [`seam`] is the DMA/IRQ/event seam for S61 `raven-usb-core`.
//!
//! **No hardware claim.** The register offsets and TRB fields are the public xHCI 1.1 standard;
//! the VL805/BCM2711 BAR, DMA, IRQ routing and every real-device result are UNVERIFIED until a
//! Pi 4 board log exists. `docs/pi4-usb.md` lists the board steps and the S61 integration seam.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod port;
pub mod regs;
pub mod ring;
pub mod seam;
pub mod trb;

pub use port::{
    MAX_PORTS, PortAction, PortChanges, PortError, PortEvent, PortMachine, PortState, Roothub,
};
pub use regs::{
    Capability, PORTSC_CCS, PORTSC_CSC, PORTSC_PED, PORTSC_PLC, PORTSC_PLS_MASK, PORTSC_PLS_SHIFT,
    PORTSC_PP, PortStatus, USBCMD_RESET, USBCMD_RUN, USBSTS_EINT, USBSTS_HCH, XHCI_DEVICE_VL805,
    XHCI_VENDOR_VIA,
};
pub use ring::{CommandRing, EventRing, MAX_RING_ENTRIES, RingError, TransferRing};
pub use seam::{Completion, DmaBuffer, DmaDirection, Host, SeamError, Transfer, XhciSeam};
pub use trb::{TRB_CHAIN, TRB_CYCLE, TRB_IOC, TRB_TYPE_MASK, TRB_TYPE_SHIFT, Trb, TrbType};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
