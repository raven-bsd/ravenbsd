// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guest xHCI controller for the QEMU `qemu-xhci` + `usb-kbd` path.
//!
//! The crate owns the standard xHCI 1.1 operational sequence: capability decode, reset/run,
//! DCBAAP/CRCR/ERST, port reset, Enable Slot/Address Device/Configure Endpoint, control transfers
//! and interrupt-IN transfers. [`Seam`] is the MMIO/DMA boundary; the userland driver implements
//! it with `raven-driver`, while the host tests use a deterministic memory double.
//!
//! **No hardware claim.** The QEMU xHCI register path is standard; the Pi/RP1 DesignWare offset
//! remains UNVERIFIED (R4). The S61 USB core and S62 HID console are wired by the userspace
//! driver, not by this crate.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod context;
pub mod controller;
pub mod error;
pub mod ring;

pub use context::{
    MAX_INPUT_CONTEXT, context_size, encode_address_input, encode_configure_input,
    input_context_size,
};
pub use controller::{
    COMMAND_RING_OFFSET, CONTROL_BUFFER_OFFSET, Controller, DCBAAP_OFFSET, DEVICE_CONTEXT_OFFSET,
    EP0_DCI, EP0_RING_OFFSET, EP1_DCI, EP1_RING_OFFSET, ERST_OFFSET, EVENT_RING_OFFSET,
    INPUT_CONTEXT_OFFSET, INTERRUPT_BUFFER_OFFSET, MIN_DMA_LEN, QEMU_XHCI_DEVICE, QEMU_XHCI_VENDOR,
    RING_TRBS, Seam,
};
pub use error::XhciError;
pub use raven_rp1_xhci::{ControlDirection, Trb, TrbType};
pub use ring::{EventRing, ProducerRing, read_trb, write_trb};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
