// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Standard USB 2.0 descriptors and the control setup packet.
//!
//! All multi-byte fields are little-endian on the wire, as USB defines. Every parser checks the
//! declared `bLength`, the descriptor type and the slice bounds; malformed input returns a typed
//! error and never panics. The values are the public USB 2.0 definitions.

use core::fmt;

/// Device descriptor length.
pub const DEVICE_LEN: usize = 18;
/// Configuration descriptor length (the header; the rest of the set follows).
pub const CONFIG_LEN: usize = 9;
/// Interface descriptor length.
pub const INTERFACE_LEN: usize = 9;
/// Endpoint descriptor length.
pub const ENDPOINT_LEN: usize = 7;

/// Descriptor type: device.
pub const TYPE_DEVICE: u8 = 1;
/// Descriptor type: configuration.
pub const TYPE_CONFIG: u8 = 2;
/// Descriptor type: interface.
pub const TYPE_INTERFACE: u8 = 4;
/// Descriptor type: endpoint.
pub const TYPE_ENDPOINT: u8 = 5;

/// Endpoint direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Host to device.
    Out,
    /// Device to host.
    In,
}

impl Direction {
    /// The direction bit of an endpoint address.
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Out => 0,
            Self::In => 0x80,
        }
    }
}

/// An endpoint address (number and direction).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointAddress {
    /// Endpoint number, `1..=15`.
    pub number: u8,
    /// Direction.
    pub direction: Direction,
}

impl EndpointAddress {
    /// Build an address; `None` for endpoint 0 or a number above 15.
    #[must_use]
    pub const fn new(number: u8, direction: Direction) -> Option<Self> {
        if number == 0 || number > 15 {
            return None;
        }
        Some(Self { number, direction })
    }

    /// The device context index for this endpoint (`(number << 1) | IN`).
    #[must_use]
    pub const fn dci(self) -> u8 {
        (self.number << 1)
            | (if matches!(self.direction, Direction::In) {
                1
            } else {
                0
            })
    }

    /// The wire byte (`0x80 | number` for IN).
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        self.number | self.direction.bit()
    }

    /// Decode the wire byte.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        Self::new(
            byte & 0x0F,
            if byte & 0x80 != 0 {
                Direction::In
            } else {
                Direction::Out
            },
        )
    }
}

/// The endpoint transfer type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    /// Control.
    Control,
    /// Isochronous.
    Isochronous,
    /// Bulk.
    Bulk,
    /// Interrupt.
    Interrupt,
}

/// The isochronous synchronisation type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncType {
    /// No synchronisation.
    None,
    /// Asynchronous (feedback).
    Asynchronous,
    /// Adaptive.
    Adaptive,
    /// Synchronous.
    Synchronous,
}

/// The isochronous usage type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageType {
    /// Data endpoint.
    Data,
    /// Feedback endpoint.
    Feedback,
    /// Implicit feedback data endpoint.
    ImplicitFeedback,
    /// Reserved.
    Reserved,
}

/// Why a descriptor cannot be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorError {
    /// The slice is shorter than the descriptor needs.
    TooShort {
        /// Bytes needed.
        need: usize,
        /// Bytes present.
        got: usize,
    },
    /// The declared `bLength` is not the standard length.
    BadLength {
        /// Declared length.
        got: u8,
        /// Expected length.
        expected: u8,
    },
    /// The descriptor type differs.
    BadType {
        /// Found type.
        got: u8,
        /// Expected type.
        expected: u8,
    },
    /// A field value is invalid.
    BadValue(&'static str),
    /// The output buffer is too small.
    Overflow,
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { need, got } => write!(f, "need {need} bytes, got {got}"),
            Self::BadLength { got, expected } => {
                write!(f, "descriptor length {got} is not {expected}")
            }
            Self::BadType { got, expected } => {
                write!(f, "descriptor type {got} is not {expected}")
            }
            Self::BadValue(what) => write!(f, "invalid value: {what}"),
            Self::Overflow => f.write_str("output buffer is too small"),
        }
    }
}

impl core::error::Error for DescriptorError {}

/// The 8-byte control setup packet.
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
    /// Decode the 8 wire bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 8]) -> Self {
        Self {
            request_type: bytes[0],
            request: bytes[1],
            value: u16::from_le_bytes([bytes[2], bytes[3]]),
            index: u16::from_le_bytes([bytes[4], bytes[5]]),
            length: u16::from_le_bytes([bytes[6], bytes[7]]),
        }
    }

    /// Encode the 8 wire bytes.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 8] {
        let value = self.value.to_le_bytes();
        let index = self.index.to_le_bytes();
        let length = self.length.to_le_bytes();
        [
            self.request_type,
            self.request,
            value[0],
            value[1],
            index[0],
            index[1],
            length[0],
            length[1],
        ]
    }

    /// The direction bit.
    #[must_use]
    pub const fn direction(self) -> Direction {
        if self.request_type & 0x80 != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }

    /// True for a class request to an interface (UAC1 control requests).
    #[must_use]
    pub const fn is_class_interface(self) -> bool {
        self.request_type & 0x60 == 0x20 && self.request_type & 0x1F == 0x01
    }
}

/// The device descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    /// `bcdUSB`.
    pub usb_version: u16,
    /// `bDeviceClass`.
    pub class: u8,
    /// `bDeviceSubClass`.
    pub subclass: u8,
    /// `bDeviceProtocol`.
    pub protocol: u8,
    /// `bMaxPacketSize0`.
    pub max_packet0: u8,
    /// `idVendor`.
    pub vendor: u16,
    /// `idProduct`.
    pub product: u16,
    /// `bcdDevice`.
    pub device_version: u16,
    /// `bNumConfigurations`.
    pub configurations: u8,
}

impl DeviceDescriptor {
    /// Parse a device descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a bad length or a wrong type.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let head = bytes.get(..DEVICE_LEN).ok_or(DescriptorError::TooShort {
            need: DEVICE_LEN,
            got: bytes.len(),
        })?;
        if head[0] != DEVICE_LEN as u8 {
            return Err(DescriptorError::BadLength {
                got: head[0],
                expected: DEVICE_LEN as u8,
            });
        }
        if head[1] != TYPE_DEVICE {
            return Err(DescriptorError::BadType {
                got: head[1],
                expected: TYPE_DEVICE,
            });
        }
        Ok(Self {
            usb_version: u16::from_le_bytes([head[2], head[3]]),
            class: head[4],
            subclass: head[5],
            protocol: head[6],
            max_packet0: head[7],
            vendor: u16::from_le_bytes([head[8], head[9]]),
            product: u16::from_le_bytes([head[10], head[11]]),
            device_version: u16::from_le_bytes([head[12], head[13]]),
            configurations: head[17],
        })
    }

    /// Encode the descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; DEVICE_LEN] {
        let usb = self.usb_version.to_le_bytes();
        let vendor = self.vendor.to_le_bytes();
        let product = self.product.to_le_bytes();
        let device = self.device_version.to_le_bytes();
        [
            DEVICE_LEN as u8,
            TYPE_DEVICE,
            usb[0],
            usb[1],
            self.class,
            self.subclass,
            self.protocol,
            self.max_packet0,
            vendor[0],
            vendor[1],
            product[0],
            product[1],
            device[0],
            device[1],
            0,
            0,
            0,
            self.configurations,
        ]
    }
}

/// The configuration descriptor header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigDescriptor {
    /// `wTotalLength`, including every interface and endpoint.
    pub total_length: u16,
    /// `bNumInterfaces`.
    pub interfaces: u8,
    /// `bConfigurationValue`.
    pub value: u8,
    /// `bmAttributes`.
    pub attributes: u8,
    /// `bMaxPower` in 2 mA units.
    pub max_power: u8,
}

impl ConfigDescriptor {
    /// Parse a configuration descriptor header.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a bad length or a wrong type.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let head = bytes.get(..CONFIG_LEN).ok_or(DescriptorError::TooShort {
            need: CONFIG_LEN,
            got: bytes.len(),
        })?;
        if head[0] != CONFIG_LEN as u8 {
            return Err(DescriptorError::BadLength {
                got: head[0],
                expected: CONFIG_LEN as u8,
            });
        }
        if head[1] != TYPE_CONFIG {
            return Err(DescriptorError::BadType {
                got: head[1],
                expected: TYPE_CONFIG,
            });
        }
        Ok(Self {
            total_length: u16::from_le_bytes([head[2], head[3]]),
            interfaces: head[4],
            value: head[5],
            attributes: head[7],
            max_power: head[8],
        })
    }

    /// Encode the configuration header.
    #[must_use]
    pub fn encode(&self) -> [u8; CONFIG_LEN] {
        let total = self.total_length.to_le_bytes();
        [
            CONFIG_LEN as u8,
            TYPE_CONFIG,
            total[0],
            total[1],
            self.interfaces,
            self.value,
            0,
            self.attributes,
            self.max_power,
        ]
    }
}

/// The interface descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceDescriptor {
    /// `bInterfaceNumber`.
    pub number: u8,
    /// `bAlternateSetting`.
    pub alternate: u8,
    /// `bNumEndpoints`.
    pub endpoints: u8,
    /// `bInterfaceClass`.
    pub class: u8,
    /// `bInterfaceSubClass`.
    pub subclass: u8,
    /// `bInterfaceProtocol`.
    pub protocol: u8,
}

impl InterfaceDescriptor {
    /// Parse an interface descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a bad length or a wrong type.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let head = bytes
            .get(..INTERFACE_LEN)
            .ok_or(DescriptorError::TooShort {
                need: INTERFACE_LEN,
                got: bytes.len(),
            })?;
        if head[0] != INTERFACE_LEN as u8 {
            return Err(DescriptorError::BadLength {
                got: head[0],
                expected: INTERFACE_LEN as u8,
            });
        }
        if head[1] != TYPE_INTERFACE {
            return Err(DescriptorError::BadType {
                got: head[1],
                expected: TYPE_INTERFACE,
            });
        }
        Ok(Self {
            number: head[2],
            alternate: head[3],
            endpoints: head[4],
            class: head[5],
            subclass: head[6],
            protocol: head[7],
        })
    }

    /// Encode the interface descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; INTERFACE_LEN] {
        [
            INTERFACE_LEN as u8,
            TYPE_INTERFACE,
            self.number,
            self.alternate,
            self.endpoints,
            self.class,
            self.subclass,
            self.protocol,
            0,
        ]
    }
}

/// The endpoint descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointDescriptor {
    /// Endpoint address.
    pub address: EndpointAddress,
    /// `bmAttributes`.
    pub attributes: u8,
    /// `wMaxPacketSize` raw.
    pub max_packet: u16,
    /// `bInterval`.
    pub interval: u8,
}

impl EndpointDescriptor {
    /// Parse an endpoint descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a bad length, a wrong type or a bad address.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let head = bytes.get(..ENDPOINT_LEN).ok_or(DescriptorError::TooShort {
            need: ENDPOINT_LEN,
            got: bytes.len(),
        })?;
        if head[0] != ENDPOINT_LEN as u8 {
            return Err(DescriptorError::BadLength {
                got: head[0],
                expected: ENDPOINT_LEN as u8,
            });
        }
        if head[1] != TYPE_ENDPOINT {
            return Err(DescriptorError::BadType {
                got: head[1],
                expected: TYPE_ENDPOINT,
            });
        }
        let address =
            EndpointAddress::from_byte(head[2]).ok_or(DescriptorError::BadValue("endpoint 0"))?;
        Ok(Self {
            address,
            attributes: head[3],
            max_packet: u16::from_le_bytes([head[4], head[5]]),
            interval: head[6],
        })
    }

    /// Encode the endpoint descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; ENDPOINT_LEN] {
        let packet = self.max_packet.to_le_bytes();
        [
            ENDPOINT_LEN as u8,
            TYPE_ENDPOINT,
            self.address.to_byte(),
            self.attributes,
            packet[0],
            packet[1],
            self.interval,
        ]
    }

    /// The transfer type.
    #[must_use]
    pub const fn transfer_type(&self) -> TransferType {
        match self.attributes & 0x03 {
            0 => TransferType::Control,
            1 => TransferType::Isochronous,
            2 => TransferType::Bulk,
            _ => TransferType::Interrupt,
        }
    }

    /// The isochronous synchronisation type.
    #[must_use]
    pub const fn sync_type(&self) -> SyncType {
        match (self.attributes >> 2) & 0x03 {
            0 => SyncType::None,
            1 => SyncType::Asynchronous,
            2 => SyncType::Adaptive,
            _ => SyncType::Synchronous,
        }
    }

    /// The isochronous usage type.
    #[must_use]
    pub const fn usage_type(&self) -> UsageType {
        match (self.attributes >> 4) & 0x03 {
            0 => UsageType::Data,
            1 => UsageType::Feedback,
            2 => UsageType::ImplicitFeedback,
            _ => UsageType::Reserved,
        }
    }

    /// The packet size in bytes (the low 11 bits).
    #[must_use]
    pub const fn max_packet_size(&self) -> u16 {
        self.max_packet & 0x07FF
    }

    /// The additional high-speed transactions (bits 12..11; 1 means one).
    #[must_use]
    pub const fn transactions(&self) -> u8 {
        (((self.max_packet >> 11) & 0x03) as u8).saturating_add(1)
    }
}

#[cfg(test)]
#[path = "usb_ut.rs"]
mod tests;
