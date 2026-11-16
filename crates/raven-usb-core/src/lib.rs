// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable USB core: descriptors, address assignment, control transfers, hub port state and
//! enumeration over the R4 xHCI model.
//!
//! The crate is the layer between an xHCI host controller and a class driver (S60 HID). It owns:
//!
//! * [`descriptor`] - standard device/configuration/interface/endpoint descriptors, with
//!   class-specific records preserved as raw bytes;
//! * [`address`] - the deterministic USB address pool (`1..=127`, address 0 is the default);
//! * [`control`] - setup packets, standard requests and the [`ControlTransport`] seam;
//! * [`enumerate`] - `Attached -> Described -> Addressed -> Configured` enumeration;
//! * [`hub`] - attach/power/reset/enable/suspend/over-current port state;
//! * [`seam`] - the Setup/Data/Status TRB sequence over `raven_rp1_xhci`'s transfer/event rings.
//!
//! **No hardware claim.** The descriptor/request shapes are the public USB 2.0 definitions; the
//! xHCI/DWC3 register path is R4's model and every real-device result is UNVERIFIED until a board
//! log exists. HID parsing/streaming is S60's work and is deliberately not implemented here.
//!
//! The crate is `no_std` outside tests, `#![forbid(unsafe_code)]`, and uses `alloc` for descriptor
//! sets and enumeration state.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod address;
pub mod control;
pub mod descriptor;
pub mod enumerate;
pub mod error;
pub mod hub;
pub mod seam;

pub use address::{AddressError, AddressPool, DEFAULT_ADDRESS, MAX_ADDRESS};
pub use control::{
    ControlError, ControlTransport, DescriptorType, Direction, Recipient, RequestKind, SetupPacket,
    StandardRequest, clear_feature, get_configuration, get_descriptor, get_interface, get_status,
    set_address, set_configuration, set_feature, set_interface,
};
pub use descriptor::{
    CONFIG_LEN, Configuration, ConfigurationHeader, DEVICE_LEN, DescriptorError, Device,
    DeviceDescriptor, ENDPOINT_LEN, EndpointAddress, EndpointDescriptor, INTERFACE_LEN, Interface,
    InterfaceDescriptor, TransferType,
};
pub use enumerate::{EnumerationState, Enumerator, device, standard_request};
pub use error::UsbError;
pub use hub::{
    Hub, HubError, HubEvent, HubPort, HubPortState, MAX_HUB_PORTS, PortChange, PortSpeed,
};
pub use seam::{
    COMPLETION_STALL, COMPLETION_SUCCESS, CommandCompletion, Completion, XhciEvent, XhciSeam,
};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
