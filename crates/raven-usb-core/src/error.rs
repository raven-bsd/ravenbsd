// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The crate's typed error.

use core::fmt;

use raven_rp1_xhci::RingError;

use crate::address::AddressError;
use crate::control::ControlError;
use crate::descriptor::DescriptorError;

/// Why USB enumeration or a control transfer failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// A descriptor is malformed or inconsistent.
    Descriptor(DescriptorError),
    /// A control transfer failed.
    Control(ControlError),
    /// Address assignment failed.
    Address(AddressError),
    /// The xHCI transfer/event ring refused the operation.
    Ring(RingError),
    /// The operation is not valid in the current state.
    State {
        /// State the operation requires.
        expected: &'static str,
        /// Current state.
        actual: &'static str,
    },
    /// The device declares no configuration.
    NoConfiguration,
    /// A requested configuration value does not exist.
    ConfigurationNotFound(u8),
    /// The endpoint is not present in the active configuration.
    EndpointNotFound(u8),
}

impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(error) => write!(f, "descriptor: {error}"),
            Self::Control(error) => write!(f, "control: {error}"),
            Self::Address(error) => write!(f, "address: {error}"),
            Self::Ring(error) => write!(f, "ring: {error}"),
            Self::State { expected, actual } => {
                write!(f, "state is {actual}, expected {expected}")
            }
            Self::NoConfiguration => f.write_str("device has no configuration"),
            Self::ConfigurationNotFound(value) => {
                write!(f, "configuration {value} is not present")
            }
            Self::EndpointNotFound(dci) => write!(f, "endpoint DCI {dci} is not present"),
        }
    }
}

impl core::error::Error for UsbError {}

impl From<DescriptorError> for UsbError {
    fn from(error: DescriptorError) -> Self {
        Self::Descriptor(error)
    }
}

impl From<ControlError> for UsbError {
    fn from(error: ControlError) -> Self {
        Self::Control(error)
    }
}

impl From<AddressError> for UsbError {
    fn from(error: AddressError) -> Self {
        Self::Address(error)
    }
}

impl From<RingError> for UsbError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}
