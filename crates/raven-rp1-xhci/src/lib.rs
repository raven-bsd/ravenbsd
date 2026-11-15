// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable RP1 xHCI host-controller model.
//!
//! The Pi 5 USB functions are two Synopsys DesignWare USB3 (`snps,dwc3`) cores behind the RP1
//! PCIe endpoint; the DesignWare core exposes a standard xHCI host controller to software. This
//! crate carries the parts a host test can prove without the board:
//!
//! * [`device`] reads the two RP1 USB nodes from the firmware device tree (windows, interrupts,
//!   `dr_mode`, LPM and the Synopsys quirks);
//! * [`regs`] is the standard xHCI 1.1 register map and the capability/PORTSC/USBCMD/USBSTS
//!   decoding;
//! * [`trb`] encodes and decodes Transfer Request Blocks;
//! * [`ring`] models the command, event and transfer rings with cycle state and wrap rules;
//! * [`port`] is the port/roothub state machine and change bits;
//! * [`slot`] models slots and endpoints (enable, address, configure, halt, stop, reset);
//! * [`interrupt`] models the interrupter's pending/enable bits, ERDP and EHB.
//!
//! **No hardware claim.** The register offsets and TRB fields are the public xHCI 1.1 standard;
//! the DesignWare wrapper offsets, the RP1 integration and every real-device result are
//! UNVERIFIED until a Pi 5 board log exists. `docs/pi5-xhci.md` lists the board steps.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod device;
pub mod interrupt;
pub mod port;
pub mod regs;
pub mod ring;
pub mod slot;
pub mod trb;

pub use device::{
    DrMode, InterruptRef, MAX_USB_CONTROLLERS, Quirks, UsbController, UsbControllers, UsbError,
};
pub use interrupt::Interrupter;
pub use port::{
    MAX_PORTS, PortAction, PortChanges, PortError, PortEvent, PortMachine, PortState, Roothub,
};
pub use regs::{
    Capability, Hcsparams2, PORT_BASE, PORT_STRIDE, PORTSC_CCS, PORTSC_CSC, PORTSC_PED, PORTSC_PLC,
    PORTSC_PLS_MASK, PORTSC_PLS_SHIFT, PORTSC_PP, PortStatus, USBCMD_RUN, USBSTS_HCH,
    decode_hcsparams2,
};
pub use ring::{CommandRing, EventRing, MAX_RING_ENTRIES, RingError, TransferRing};
pub use slot::{EndpointState, MAX_ENDPOINTS, MAX_SLOTS, SlotState, SlotTable, SlotTableError};
pub use trb::{TRB_CHAIN, TRB_CYCLE, TRB_IOC, TRB_TYPE_MASK, TRB_TYPE_SHIFT, Trb, TrbType};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
