// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB setup packets, standard requests and the control-transfer seam.
//!
//! The setup packet and request codes are the public USB 2.0 definitions. The enumerator talks to
//! a device only through [`ControlTransport`], so the host tests use a recorded/synthetic device
//! and the xHCI seam can submit the same request as a Setup/Data/Status TRB sequence.

use core::fmt;

use crate::error::UsbError;

/// Standard request type bits.
pub const REQUEST_STANDARD: u8 = 0x00;
/// Class request type bits.
pub const REQUEST_CLASS: u8 = 0x20;
/// Vendor request type bits.
pub const REQUEST_VENDOR: u8 = 0x40;
/// Device-to-host direction bit.
pub const REQUEST_DIRECTION_IN: u8 = 0x80;

/// Device recipient.
pub const RECIPIENT_DEVICE: u8 = 0;
/// Interface recipient.
pub const RECIPIENT_INTERFACE: u8 = 1;
/// Endpoint recipient.
pub const RECIPIENT_ENDPOINT: u8 = 2;
/// Other recipient.
pub const RECIPIENT_OTHER: u8 = 3;

/// Standard request codes.
pub mod request {
    /// `GET_STATUS`.
    pub const GET_STATUS: u8 = 0;
    /// `CLEAR_FEATURE`.
    pub const CLEAR_FEATURE: u8 = 1;
    /// `SET_FEATURE`.
    pub const SET_FEATURE: u8 = 3;
    /// `SET_ADDRESS`.
    pub const SET_ADDRESS: u8 = 5;
    /// `GET_DESCRIPTOR`.
    pub const GET_DESCRIPTOR: u8 = 6;
    /// `SET_DESCRIPTOR`.
    pub const SET_DESCRIPTOR: u8 = 7;
    /// `GET_CONFIGURATION`.
    pub const GET_CONFIGURATION: u8 = 8;
    /// `SET_CONFIGURATION`.
    pub const SET_CONFIGURATION: u8 = 9;
    /// `GET_INTERFACE`.
    pub const GET_INTERFACE: u8 = 10;
    /// `SET_INTERFACE`.
    pub const SET_INTERFACE: u8 = 11;
    /// `SYNCH_FRAME`.
    pub const SYNCH_FRAME: u8 = 12;
}

/// The request direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Host-to-device.
    Out,
    /// Device-to-host.
    In,
}

/// The request type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    /// Standard request.
    Standard,
    /// Class request.
    Class,
    /// Vendor request.
    Vendor,
}

/// The request recipient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipient {
    /// Device.
    Device,
    /// Interface.
    Interface,
    /// Endpoint.
    Endpoint,
    /// Other.
    Other,
}

/// Why a setup packet or control transfer is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// The setup packet is shorter than eight bytes.
    ShortSetup(usize),
    /// The request-type reserved bits or recipient are invalid.
    BadRequestType(u8),
    /// A data-stage length does not match the buffer.
    LengthMismatch {
        /// Length in the setup packet.
        expected: usize,
        /// Buffer length.
        got: usize,
    },
    /// The device returned fewer bytes than the request length.
    ShortTransfer {
        /// Requested bytes.
        expected: usize,
        /// Returned bytes.
        got: usize,
    },
    /// The endpoint returned a STALL handshake.
    Stall,
    /// The device returned a protocol-invalid response.
    Protocol,
    /// The transport failed below the USB protocol layer.
    Transport,
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortSetup(got) => write!(f, "setup packet has {got} bytes, expected 8"),
            Self::BadRequestType(value) => write!(f, "invalid request type {value:#04x}"),
            Self::LengthMismatch { expected, got } => {
                write!(f, "control length {expected}, buffer {got}")
            }
            Self::ShortTransfer { expected, got } => {
                write!(f, "control returned {got} of {expected} bytes")
            }
            Self::Stall => f.write_str("control endpoint stalled"),
            Self::Protocol => f.write_str("control response is not valid USB"),
            Self::Transport => f.write_str("control transport failed"),
        }
    }
}

impl core::error::Error for ControlError {}

/// The 8-byte USB setup packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupPacket {
    /// `bmRequestType`.
    pub request_type: u8,
    /// `bRequest`.
    pub request: u8,
    /// `wValue`.
    pub value: u16,
    /// `wIndex`.
    pub index: u16,
    /// `wLength`.
    pub length: u16,
}

impl SetupPacket {
    /// Parse the 8 setup bytes and reject reserved request-type/recipient encodings.
    ///
    /// # Errors
    ///
    /// Returns [`ControlError::ShortSetup`] for a short slice and
    /// [`ControlError::BadRequestType`] for a reserved direction/type/recipient.
    pub fn parse(bytes: &[u8]) -> Result<Self, ControlError> {
        let bytes = bytes
            .get(..8)
            .ok_or(ControlError::ShortSetup(bytes.len()))?;
        let request_type = bytes[0];
        if request_type & 0x60 == 0x60 || request_type & 0x1f > RECIPIENT_OTHER {
            return Err(ControlError::BadRequestType(request_type));
        }
        Ok(Self {
            request_type,
            request: bytes[1],
            value: u16::from_le_bytes([bytes[2], bytes[3]]),
            index: u16::from_le_bytes([bytes[4], bytes[5]]),
            length: u16::from_le_bytes([bytes[6], bytes[7]]),
        })
    }

    /// Encode the setup packet.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0] = self.request_type;
        out[1] = self.request;
        out[2..4].copy_from_slice(&self.value.to_le_bytes());
        out[4..6].copy_from_slice(&self.index.to_le_bytes());
        out[6..8].copy_from_slice(&self.length.to_le_bytes());
        out
    }

    /// The direction bit.
    #[must_use]
    pub const fn direction(self) -> Direction {
        if self.request_type & REQUEST_DIRECTION_IN != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }

    /// The request type.
    #[must_use]
    pub const fn kind(self) -> RequestKind {
        match self.request_type & 0x60 {
            REQUEST_CLASS => RequestKind::Class,
            REQUEST_VENDOR => RequestKind::Vendor,
            _ => RequestKind::Standard,
        }
    }

    /// The request recipient.
    #[must_use]
    pub const fn recipient(self) -> Recipient {
        match self.request_type & 0x1f {
            RECIPIENT_INTERFACE => Recipient::Interface,
            RECIPIENT_ENDPOINT => Recipient::Endpoint,
            RECIPIENT_OTHER => Recipient::Other,
            _ => Recipient::Device,
        }
    }

    /// True when this is a standard request.
    #[must_use]
    pub const fn is_standard(self) -> bool {
        matches!(self.kind(), RequestKind::Standard)
    }
}

/// Standard request codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardRequest {
    /// `GET_STATUS`.
    GetStatus,
    /// `CLEAR_FEATURE`.
    ClearFeature,
    /// `SET_FEATURE`.
    SetFeature,
    /// `SET_ADDRESS`.
    SetAddress,
    /// `GET_DESCRIPTOR`.
    GetDescriptor,
    /// `SET_DESCRIPTOR`.
    SetDescriptor,
    /// `GET_CONFIGURATION`.
    GetConfiguration,
    /// `SET_CONFIGURATION`.
    SetConfiguration,
    /// `GET_INTERFACE`.
    GetInterface,
    /// `SET_INTERFACE`.
    SetInterface,
    /// `SYNCH_FRAME`.
    SynchFrame,
}

impl StandardRequest {
    /// Decode a standard request code.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            request::GET_STATUS => Some(Self::GetStatus),
            request::CLEAR_FEATURE => Some(Self::ClearFeature),
            request::SET_FEATURE => Some(Self::SetFeature),
            request::SET_ADDRESS => Some(Self::SetAddress),
            request::GET_DESCRIPTOR => Some(Self::GetDescriptor),
            request::SET_DESCRIPTOR => Some(Self::SetDescriptor),
            request::GET_CONFIGURATION => Some(Self::GetConfiguration),
            request::SET_CONFIGURATION => Some(Self::SetConfiguration),
            request::GET_INTERFACE => Some(Self::GetInterface),
            request::SET_INTERFACE => Some(Self::SetInterface),
            request::SYNCH_FRAME => Some(Self::SynchFrame),
            _ => None,
        }
    }

    /// Decode from a standard setup packet.
    #[must_use]
    pub fn from_setup(setup: SetupPacket) -> Option<Self> {
        if !setup.is_standard() {
            return None;
        }
        Self::from_code(setup.request)
    }
}

/// A standard descriptor type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorType {
    /// Device.
    Device,
    /// Configuration.
    Configuration,
    /// String.
    String,
    /// Interface.
    Interface,
    /// Endpoint.
    Endpoint,
    /// Device qualifier.
    DeviceQualifier,
    /// Other-speed configuration.
    OtherSpeedConfiguration,
    /// Interface power.
    InterfacePower,
    /// Other type code.
    Other(u8),
}

impl DescriptorType {
    /// Decode the high byte of a `GET_DESCRIPTOR` `wValue`.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Device,
            2 => Self::Configuration,
            3 => Self::String,
            4 => Self::Interface,
            5 => Self::Endpoint,
            6 => Self::DeviceQualifier,
            7 => Self::OtherSpeedConfiguration,
            8 => Self::InterfacePower,
            other => Self::Other(other),
        }
    }

    /// The descriptor type code.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Device => 1,
            Self::Configuration => 2,
            Self::String => 3,
            Self::Interface => 4,
            Self::Endpoint => 5,
            Self::DeviceQualifier => 6,
            Self::OtherSpeedConfiguration => 7,
            Self::InterfacePower => 8,
            Self::Other(code) => code,
        }
    }
}

/// A `GET_DESCRIPTOR` setup packet.
#[must_use]
pub fn get_descriptor(kind: DescriptorType, index: u8, language: u16, length: u16) -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_DIRECTION_IN | REQUEST_STANDARD | RECIPIENT_DEVICE,
        request: request::GET_DESCRIPTOR,
        value: u16::from(kind.code()) << 8 | u16::from(index),
        index: language,
        length,
    }
}

/// A `SET_ADDRESS` setup packet; `None` for address 0 or above 127.
#[must_use]
pub fn set_address(address: u8) -> Option<SetupPacket> {
    if address == 0 || address > 127 {
        return None;
    }
    Some(SetupPacket {
        request_type: REQUEST_STANDARD | RECIPIENT_DEVICE,
        request: request::SET_ADDRESS,
        value: u16::from(address),
        index: 0,
        length: 0,
    })
}

/// A `GET_CONFIGURATION` setup packet.
#[must_use]
pub fn get_configuration() -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_DIRECTION_IN | REQUEST_STANDARD | RECIPIENT_DEVICE,
        request: request::GET_CONFIGURATION,
        value: 0,
        index: 0,
        length: 1,
    }
}

/// A `SET_CONFIGURATION` setup packet; `None` for value 0 (unconfigured).
#[must_use]
pub fn set_configuration(value: u8) -> Option<SetupPacket> {
    if value == 0 {
        return None;
    }
    Some(SetupPacket {
        request_type: REQUEST_STANDARD | RECIPIENT_DEVICE,
        request: request::SET_CONFIGURATION,
        value: u16::from(value),
        index: 0,
        length: 0,
    })
}

/// A `GET_STATUS` setup packet.
#[must_use]
pub fn get_status(recipient: Recipient, index: u16) -> SetupPacket {
    let request_type = REQUEST_DIRECTION_IN
        | REQUEST_STANDARD
        | match recipient {
            Recipient::Device => RECIPIENT_DEVICE,
            Recipient::Interface => RECIPIENT_INTERFACE,
            Recipient::Endpoint => RECIPIENT_ENDPOINT,
            Recipient::Other => RECIPIENT_OTHER,
        };
    SetupPacket {
        request_type,
        request: request::GET_STATUS,
        value: 0,
        index,
        length: 2,
    }
}

/// A `SET_FEATURE` setup packet.
#[must_use]
pub fn set_feature(feature: u16, recipient: Recipient, index: u16) -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_STANDARD
            | match recipient {
                Recipient::Device => RECIPIENT_DEVICE,
                Recipient::Interface => RECIPIENT_INTERFACE,
                Recipient::Endpoint => RECIPIENT_ENDPOINT,
                Recipient::Other => RECIPIENT_OTHER,
            },
        request: request::SET_FEATURE,
        value: feature,
        index,
        length: 0,
    }
}

/// A `CLEAR_FEATURE` setup packet.
#[must_use]
pub fn clear_feature(feature: u16, recipient: Recipient, index: u16) -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_STANDARD
            | match recipient {
                Recipient::Device => RECIPIENT_DEVICE,
                Recipient::Interface => RECIPIENT_INTERFACE,
                Recipient::Endpoint => RECIPIENT_ENDPOINT,
                Recipient::Other => RECIPIENT_OTHER,
            },
        request: request::CLEAR_FEATURE,
        value: feature,
        index,
        length: 0,
    }
}

/// A `SET_INTERFACE` setup packet.
#[must_use]
pub fn set_interface(number: u8, alternate: u8) -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_STANDARD | RECIPIENT_INTERFACE,
        request: request::SET_INTERFACE,
        value: u16::from(alternate),
        index: u16::from(number),
        length: 0,
    }
}

/// A `GET_INTERFACE` setup packet.
#[must_use]
pub fn get_interface(number: u8) -> SetupPacket {
    SetupPacket {
        request_type: REQUEST_DIRECTION_IN | REQUEST_STANDARD | RECIPIENT_INTERFACE,
        request: request::GET_INTERFACE,
        value: 0,
        index: u16::from(number),
        length: 1,
    }
}

/// The transport a control transfer runs over.
///
/// For an OUT request, `data` already contains the payload and its length must equal
/// `setup.length`. For an IN request, `data.len()` is the `wLength` buffer. The return value is
/// the number of bytes transferred (zero for a no-data request).
pub trait ControlTransport {
    /// Run one control transfer.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError::Control`] for a stall/protocol/transport failure.
    fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError>;

    /// Run a no-data control transfer.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError::Control`] for a stall/protocol/transport failure.
    fn control_no_data(&mut self, setup: SetupPacket) -> Result<(), UsbError> {
        if setup.length != 0 {
            return Err(UsbError::Control(ControlError::LengthMismatch {
                expected: usize::from(setup.length),
                got: 0,
            }));
        }
        let mut empty = [];
        let _ = self.control(setup, &mut empty)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "control_ut.rs"]
mod tests;
