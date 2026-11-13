// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interface, route and ARP control for the `net:` control channel.
//!
//! The same framing as the socket protocol applies: a little-endian `u16` version first, then an
//! operation code and a bounded body. Interface names are at most [`MAX_IFNAME`] ASCII bytes;
//! addresses are four octets with a prefix length, never a pointer into the sender's memory.

use alloc::vec::Vec;
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
/// Interface control operation: list routes.
pub const OP_ROUTE_LIST: u16 = 8;
/// Interface control operation: add or replace a route on a named interface.
pub const OP_ROUTE_ADD_ON: u16 = 9;
/// Interface control operation: delete a route on a named interface.
pub const OP_ROUTE_DEL_ON: u16 = 10;
/// Interface control operation: read the live DHCP lease for an interface.
pub const OP_DHCP_LEASE: u16 = 11;
/// Interface control operation: list live sockets.
pub const OP_SOCKET_LIST: u16 = 12;
/// Interface control operation: read interface counters.
pub const OP_COUNTERS: u16 = 13;

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
/// Response tag: route-list record with flags and interface.
const TAG_ROUTE_INFO: u16 = 6;
/// Response tag: DHCP lease record.
const TAG_DHCP_LEASE: u16 = 7;
/// Response tag: live socket record.
const TAG_SOCKET: u16 = 8;
/// Response tag: interface counters.
const TAG_COUNTERS: u16 = 9;

/// Route flag: the route is active.
pub const ROUTE_UP: u8 = 1 << 0;
/// Route flag: a gateway follows.
pub const ROUTE_GATEWAY: u8 = 1 << 1;
/// Route flag: the destination is a single host (`prefix == 32`).
pub const ROUTE_HOST: u8 = 1 << 2;
/// Every defined route-list flag.
pub const ROUTE_FLAG_MASK: u8 = ROUTE_UP | ROUTE_GATEWAY | ROUTE_HOST;
/// Legacy `Route` wire flag: a gateway follows.
const ROUTE_FLAG_GATEWAY: u8 = 0x01;

/// Maximum DNS servers in one DHCP lease record.
pub const MAX_DHCP_DNS: usize = 3;
/// DHCP timer value meaning the native stack does not expose that timer.
pub const DHCP_TIMER_UNKNOWN: u32 = u32::MAX;

/// Socket transport id: TCP.
pub const SOCKET_TCP: u8 = 1;
/// Socket transport id: UDP.
pub const SOCKET_UDP: u8 = 2;
/// Socket transport id: ICMP.
pub const SOCKET_ICMP: u8 = 3;

/// Socket state id: no connection.
pub const SOCKET_STATE_CLOSED: u8 = 0;
/// Socket state id: listening.
pub const SOCKET_STATE_LISTEN: u8 = 1;
/// Socket state id: SYN sent.
pub const SOCKET_STATE_SYN_SENT: u8 = 2;
/// Socket state id: SYN received.
pub const SOCKET_STATE_SYN_RECEIVED: u8 = 3;
/// Socket state id: established.
pub const SOCKET_STATE_ESTABLISHED: u8 = 4;
/// Socket state id: FIN wait 1.
pub const SOCKET_STATE_FIN_WAIT_1: u8 = 5;
/// Socket state id: FIN wait 2.
pub const SOCKET_STATE_FIN_WAIT_2: u8 = 6;
/// Socket state id: close wait.
pub const SOCKET_STATE_CLOSE_WAIT: u8 = 7;
/// Socket state id: closing.
pub const SOCKET_STATE_CLOSING: u8 = 8;
/// Socket state id: last ACK.
pub const SOCKET_STATE_LAST_ACK: u8 = 9;
/// Socket state id: TIME-WAIT.
pub const SOCKET_STATE_TIME_WAIT: u8 = 10;
/// Socket state id: unconnected UDP.
pub const SOCKET_STATE_UDP: u8 = 11;
/// Socket state id: bound ICMP echo.
pub const SOCKET_STATE_ICMP: u8 = 12;

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

/// One route-list record with the interface and derived flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteInfo {
    /// Destination prefix.
    pub dest: [u8; 4],
    /// Destination prefix length in bits, `0..=32`.
    pub prefix: u8,
    /// [`ROUTE_UP`], [`ROUTE_GATEWAY`] and [`ROUTE_HOST`] bits.
    pub flags: u8,
    /// Interface carrying the route.
    pub iface: IfName,
    /// Next hop, absent for a directly connected prefix.
    pub gateway: Option<[u8; 4]>,
}

impl RouteInfo {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        if self.prefix > 32 || self.flags & !ROUTE_FLAG_MASK != 0 {
            return Err(WireError::Invalid);
        }
        writer.put_bytes(&self.dest)?;
        writer.put_u8(self.prefix)?;
        writer.put_u8(self.flags)?;
        self.iface.write(writer)?;
        if let Some(gateway) = &self.gateway {
            writer.put_bytes(gateway)?;
        }
        Ok(())
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let dest = reader.get_bytes(4).map_err(DecodeError::from)?;
        let prefix = reader.get_u8().map_err(DecodeError::from)?;
        let flags = reader.get_u8().map_err(DecodeError::from)?;
        if prefix > 32 || flags & !ROUTE_FLAG_MASK != 0 {
            return Err(DecodeError::malformed());
        }
        let iface = IfName::read(reader).map_err(DecodeError::from)?;
        let gateway = if flags & ROUTE_GATEWAY != 0 {
            let bytes = reader.get_bytes(4).map_err(DecodeError::from)?;
            Some([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            None
        };
        Ok(Self {
            dest: [dest[0], dest[1], dest[2], dest[3]],
            prefix,
            flags,
            iface,
            gateway,
        })
    }
}

/// One live DHCP lease.
///
/// `renew_ms`, `rebind_ms` and `expires_ms` are relative to the response time.
/// [`DHCP_TIMER_UNKNOWN`] means the native stack in this revision does not
/// expose that timer; the other fields are still live lease data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DhcpLeaseInfo {
    /// Leased address.
    pub addr: [u8; 4],
    /// Prefix length.
    pub prefix: u8,
    /// Default router.
    pub router: Option<[u8; 4]>,
    /// DNS servers, at most [`MAX_DHCP_DNS`].
    pub dns: Vec<[u8; 4]>,
    /// Milliseconds until renewal, or [`DHCP_TIMER_UNKNOWN`].
    pub renew_ms: u32,
    /// Milliseconds until rebinding, or [`DHCP_TIMER_UNKNOWN`].
    pub rebind_ms: u32,
    /// Milliseconds until expiry, or [`DHCP_TIMER_UNKNOWN`].
    pub expires_ms: u32,
}

impl DhcpLeaseInfo {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        if self.prefix > 32 || self.dns.len() > MAX_DHCP_DNS {
            return Err(WireError::Invalid);
        }
        writer.put_bytes(&self.addr)?;
        writer.put_u8(self.prefix)?;
        writer.put_u8(u8::from(self.router.is_some()))?;
        if let Some(router) = &self.router {
            writer.put_bytes(router)?;
        }
        writer.put_u8(u8::try_from(self.dns.len()).map_err(|_| WireError::Invalid)?)?;
        for server in &self.dns {
            writer.put_bytes(server)?;
        }
        writer.put_u32(self.renew_ms)?;
        writer.put_u32(self.rebind_ms)?;
        writer.put_u32(self.expires_ms)?;
        Ok(())
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let addr = reader.get_bytes(4).map_err(DecodeError::from)?;
        let prefix = reader.get_u8().map_err(DecodeError::from)?;
        let has_router = reader.get_u8().map_err(DecodeError::from)?;
        if prefix > 32 || has_router > 1 {
            return Err(DecodeError::malformed());
        }
        let router = if has_router == 1 {
            let bytes = reader.get_bytes(4).map_err(DecodeError::from)?;
            Some([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            None
        };
        let count = reader.get_u8().map_err(DecodeError::from)? as usize;
        if count > MAX_DHCP_DNS {
            return Err(DecodeError::malformed());
        }
        let mut dns = Vec::with_capacity(count);
        for _ in 0..count {
            let bytes = reader.get_bytes(4).map_err(DecodeError::from)?;
            dns.push([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        Ok(Self {
            addr: [addr[0], addr[1], addr[2], addr[3]],
            prefix,
            router,
            dns,
            renew_ms: reader.get_u32().map_err(DecodeError::from)?,
            rebind_ms: reader.get_u32().map_err(DecodeError::from)?,
            expires_ms: reader.get_u32().map_err(DecodeError::from)?,
        })
    }
}

/// One live socket record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocketEntry {
    /// Core socket id.
    pub id: u32,
    /// [`SOCKET_TCP`], [`SOCKET_UDP`] or [`SOCKET_ICMP`].
    pub transport: u8,
    /// One of the `SOCKET_STATE_*` ids.
    pub state: u8,
    /// Interface carrying the socket.
    pub iface: IfName,
    /// Local IPv4 address (zero when not bound).
    pub local: [u8; 4],
    /// Local port (zero for ICMP).
    pub local_port: u16,
    /// Peer IPv4 address, when connected.
    pub remote: Option<[u8; 4]>,
    /// Peer port, zero when absent.
    pub remote_port: u16,
    /// Bytes queued for the application.
    pub rx_queue: u32,
    /// Bytes queued for the wire.
    pub tx_queue: u32,
}

impl SocketEntry {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        if !matches!(self.transport, SOCKET_TCP | SOCKET_UDP | SOCKET_ICMP)
            || self.state > SOCKET_STATE_ICMP
        {
            return Err(WireError::Invalid);
        }
        writer.put_u32(self.id)?;
        writer.put_u8(self.transport)?;
        writer.put_u8(self.state)?;
        self.iface.write(writer)?;
        writer.put_bytes(&self.local)?;
        writer.put_u16(self.local_port)?;
        writer.put_u8(u8::from(self.remote.is_some()))?;
        if let Some(remote) = &self.remote {
            writer.put_bytes(remote)?;
            writer.put_u16(self.remote_port)?;
        }
        writer.put_u32(self.rx_queue)?;
        writer.put_u32(self.tx_queue)
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let id = reader.get_u32().map_err(DecodeError::from)?;
        let transport = reader.get_u8().map_err(DecodeError::from)?;
        let state = reader.get_u8().map_err(DecodeError::from)?;
        if !matches!(transport, SOCKET_TCP | SOCKET_UDP | SOCKET_ICMP) || state > SOCKET_STATE_ICMP
        {
            return Err(DecodeError::malformed());
        }
        let iface = IfName::read(reader).map_err(DecodeError::from)?;
        let local = reader.get_bytes(4).map_err(DecodeError::from)?;
        let local_port = reader.get_u16().map_err(DecodeError::from)?;
        let has_remote = reader.get_u8().map_err(DecodeError::from)?;
        if has_remote > 1 {
            return Err(DecodeError::malformed());
        }
        let (remote, remote_port) = if has_remote == 1 {
            let bytes = reader.get_bytes(4).map_err(DecodeError::from)?;
            (
                Some([bytes[0], bytes[1], bytes[2], bytes[3]]),
                reader.get_u16().map_err(DecodeError::from)?,
            )
        } else {
            (None, 0)
        };
        Ok(Self {
            id,
            transport,
            state,
            iface,
            local: [local[0], local[1], local[2], local[3]],
            local_port,
            remote,
            remote_port,
            rx_queue: reader.get_u32().map_err(DecodeError::from)?,
            tx_queue: reader.get_u32().map_err(DecodeError::from)?,
        })
    }
}

/// Interface packet and byte counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CounterInfo {
    /// Frames received.
    pub rx_packets: u64,
    /// Frames transmitted.
    pub tx_packets: u64,
    /// Bytes received.
    pub rx_bytes: u64,
    /// Bytes transmitted.
    pub tx_bytes: u64,
    /// Receive errors.
    pub rx_errors: u64,
    /// Transmit errors.
    pub tx_errors: u64,
    /// Received frames dropped by the stack.
    pub rx_drops: u64,
    /// Transmitted frames dropped.
    pub tx_drops: u64,
}

impl CounterInfo {
    fn write(&self, writer: &mut Writer<'_>) -> Result<(), WireError> {
        for value in [
            self.rx_packets,
            self.tx_packets,
            self.rx_bytes,
            self.tx_bytes,
            self.rx_errors,
            self.tx_errors,
            self.rx_drops,
            self.tx_drops,
        ] {
            writer.put_u64(value)?;
        }
        Ok(())
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            rx_packets: reader.get_u64().map_err(DecodeError::from)?,
            tx_packets: reader.get_u64().map_err(DecodeError::from)?,
            rx_bytes: reader.get_u64().map_err(DecodeError::from)?,
            tx_bytes: reader.get_u64().map_err(DecodeError::from)?,
            rx_errors: reader.get_u64().map_err(DecodeError::from)?,
            tx_errors: reader.get_u64().map_err(DecodeError::from)?,
            rx_drops: reader.get_u64().map_err(DecodeError::from)?,
            tx_drops: reader.get_u64().map_err(DecodeError::from)?,
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
    /// List the route table.
    RouteList,
    /// Add or replace a route on a named interface.
    RouteAddOn {
        /// Interface name.
        name: IfName,
        /// Route to add.
        route: Route,
    },
    /// Delete a route from a named interface.
    RouteDelOn {
        /// Interface name.
        name: IfName,
        /// Destination prefix.
        dest: [u8; 4],
        /// Destination prefix length in bits.
        prefix: u8,
    },
    /// Read the live DHCP lease for an interface.
    DhcpLease {
        /// Interface name.
        name: IfName,
    },
    /// List live sockets across all attached interfaces.
    SocketList,
    /// Read interface counters.
    Counters {
        /// Interface name.
        name: IfName,
    },
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
            Self::RouteList => writer.put_u16(OP_ROUTE_LIST)?,
            Self::RouteAddOn { name, route } => {
                writer.put_u16(OP_ROUTE_ADD_ON)?;
                name.write(&mut writer)?;
                route.write(&mut writer)?;
            }
            Self::RouteDelOn { name, dest, prefix } => {
                if *prefix > 32 {
                    return Err(WireError::Invalid);
                }
                writer.put_u16(OP_ROUTE_DEL_ON)?;
                name.write(&mut writer)?;
                writer.put_bytes(dest)?;
                writer.put_u8(*prefix)?;
            }
            Self::DhcpLease { name } => {
                writer.put_u16(OP_DHCP_LEASE)?;
                name.write(&mut writer)?;
            }
            Self::SocketList => writer.put_u16(OP_SOCKET_LIST)?,
            Self::Counters { name } => {
                writer.put_u16(OP_COUNTERS)?;
                name.write(&mut writer)?;
            }
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
            OP_ROUTE_LIST => Self::RouteList,
            OP_ROUTE_ADD_ON => Self::RouteAddOn {
                name: IfName::read(&mut reader).map_err(DecodeError::from)?,
                route: Route::read(&mut reader)?,
            },
            OP_ROUTE_DEL_ON => {
                let name = IfName::read(&mut reader).map_err(DecodeError::from)?;
                let dest = reader.get_bytes(4).map_err(DecodeError::from)?;
                let prefix = reader.get_u8().map_err(DecodeError::from)?;
                if prefix > 32 {
                    return Err(DecodeError::malformed());
                }
                Self::RouteDelOn {
                    name,
                    dest: [dest[0], dest[1], dest[2], dest[3]],
                    prefix,
                }
            }
            OP_DHCP_LEASE => Self::DhcpLease {
                name: IfName::read(&mut reader).map_err(DecodeError::from)?,
            },
            OP_SOCKET_LIST => Self::SocketList,
            OP_COUNTERS => Self::Counters {
                name: IfName::read(&mut reader).map_err(DecodeError::from)?,
            },
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
/// `List`, `RouteList`, `SocketList` and `ArpList` produce one [`IfaceResponse::Count`] followed by
/// that many [`IfaceResponse::Iface`], [`IfaceResponse::Route`]/[`IfaceResponse::RouteInfo`],
/// [`IfaceResponse::Socket`] or [`IfaceResponse::Arp`] records. The request/response layer above
/// uses the count to stop reading.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// One route-list record.
    RouteInfo(RouteInfo),
    /// One live DHCP lease.
    DhcpLease(DhcpLeaseInfo),
    /// One live socket record.
    Socket(SocketEntry),
    /// Interface counters.
    Counters(CounterInfo),
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
            Self::RouteInfo(route) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_ROUTE_INFO)?;
                route.write(&mut writer)?;
            }
            Self::DhcpLease(lease) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_DHCP_LEASE)?;
                lease.write(&mut writer)?;
            }
            Self::Socket(entry) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_SOCKET)?;
                entry.write(&mut writer)?;
            }
            Self::Counters(counters) => {
                writer.put_u16(0)?;
                writer.put_u16(TAG_COUNTERS)?;
                counters.write(&mut writer)?;
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
            TAG_ROUTE_INFO => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::RouteInfo(RouteInfo::read(&mut reader)?)
            }
            TAG_DHCP_LEASE => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::DhcpLease(DhcpLeaseInfo::read(&mut reader)?)
            }
            TAG_SOCKET => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Socket(SocketEntry::read(&mut reader)?)
            }
            TAG_COUNTERS => {
                if status != 0 {
                    return Err(DecodeError::malformed());
                }
                Self::Counters(CounterInfo::read(&mut reader)?)
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
#[path = "iface_ut.rs"]
mod tests;
