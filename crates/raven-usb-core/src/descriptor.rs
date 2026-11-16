// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Standard USB 2.0 device, configuration, interface and endpoint descriptors.
//!
//! The parser is deliberately strict: it checks every declared length against the slice, the
//! descriptor type, the configuration's `wTotalLength`, the per-interface endpoint count and
//! duplicate interface/endpoint addresses. Class-specific descriptors are preserved as raw bytes
//! so a class driver (for example the S60 HID driver) can decode its own records without this
//! crate knowing the class.

use alloc::vec::Vec;
use core::fmt;

/// Device descriptor length.
pub const DEVICE_LEN: usize = 18;
/// Configuration descriptor header length.
pub const CONFIG_LEN: usize = 9;
/// Interface descriptor length.
pub const INTERFACE_LEN: usize = 9;
/// Endpoint descriptor length.
pub const ENDPOINT_LEN: usize = 7;

/// Device descriptor type.
pub const TYPE_DEVICE: u8 = 1;
/// Configuration descriptor type.
pub const TYPE_CONFIG: u8 = 2;
/// String descriptor type.
pub const TYPE_STRING: u8 = 3;
/// Interface descriptor type.
pub const TYPE_INTERFACE: u8 = 4;
/// Endpoint descriptor type.
pub const TYPE_ENDPOINT: u8 = 5;

/// Why a descriptor cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescriptorError {
    /// The slice is shorter than the fixed descriptor.
    TooShort {
        /// Required bytes.
        needed: usize,
        /// Available bytes.
        got: usize,
    },
    /// The encoded length is not the standard length.
    BadLength {
        /// Descriptor name.
        descriptor: &'static str,
        /// Expected length.
        expected: u8,
        /// Encoded length.
        got: u8,
    },
    /// The descriptor type differs.
    BadType {
        /// Descriptor name.
        descriptor: &'static str,
        /// Expected type.
        expected: u8,
        /// Encoded type.
        got: u8,
    },
    /// `bMaxPacketSize0` is not 8, 16, 32 or 64.
    InvalidMaxPacket0(u8),
    /// The device declares no configurations.
    ZeroConfigurations,
    /// A configuration declares zero interfaces.
    ZeroInterfaces,
    /// `wTotalLength` is below the header or beyond the slice.
    BadTotalLength {
        /// Declared total.
        declared: u16,
        /// Available bytes.
        available: usize,
    },
    /// A configuration value is zero.
    ZeroConfigurationValue,
    /// A descriptor length is below two or extends past the configuration.
    BadDescriptorLength {
        /// Offset inside the configuration set.
        offset: usize,
        /// Encoded length.
        length: u8,
    },
    /// An endpoint appears before its interface.
    EndpointBeforeInterface,
    /// A class-specific descriptor appears before its interface.
    ClassDescriptorBeforeInterface,
    /// An interface declares a different endpoint count than it contains.
    EndpointCount {
        /// Interface number.
        interface: u8,
        /// Alternate setting.
        alternate: u8,
        /// Declared count.
        declared: u8,
        /// Parsed count.
        found: usize,
    },
    /// Two interface descriptors use the same number/alternate pair.
    DuplicateInterface {
        /// Interface number.
        number: u8,
        /// Alternate setting.
        alternate: u8,
    },
    /// Two endpoints in one interface use the same address.
    DuplicateEndpoint {
        /// Interface number.
        interface: u8,
        /// Endpoint address byte.
        address: u8,
    },
    /// The number of unique interfaces does not match `bNumInterfaces`.
    InterfaceCount {
        /// Declared count.
        declared: u8,
        /// Parsed unique count.
        found: usize,
    },
    /// The device descriptor's configuration count does not match the parsed set.
    ConfigurationCount {
        /// Declared count.
        declared: u8,
        /// Parsed count.
        found: usize,
    },
    /// The endpoint address is zero, reserved or out of range.
    InvalidEndpointAddress(u8),
    /// The endpoint attributes use a reserved transfer type or reserved bits.
    InvalidEndpointAttributes(u8),
    /// The endpoint declares a zero maximum packet size.
    ZeroPacketSize,
    /// A config descriptor appears inside another config descriptor.
    NestedConfiguration,
    /// The configuration set ended in the middle of a descriptor.
    Truncated {
        /// Offset inside the configuration set.
        offset: usize,
    },
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { needed, got } => {
                write!(f, "descriptor needs {needed} bytes, got {got}")
            }
            Self::BadLength {
                descriptor,
                expected,
                got,
            } => write!(f, "{descriptor} length {got}, expected {expected}"),
            Self::BadType {
                descriptor,
                expected,
                got,
            } => write!(f, "{descriptor} type {got}, expected {expected}"),
            Self::InvalidMaxPacket0(value) => write!(f, "invalid bMaxPacketSize0 {value}"),
            Self::ZeroConfigurations => f.write_str("device declares no configurations"),
            Self::ZeroInterfaces => f.write_str("configuration declares no interfaces"),
            Self::BadTotalLength {
                declared,
                available,
            } => write!(f, "wTotalLength {declared} exceeds {available} bytes"),
            Self::ZeroConfigurationValue => f.write_str("configuration value is zero"),
            Self::BadDescriptorLength { offset, length } => {
                write!(f, "descriptor at {offset} has invalid length {length}")
            }
            Self::EndpointBeforeInterface => f.write_str("endpoint before interface"),
            Self::ClassDescriptorBeforeInterface => {
                f.write_str("class-specific descriptor before interface")
            }
            Self::EndpointCount {
                interface,
                alternate,
                declared,
                found,
            } => write!(
                f,
                "interface {interface}.{alternate} declares {declared} endpoints, parsed {found}"
            ),
            Self::DuplicateInterface { number, alternate } => {
                write!(f, "duplicate interface {number}.{alternate}")
            }
            Self::DuplicateEndpoint { interface, address } => {
                write!(
                    f,
                    "duplicate endpoint {address:#04x} on interface {interface}"
                )
            }
            Self::InterfaceCount { declared, found } => {
                write!(f, "bNumInterfaces {declared}, parsed {found}")
            }
            Self::ConfigurationCount { declared, found } => {
                write!(f, "bNumConfigurations {declared}, parsed {found}")
            }
            Self::InvalidEndpointAddress(address) => {
                write!(f, "invalid endpoint address {address:#04x}")
            }
            Self::InvalidEndpointAttributes(attributes) => {
                write!(f, "invalid endpoint attributes {attributes:#04x}")
            }
            Self::ZeroPacketSize => f.write_str("endpoint has a zero maximum packet size"),
            Self::NestedConfiguration => f.write_str("nested configuration descriptor"),
            Self::Truncated { offset } => write!(f, "configuration set truncated at {offset}"),
        }
    }
}

impl core::error::Error for DescriptorError {}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let low = *bytes.get(offset)?;
    let high = *bytes.get(offset.checked_add(1)?)?;
    Some(u16::from_le_bytes([low, high]))
}

/// The standard device descriptor.
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
    pub max_packet_size_0: u8,
    /// `idVendor`.
    pub vendor_id: u16,
    /// `idProduct`.
    pub product_id: u16,
    /// `bcdDevice`.
    pub device_version: u16,
    /// `iManufacturer`.
    pub manufacturer: u8,
    /// `iProduct`.
    pub product: u8,
    /// `iSerialNumber`.
    pub serial_number: u8,
    /// `bNumConfigurations`.
    pub num_configurations: u8,
}

impl DeviceDescriptor {
    /// Parse the 18-byte device descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a wrong length/type, a bad
    /// `bMaxPacketSize0` or zero configurations.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let descriptor = bytes.get(..DEVICE_LEN).ok_or(DescriptorError::TooShort {
            needed: DEVICE_LEN,
            got: bytes.len(),
        })?;
        if descriptor[0] != DEVICE_LEN as u8 {
            return Err(DescriptorError::BadLength {
                descriptor: "device",
                expected: DEVICE_LEN as u8,
                got: descriptor[0],
            });
        }
        if descriptor[1] != TYPE_DEVICE {
            return Err(DescriptorError::BadType {
                descriptor: "device",
                expected: TYPE_DEVICE,
                got: descriptor[1],
            });
        }
        if !matches!(descriptor[7], 8 | 16 | 32 | 64) {
            return Err(DescriptorError::InvalidMaxPacket0(descriptor[7]));
        }
        if descriptor[17] == 0 {
            return Err(DescriptorError::ZeroConfigurations);
        }
        Ok(Self {
            usb_version: read_u16(descriptor, 2).unwrap_or(0),
            class: descriptor[4],
            subclass: descriptor[5],
            protocol: descriptor[6],
            max_packet_size_0: descriptor[7],
            vendor_id: read_u16(descriptor, 8).unwrap_or(0),
            product_id: read_u16(descriptor, 10).unwrap_or(0),
            device_version: read_u16(descriptor, 12).unwrap_or(0),
            manufacturer: descriptor[14],
            product: descriptor[15],
            serial_number: descriptor[16],
            num_configurations: descriptor[17],
        })
    }

    /// Encode the descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; DEVICE_LEN] {
        let mut out = [0u8; DEVICE_LEN];
        out[0] = DEVICE_LEN as u8;
        out[1] = TYPE_DEVICE;
        out[2..4].copy_from_slice(&self.usb_version.to_le_bytes());
        out[4] = self.class;
        out[5] = self.subclass;
        out[6] = self.protocol;
        out[7] = self.max_packet_size_0;
        out[8..10].copy_from_slice(&self.vendor_id.to_le_bytes());
        out[10..12].copy_from_slice(&self.product_id.to_le_bytes());
        out[12..14].copy_from_slice(&self.device_version.to_le_bytes());
        out[14] = self.manufacturer;
        out[15] = self.product;
        out[16] = self.serial_number;
        out[17] = self.num_configurations;
        out
    }
}

/// The configuration descriptor header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigurationHeader {
    /// `wTotalLength`, including the interface and endpoint descriptors.
    pub total_length: u16,
    /// `bNumInterfaces` (unique interface numbers, not alternate settings).
    pub num_interfaces: u8,
    /// `bConfigurationValue` (non-zero).
    pub configuration_value: u8,
    /// `iConfiguration`.
    pub configuration_string: u8,
    /// `bmAttributes`.
    pub attributes: u8,
    /// `bMaxPower` in 2 mA units.
    pub max_power: u8,
}

impl ConfigurationHeader {
    /// Parse the 9-byte configuration header.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a wrong length/type, a zero value or a
    /// `wTotalLength` below the header.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let descriptor = bytes.get(..CONFIG_LEN).ok_or(DescriptorError::TooShort {
            needed: CONFIG_LEN,
            got: bytes.len(),
        })?;
        if descriptor[0] != CONFIG_LEN as u8 {
            return Err(DescriptorError::BadLength {
                descriptor: "configuration",
                expected: CONFIG_LEN as u8,
                got: descriptor[0],
            });
        }
        if descriptor[1] != TYPE_CONFIG {
            return Err(DescriptorError::BadType {
                descriptor: "configuration",
                expected: TYPE_CONFIG,
                got: descriptor[1],
            });
        }
        let total_length = read_u16(descriptor, 2).unwrap_or(0);
        if total_length < CONFIG_LEN as u16 {
            return Err(DescriptorError::BadTotalLength {
                declared: total_length,
                available: bytes.len(),
            });
        }
        if descriptor[5] == 0 {
            return Err(DescriptorError::ZeroConfigurationValue);
        }
        Ok(Self {
            total_length,
            num_interfaces: descriptor[4],
            configuration_value: descriptor[5],
            configuration_string: descriptor[6],
            attributes: descriptor[7],
            max_power: descriptor[8],
        })
    }

    /// Encode the header.
    #[must_use]
    pub fn encode(&self) -> [u8; CONFIG_LEN] {
        let mut out = [0u8; CONFIG_LEN];
        out[0] = CONFIG_LEN as u8;
        out[1] = TYPE_CONFIG;
        out[2..4].copy_from_slice(&self.total_length.to_le_bytes());
        out[4] = self.num_interfaces;
        out[5] = self.configuration_value;
        out[6] = self.configuration_string;
        out[7] = self.attributes;
        out[8] = self.max_power;
        out
    }
}

/// The standard interface descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceDescriptor {
    /// `bInterfaceNumber`.
    pub number: u8,
    /// `bAlternateSetting`.
    pub alternate: u8,
    /// `bNumEndpoints`.
    pub num_endpoints: u8,
    /// `bInterfaceClass`.
    pub class: u8,
    /// `bInterfaceSubClass`.
    pub subclass: u8,
    /// `bInterfaceProtocol`.
    pub protocol: u8,
    /// `iInterface`.
    pub interface_string: u8,
}

impl InterfaceDescriptor {
    /// Parse the 9-byte interface descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice or a wrong length/type.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let descriptor = bytes
            .get(..INTERFACE_LEN)
            .ok_or(DescriptorError::TooShort {
                needed: INTERFACE_LEN,
                got: bytes.len(),
            })?;
        if descriptor[0] != INTERFACE_LEN as u8 {
            return Err(DescriptorError::BadLength {
                descriptor: "interface",
                expected: INTERFACE_LEN as u8,
                got: descriptor[0],
            });
        }
        if descriptor[1] != TYPE_INTERFACE {
            return Err(DescriptorError::BadType {
                descriptor: "interface",
                expected: TYPE_INTERFACE,
                got: descriptor[1],
            });
        }
        Ok(Self {
            number: descriptor[2],
            alternate: descriptor[3],
            num_endpoints: descriptor[4],
            class: descriptor[5],
            subclass: descriptor[6],
            protocol: descriptor[7],
            interface_string: descriptor[8],
        })
    }

    /// Encode the descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; INTERFACE_LEN] {
        [
            INTERFACE_LEN as u8,
            TYPE_INTERFACE,
            self.number,
            self.alternate,
            self.num_endpoints,
            self.class,
            self.subclass,
            self.protocol,
            self.interface_string,
        ]
    }
}

/// The endpoint direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    /// Host-to-device (`OUT`).
    Out,
    /// Device-to-host (`IN`).
    In,
}

/// The endpoint address (`bEndpointAddress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EndpointAddress {
    /// Endpoint number 1..=15.
    pub number: u8,
    /// Direction.
    pub direction: Direction,
}

impl EndpointAddress {
    /// Build an address.
    #[must_use]
    pub const fn new(number: u8, direction: Direction) -> Option<Self> {
        if number == 0 || number > 15 {
            return None;
        }
        Some(Self { number, direction })
    }

    /// Decode `bEndpointAddress`.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        if byte & 0x70 != 0 {
            return None;
        }
        let number = byte & 0x0f;
        if number == 0 {
            return None;
        }
        Some(Self {
            number,
            direction: if byte & 0x80 != 0 {
                Direction::In
            } else {
                Direction::Out
            },
        })
    }

    /// Encode `bEndpointAddress`.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        self.number
            | if matches!(self.direction, Direction::In) {
                0x80
            } else {
                0
            }
    }

    /// Device context index (`2 * number + IN`).
    #[must_use]
    pub const fn dci(self) -> u8 {
        self.number.saturating_mul(2)
            + if matches!(self.direction, Direction::In) {
                1
            } else {
                0
            }
    }
}

/// The endpoint transfer type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    /// Isochronous.
    Isochronous,
    /// Bulk.
    Bulk,
    /// Interrupt.
    Interrupt,
}

impl TransferType {
    /// Decode the low two attribute bits; `None` for control (not valid outside EP0).
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits & 0x3 {
            0 => None,
            1 => Some(Self::Isochronous),
            2 => Some(Self::Bulk),
            3 => Some(Self::Interrupt),
            _ => None,
        }
    }

    /// Encode the low two attribute bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        match self {
            Self::Isochronous => 1,
            Self::Bulk => 2,
            Self::Interrupt => 3,
        }
    }
}

/// The standard endpoint descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointDescriptor {
    /// `bEndpointAddress`.
    pub address: EndpointAddress,
    /// Raw `bmAttributes`.
    pub attributes: u8,
    /// `wMaxPacketSize` after masking the reserved bits.
    pub max_packet_size: u16,
    /// `bInterval`.
    pub interval: u8,
}

impl EndpointDescriptor {
    /// Parse the 7-byte endpoint descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a short slice, a wrong length/type, an invalid address or
    /// attributes, or a zero packet size.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let descriptor = bytes.get(..ENDPOINT_LEN).ok_or(DescriptorError::TooShort {
            needed: ENDPOINT_LEN,
            got: bytes.len(),
        })?;
        if descriptor[0] != ENDPOINT_LEN as u8 {
            return Err(DescriptorError::BadLength {
                descriptor: "endpoint",
                expected: ENDPOINT_LEN as u8,
                got: descriptor[0],
            });
        }
        if descriptor[1] != TYPE_ENDPOINT {
            return Err(DescriptorError::BadType {
                descriptor: "endpoint",
                expected: TYPE_ENDPOINT,
                got: descriptor[1],
            });
        }
        let address = EndpointAddress::from_byte(descriptor[2])
            .ok_or(DescriptorError::InvalidEndpointAddress(descriptor[2]))?;
        let attributes = descriptor[3];
        if attributes & 0xc0 != 0 || TransferType::from_bits(attributes).is_none() {
            return Err(DescriptorError::InvalidEndpointAttributes(attributes));
        }
        let max_packet_size = read_u16(descriptor, 4).unwrap_or(0) & 0x07ff;
        if max_packet_size == 0 {
            return Err(DescriptorError::ZeroPacketSize);
        }
        Ok(Self {
            address,
            attributes,
            max_packet_size,
            interval: descriptor[6],
        })
    }

    /// Encode the descriptor.
    #[must_use]
    pub fn encode(&self) -> [u8; ENDPOINT_LEN] {
        let mut out = [0u8; ENDPOINT_LEN];
        out[0] = ENDPOINT_LEN as u8;
        out[1] = TYPE_ENDPOINT;
        out[2] = self.address.to_byte();
        out[3] = self.attributes;
        out[4..6].copy_from_slice(&self.max_packet_size.to_le_bytes());
        out[6] = self.interval;
        out
    }

    /// The transfer type.
    #[must_use]
    pub const fn transfer_type(&self) -> Option<TransferType> {
        TransferType::from_bits(self.attributes)
    }

    /// The endpoint address.
    #[must_use]
    pub const fn address(&self) -> EndpointAddress {
        self.address
    }

    /// The device context index (`2 * number + IN`).
    #[must_use]
    pub const fn dci(&self) -> u8 {
        self.address.dci()
    }
}

/// One interface and its endpoints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    /// The standard interface descriptor.
    pub descriptor: InterfaceDescriptor,
    /// Endpoints in descriptor order.
    pub endpoints: Vec<EndpointDescriptor>,
    /// Class-specific descriptors (for example HID) preserved as raw bytes.
    pub class_specific: Vec<Vec<u8>>,
}

impl Interface {
    /// Find an endpoint by address.
    #[must_use]
    pub fn endpoint(&self, address: EndpointAddress) -> Option<&EndpointDescriptor> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.address == address)
    }
}

/// One complete configuration and its interfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// The configuration header.
    pub header: ConfigurationHeader,
    /// Interfaces in descriptor order.
    pub interfaces: Vec<Interface>,
}

impl Configuration {
    /// Parse a complete configuration descriptor set.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError`] for a malformed header, a truncated/overlapping descriptor, an
    /// endpoint before an interface, a declared/parsed endpoint mismatch, duplicate
    /// interface/endpoint addresses or a `bNumInterfaces` mismatch.
    pub fn parse(bytes: &[u8]) -> Result<Self, DescriptorError> {
        let header = ConfigurationHeader::parse(bytes)?;
        let total = usize::from(header.total_length);
        if total > bytes.len() {
            return Err(DescriptorError::BadTotalLength {
                declared: header.total_length,
                available: bytes.len(),
            });
        }
        let mut interfaces: Vec<Interface> = Vec::new();
        let mut current: Option<Interface> = None;
        let mut offset = CONFIG_LEN;
        while offset < total {
            let length = *bytes
                .get(offset)
                .ok_or(DescriptorError::Truncated { offset })?;
            if length < 2 {
                return Err(DescriptorError::BadDescriptorLength { offset, length });
            }
            let end = offset
                .checked_add(usize::from(length))
                .ok_or(DescriptorError::Truncated { offset })?;
            if end > total {
                return Err(DescriptorError::BadDescriptorLength { offset, length });
            }
            let kind = *bytes
                .get(
                    offset
                        .checked_add(1)
                        .ok_or(DescriptorError::Truncated { offset })?,
                )
                .ok_or(DescriptorError::Truncated { offset })?;
            let descriptor = bytes
                .get(offset..end)
                .ok_or(DescriptorError::Truncated { offset })?;
            match kind {
                TYPE_INTERFACE => {
                    if let Some(previous) = current.take() {
                        push_interface(&mut interfaces, previous)?;
                    }
                    let descriptor = InterfaceDescriptor::parse(descriptor)?;
                    current = Some(Interface {
                        descriptor,
                        endpoints: Vec::new(),
                        class_specific: Vec::new(),
                    });
                }
                TYPE_ENDPOINT => {
                    let Some(interface) = current.as_mut() else {
                        return Err(DescriptorError::EndpointBeforeInterface);
                    };
                    let endpoint = EndpointDescriptor::parse(descriptor)?;
                    if interface
                        .endpoints
                        .iter()
                        .any(|item| item.address == endpoint.address)
                    {
                        return Err(DescriptorError::DuplicateEndpoint {
                            interface: interface.descriptor.number,
                            address: endpoint.address.to_byte(),
                        });
                    }
                    interface.endpoints.push(endpoint);
                }
                TYPE_CONFIG => return Err(DescriptorError::NestedConfiguration),
                _ => {
                    let Some(interface) = current.as_mut() else {
                        return Err(DescriptorError::ClassDescriptorBeforeInterface);
                    };
                    interface.class_specific.push(descriptor.to_vec());
                }
            }
            offset = end;
        }
        if offset != total {
            return Err(DescriptorError::Truncated { offset });
        }
        if let Some(previous) = current.take() {
            push_interface(&mut interfaces, previous)?;
        }
        if interfaces.is_empty() {
            return Err(DescriptorError::ZeroInterfaces);
        }
        let mut unique: Vec<u8> = Vec::new();
        for interface in &interfaces {
            if !unique.contains(&interface.descriptor.number) {
                unique.push(interface.descriptor.number);
            }
        }
        if unique.len() != usize::from(header.num_interfaces) {
            return Err(DescriptorError::InterfaceCount {
                declared: header.num_interfaces,
                found: unique.len(),
            });
        }
        Ok(Self { header, interfaces })
    }

    /// Find an interface by number and alternate setting.
    #[must_use]
    pub fn interface(&self, number: u8, alternate: u8) -> Option<&Interface> {
        self.interfaces.iter().find(|interface| {
            interface.descriptor.number == number && interface.descriptor.alternate == alternate
        })
    }

    /// Find the alternate setting zero (or the first alternate) for `number`.
    #[must_use]
    pub fn default_interface(&self, number: u8) -> Option<&Interface> {
        self.interface(number, 0).or_else(|| {
            self.interfaces
                .iter()
                .find(|interface| interface.descriptor.number == number)
        })
    }
}

/// A device descriptor plus its parsed configurations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The device descriptor.
    pub descriptor: DeviceDescriptor,
    /// Configurations in index order.
    pub configurations: Vec<Configuration>,
}

impl Device {
    /// Validate and build a device from its descriptor and configurations.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptorError::ConfigurationCount`] when the number of configurations does not
    /// match `bNumConfigurations`.
    pub fn new(
        descriptor: DeviceDescriptor,
        configurations: Vec<Configuration>,
    ) -> Result<Self, DescriptorError> {
        if usize::from(descriptor.num_configurations) != configurations.len() {
            return Err(DescriptorError::ConfigurationCount {
                declared: descriptor.num_configurations,
                found: configurations.len(),
            });
        }
        Ok(Self {
            descriptor,
            configurations,
        })
    }

    /// Find a configuration by `bConfigurationValue`.
    #[must_use]
    pub fn configuration(&self, value: u8) -> Option<&Configuration> {
        self.configurations
            .iter()
            .find(|configuration| configuration.header.configuration_value == value)
    }
}

fn push_interface(
    interfaces: &mut Vec<Interface>,
    interface: Interface,
) -> Result<(), DescriptorError> {
    if interfaces.iter().any(|item| {
        item.descriptor.number == interface.descriptor.number
            && item.descriptor.alternate == interface.descriptor.alternate
    }) {
        return Err(DescriptorError::DuplicateInterface {
            number: interface.descriptor.number,
            alternate: interface.descriptor.alternate,
        });
    }
    if usize::from(interface.descriptor.num_endpoints) != interface.endpoints.len() {
        return Err(DescriptorError::EndpointCount {
            interface: interface.descriptor.number,
            alternate: interface.descriptor.alternate,
            declared: interface.descriptor.num_endpoints,
            found: interface.endpoints.len(),
        });
    }
    interfaces.push(interface);
    Ok(())
}

#[cfg(test)]
#[path = "descriptor_ut.rs"]
mod tests;
