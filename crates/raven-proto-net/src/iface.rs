// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interface, route and ARP control for the `net:` control channel.
//!
//! The same framing as the socket protocol applies: a little-endian `u16` version first, then an
//! operation code and a bounded body. Interface names are at most [`MAX_IFNAME`] ASCII bytes;
//! addresses are four octets with a prefix length, never a pointer into the sender's memory.

use core::fmt;

use raven_abi::errno::Errno;

use crate::DecodeError;
use crate::socket::PROTOCOL_VERSION;
use crate::wire::{Reader, WireError, Writer};

/// Maximum interface name length in bytes.
pub const MAX_IFNAME: usize = 16;

/// Interface is administratively up.
pub const IF_UP: u8 = 1 << 0;
/// Interface has carrier.
pub const IF_LINK_UP: u8 = 1 << 1;
/// Interface is the loopback device.
pub const IF_LOOPBACK: u8 = 1 << 2;

/// ARP entry is resolved.
pub const ARP_COMPLETE: u8 = 1 << 0;
/// ARP entry is static (never expires).
pub const ARP_STATIC: u8 = 1 << 1;

/// Interface control operation: list interfaces.
pub const OP_LIST: u16 = 1;
/// Interface control operation: get one interface.
pub const OP_GET: u16 = 2;
/// Interface control operation: set address and netmask.
pub const OP_SET_ADDRESS: u16 = 3;
/// Interface control operation: set administrative state.
pub const OP_SET_STATE: u16 = 4;
/// Interface control operation: add or replace a route.
pub const OP_ROUTE_ADD: u16 = 5;
/// Interface control operation: delete a route.
pub const OP_ROUTE_DEL: u16 = 6;
/// Interface control operation: read the ARP table.
pub const OP_ARP_LIST: u16 = 7;

/// Response tag: success without a body.
const TAG_OK: u16 = 0;
/// Response tag: entry count.
const TAG_COUNT: u16 = 1;
/// Response tag: interface record.
const TAG_IFACE: u16 = 2;
/// Response tag: route record.
const TAG_ROUTE: u16 = 3;
/// Response tag: ARP record.
const TAG_ARP: u16 = 4;
/// Response tag: errno.
const TAG_ERR: u16 = 5;

/// Route flag: a gateway follows.
const ROUTE_FLAG_GATEWAY: u8 = 0x01;

/// A validated interface name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IfName {
    bytes: [u8; MAX_IFNAME],
    length: u8,
}

impl IfName {
    /// Validate and copy `name`.
    ///
    /// Names are non-empty ASCII letters, digits, underscore, hyphen or dot, at most
    /// [`MAX_IFNAME`] bytes; that covers `lo`, `eth0`, `virtio0`, `genet0` and `usb0`.
    pub fn new(name: &str) -> Option<Self> {
        let length = name.len();
        if length == 0 || length > MAX_IFNAME {
            return None;
        }
        if !name.bytes().all(is_name_byte) {
            return None;
        }
        let mut bytes = [0u8; MAX_IFNAME];
        bytes[..length].copy_from_slice(name.as_bytes());
        Some(Self {
            bytes,
            length: length as u8,
        })
    }

    /// Borrow the name as a string slice.
    pub fn as_str(&self) -> &str {
        // `new` and `read` only store validated ASCII.
        core::str::from_utf8(&self.bytes[..self.length as usize]).unwrap_or("")
    }

    /// Encoded size in bytes.
    pub fn encoded_len(&self) -> usize {
        1 + self.length as usize
    }

    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        writer.put_u8(self.length)?;
        writer.put_bytes(&self.bytes[..self.length as usize])
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let length = reader.get_u8()? as usize;
        if length == 0 || length > MAX_IFNAME {
            return Err(WireError::Invalid);
        }
        let bytes = reader.get_bytes(length)?;
        if !bytes.iter().copied().all(is_name_byte) {
            return Err(WireError::Invalid);
        }
        let mut name = [0u8; MAX_IFNAME];
        name[..length].copy_from_slice(bytes);
        Ok(Self {
            bytes: name,
            length: length as u8,
        })
    }
}

impl fmt::Debug for IfName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

/// ASCII byte allowed in an interface name.
const fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

/// One interface record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IfaceInfo {
    /// Interface name.
    pub name: IfName,
    /// IPv4 address.
    pub addr: [u8; 4],
    /// Netmask length in bits, `0..=32`.
    pub prefix: u8,
    /// [`IF_UP`], [`IF_LINK_UP`] and [`IF_LOOPBACK`] bits.
    pub flags: u8,
    /// MTU in bytes.
    pub mtu: u16,
    /// Hardware address.
    pub mac: [u8; 6],
}

impl IfaceInfo {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        if self.prefix > 32 || self.mtu == 0 {
            return Err(WireError::Invalid);
        }
        self.name.write(writer)?;
        writer.put_bytes(&self.addr)?;
        writer.put_u8(self.prefix)?;
        writer.put_u8(self.flags)?;
        writer.put_u16(self.mtu)?;
        writer.put_bytes(&self.mac)
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let name = IfName::read(reader).map_err(DecodeError::from)?;
        let addr = reader.get_bytes(4).map_err(DecodeError::from)?;
        let prefix = reader.get_u8().map_err(DecodeError::from)?;
        let flags = reader.get_u8().map_err(DecodeError::from)?;
        let mtu = reader.get_u16().map_err(DecodeError::from)?;
        let mac = reader.get_bytes(6).map_err(DecodeError::from)?;
        if prefix > 32 || mtu == 0 {
            return Err(DecodeError::malformed());
        }
        Ok(Self {
            name,
            addr: [addr[0], addr[1], addr[2], addr[3]],
            prefix,
            flags,
            mtu,
            mac: [mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
        })
    }
}

/// One routing table entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    /// Destination prefix.
    pub dest: [u8; 4],
    /// Destination prefix length in bits, `0..=32`; zero is the default route.
    pub prefix: u8,
    /// Next hop, absent for a directly connected prefix.
    pub gateway: Option<[u8; 4]>,
}

impl Route {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        if self.prefix > 32 {
            return Err(WireError::Invalid);
        }
        writer.put_bytes(&self.dest)?;
        writer.put_u8(self.prefix)?;
        writer.put_u8(u8::from(self.gateway.is_some()))?;
        if let Some(gateway) = &self.gateway {
            writer.put_bytes(gateway)?;
        }
        Ok(())
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let dest = reader.get_bytes(4).map_err(DecodeError::from)?;
        let prefix = reader.get_u8().map_err(DecodeError::from)?;
        let flags = reader.get_u8().map_err(DecodeError::from)?;
        if prefix > 32 || flags & !ROUTE_FLAG_GATEWAY != 0 {
            return Err(DecodeError::malformed());
        }
        let gateway = if flags & ROUTE_FLAG_GATEWAY != 0 {
            let bytes = reader.get_bytes(4).map_err(DecodeError::from)?;
            Some([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            None
        };
        Ok(Self {
            dest: [dest[0], dest[1], dest[2], dest[3]],
            prefix,
            gateway,
        })
    }
}

/// One ARP table entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArpEntry {
    /// Neighbour IPv4 address.
    pub ip: [u8; 4],
    /// Neighbour hardware address.
    pub mac: [u8; 6],
    /// [`ARP_COMPLETE`] and [`ARP_STATIC`] bits.
    pub flags: u8,
}

impl ArpEntry {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        writer.put_bytes(&self.ip)?;
        writer.put_bytes(&self.mac)?;
        writer.put_u8(self.flags)
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let ip = reader.get_bytes(4).map_err(DecodeError::from)?;
        let mac = reader.get_bytes(6).map_err(DecodeError::from)?;
        Ok(Self {
            ip: [ip[0], ip[1], ip[2], ip[3]],
            mac: [mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]],
            flags: reader.get_u8().map_err(DecodeError::from)?,
        })
    }
}

/// A request on the `net:` control channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IfaceRequest {
    /// List interfaces.
    List,
    /// Get one interface by name.
    Get {
        /// Interface name.
        name: IfName,
    },
    /// Set an interface address and netmask.
    SetAddress {
        /// Interface name.
        name: IfName,
        /// New IPv4 address.
        addr: [u8; 4],
        /// Netmask length in bits.
        prefix: u8,
    },
    /// Bring an interface up or down.
    SetState {
        /// Interface name.
        name: IfName,
        /// `true` for up, `false` for down.
        up: bool,
    },
    /// Add or replace a route.
    RouteAdd {
        /// Route to add.
        route: Route,
    },
    /// Delete a route.
    RouteDel {
        /// Destination prefix to delete.
        dest: [u8; 4],
        /// Destination prefix length in bits.
        prefix: u8,
    },
    /// Read the ARP table.
    ArpList,
}

impl IfaceRequest {
    /// Encode the request into `out` and return the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::List => writer.put_u16(OP_LIST)?,
            Self::Get { name } => {
                writer.put_u16(OP_GET)?;
                name.write(&mut writer)?;
            }
            Self::SetAddress { name, addr, prefix } => {
                if *prefix > 32 {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_SET_ADDRESS)?;
                name.write(&mut writer)?;
                writer.put_bytes(addr)?;
                writer.put_u8(*prefix)?;
            }
            Self::SetState { name, up } => {
                writer.put_u16(OP_SET_STATE)?;
                name.write(&mut writer)?;
                writer.put_u8(u8::from(*up))?;
            }
            Self::RouteAdd { route } => {
                writer.put_u16(OP_ROUTE_ADD)?;
                route.write(&mut writer)?;
            }
            Self::RouteDel { dest, prefix } => {
                if *prefix > 32 {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_ROUTE_DEL)?;
                writer.put_bytes(dest)?;
                writer.put_u8(*prefix)?;
            }
            Self::ArpList => writer.put_u16(OP_ARP_LIST)?,
        }
        Ok(writer.position())
    }

    /// Decode one request. The entire slice must belong to the request.
    pub fn decode(buffer: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let op = reader.get_u16().map_err(DecodeError::from)?;
        let request = match op {
            OP_LIST => Self::List,
            OP_GET => Self::Get {
                name: IfName::read(&mut reader).map_err(DecodeError::from)?,
            },
            OP_SET_ADDRESS => {
                let name = IfName::read(&mut reader).map_err(DecodeError::from)?;
                let addr = reader.get_bytes(4).map_err(DecodeError::from)?;
                let prefix = reader.get_u8().map_err(DecodeError::from)?;
                if prefix > 32 {
                    return Err(DecodeError::malformed());
                }
                Self::SetAddress {
                    name,
                    addr: [addr[0], addr[1], addr[2], addr[3]],
                    prefix,
                }
            }
            OP_SET_STATE => {
                let name = IfName::read(&mut reader).map_err(DecodeError::from)?;
                let up = match reader.get_u8().map_err(DecodeError::from)? {
                    0 => false,
                    1 => true,
                    _ => return Err(DecodeError::malformed()),
                };
                Self::SetState { name, up }
            }
            OP_ROUTE_ADD => Self::RouteAdd {
                route: Route::read(&mut reader)?,
            },
            OP_ROUTE_DEL => {
                let dest = reader.get_bytes(4).map_err(DecodeError::from)?;
                let prefix = reader.get_u8().map_err(DecodeError::from)?;
                if prefix > 32 {
                    return Err(DecodeError::malformed());
                }
                Self::RouteDel {
                    dest: [dest[0], dest[1], dest[2], dest[3]],
                    prefix,
                }
            }
            OP_ARP_LIST => Self::ArpList,
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(request)
    }
}

/// A response on the `net:` control channel.
///
/// `List`, `RouteList` and `ArpList` produce one [`IfaceResponse::Count`] followed by that many
/// [`IfaceResponse::Iface`], [`IfaceResponse::Route`] or [`IfaceResponse::Arp`] records. The
/// request/response layer above uses the count to stop reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IfaceResponse {
    /// Success without a body.
    Ok,
    /// Number of records that follow.
    Count {
        /// Record count.
        count: u16,
    },
    /// One interface record.
    Iface(IfaceInfo),
    /// One route record.
    Route(Route),
    /// One ARP record.
    Arp(ArpEntry),
    /// Failure carrying the errno the caller must report.
    Err(Errno),
}

impl IfaceResponse {
    /// Build an error response.
    pub const fn error(errno: Errno) -> Self {
        Self::Err(errno)
    }

    /// Encode the response into `out` and return the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_u16(PROTOCOL_VERSION)?;
        match self {
            Self::Ok => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_OK)?;
            }
            Self::Count { count } => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_COUNT)?;
                writer.put_u16(*count)?;
            }
            Self::Iface(info) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_IFACE)?;
                info.write(&mut writer)?;
            }
            Self::Route(route) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_ROUTE)?;
                route.write(&mut writer)?;
            }
            Self::Arp(entry) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_ARP)?;
                entry.write(&mut writer)?;
            }
            Self::Err(errno) => {
                writer.put_u16(errno.0)?;
                writer.put_u16(TAG_ERR)?;
            }
        }
        Ok(writer.position())
    }

    /// Decode one response. The entire slice must belong to the response.
    pub fn decode(buffer: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(buffer);
        let version = reader.get_u16().map_err(DecodeError::from)?;
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::version());
        }
        let status = reader.get_u16().map_err(DecodeError::from)?;
        let tag = reader.get_u16().map_err(DecodeError::from)?;
        let response = match tag {
            TAG_OK => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Ok
            }
            TAG_COUNT => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Count {
                    count: reader.get_u16().map_err(DecodeError::from)?,
                }
            }
            TAG_IFACE => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Iface(IfaceInfo::read(&mut reader)?)
            }
            TAG_ROUTE => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Route(Route::read(&mut reader)?)
            }
            TAG_ARP => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Arp(ArpEntry::read(&mut reader)?)
            }
            TAG_ERR => {
                if status == 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Err(Errno(status))
            }
            _ => return Err(DecodeError::tag()),
        };
        if !reader.is_empty() {
            return Err(DecodeError::trailing());
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(request: IfaceRequest) -> Vec<u8> {
        let mut buffer = [0u8; 128];
        let length = request.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    fn encode_response(response: IfaceResponse) -> Vec<u8> {
        let mut buffer = [0u8; 128];
        let length = response.encode(&mut buffer).unwrap();
        buffer[..length].to_vec()
    }

    #[test]
    fn list_and_arp_list_golden_bytes() {
        assert_eq!(encode(IfaceRequest::List), [0x01, 0x00, 0x01, 0x00]);
        assert_eq!(encode(IfaceRequest::ArpList), [0x01, 0x00, 0x07, 0x00]);
    }

    #[test]
    fn get_golden_bytes() {
        assert_eq!(
            encode(IfaceRequest::Get {
                name: IfName::new("eth0").unwrap()
            }),
            [0x01, 0x00, 0x02, 0x00, 0x04, b'e', b't', b'h', b'0']
        );
    }

    #[test]
    fn set_address_golden_bytes() {
        assert_eq!(
            encode(IfaceRequest::SetAddress {
                name: IfName::new("lo").unwrap(),
                addr: [127, 0, 0, 1],
                prefix: 8
            }),
            [0x01, 0x00, 0x03, 0x00, 0x02, b'l', b'o', 127, 0, 0, 1, 8]
        );
    }

    #[test]
    fn route_golden_bytes() {
        assert_eq!(
            encode(IfaceRequest::RouteAdd {
                route: Route {
                    dest: [0, 0, 0, 0],
                    prefix: 0,
                    gateway: None
                }
            }),
            [0x01, 0x00, 0x05, 0x00, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn requests_roundtrip() {
        let name = IfName::new("virtio0").unwrap();
        for request in [
            IfaceRequest::List,
            IfaceRequest::Get { name },
            IfaceRequest::SetAddress {
                name,
                addr: [10, 0, 0, 1],
                prefix: 24,
            },
            IfaceRequest::SetState { name, up: false },
            IfaceRequest::RouteAdd {
                route: Route {
                    dest: [10, 0, 0, 0],
                    prefix: 8,
                    gateway: Some([10, 0, 0, 254]),
                },
            },
            IfaceRequest::RouteDel {
                dest: [10, 0, 0, 0],
                prefix: 8,
            },
            IfaceRequest::ArpList,
        ] {
            let bytes = encode(request);
            assert_eq!(IfaceRequest::decode(&bytes), Ok(request), "{request:?}");
        }
    }

    #[test]
    fn interface_response_golden_bytes() {
        let info = IfaceInfo {
            name: IfName::new("lo").unwrap(),
            addr: [127, 0, 0, 1],
            prefix: 8,
            flags: IF_UP | IF_LINK_UP | IF_LOOPBACK,
            mtu: 65_535,
            mac: [0, 0, 0, 0, 0, 0],
        };
        assert_eq!(
            encode_response(IfaceResponse::Iface(info)),
            [
                0x01, 0x00, 0x00, 0x00, 0x02, 0x00, // header
                0x02, b'l', b'o', // name
                127, 0, 0, 1, // addr
                8, 0x07, 0xff, 0xff, // prefix, flags, mtu
                0, 0, 0, 0, 0, 0, // mac
            ]
        );
    }

    #[test]
    fn responses_roundtrip() {
        let name = IfName::new("eth0").unwrap();
        for response in [
            IfaceResponse::Ok,
            IfaceResponse::Count { count: 3 },
            IfaceResponse::Iface(IfaceInfo {
                name,
                addr: [10, 0, 0, 1],
                prefix: 24,
                flags: IF_UP | IF_LINK_UP,
                mtu: 1500,
                mac: [2, 0, 0, 0, 0, 1],
            }),
            IfaceResponse::Route(Route {
                dest: [0, 0, 0, 0],
                prefix: 0,
                gateway: Some([10, 0, 0, 1]),
            }),
            IfaceResponse::Arp(ArpEntry {
                ip: [10, 0, 0, 2],
                mac: [2, 0, 0, 0, 0, 2],
                flags: ARP_COMPLETE,
            }),
            IfaceResponse::Err(Errno::ENODEV),
        ] {
            let bytes = encode_response(response);
            assert_eq!(IfaceResponse::decode(&bytes), Ok(response), "{response:?}");
        }
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(IfName::new("").is_none());
        assert!(IfName::new("this-name-is-too-long").is_none());
        assert!(IfName::new("eth 0").is_none());
        assert!(IfName::new("eth/0").is_none());
        assert_eq!(IfName::new("eth0").unwrap().as_str(), "eth0");
        assert!(IfName::new("a.b-c_9").is_some());
    }

    #[test]
    fn invalid_prefixes_are_rejected() {
        let name = IfName::new("eth0").unwrap();
        let mut buffer = [0u8; 64];
        assert_eq!(
            IfaceRequest::SetAddress {
                name,
                addr: [1, 2, 3, 4],
                prefix: 33
            }
            .encode(&mut buffer),
            Err(WireError::Invalid)
        );
    }

    #[test]
    fn truncation_and_trailing_bytes_are_rejected() {
        let bytes = encode(IfaceRequest::SetAddress {
            name: IfName::new("eth0").unwrap(),
            addr: [10, 0, 0, 2],
            prefix: 24,
        });
        for length in 0..bytes.len() {
            assert!(IfaceRequest::decode(&bytes[..length]).is_err(), "{length}");
        }
        let mut bytes = encode(IfaceRequest::List);
        bytes.push(0);
        assert_eq!(IfaceRequest::decode(&bytes), Err(DecodeError::trailing()));
    }

    #[test]
    fn error_status_must_be_nonzero() {
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x05, 0x00];
        assert_eq!(IfaceResponse::decode(&bytes), Err(DecodeError::malformed()));
    }
}
