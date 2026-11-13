// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable `net:` server state: socket table, file table, typed requests and interface
//! control. The target main wraps this in the K30 scheme server.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_netcore::{
    CoreSocket, Device, DhcpLease, Event, InterfaceCounters, Ipv4Addr, NetCore, RouteEntry,
    SocketId, SocketState, SocketTransport,
};
use raven_proto_fs::{S_IFSOCK, Stat};
use raven_proto_net::{
    CounterInfo, DHCP_TIMER_UNKNOWN, DhcpLeaseInfo, IF_LINK_UP, IF_LOOPBACK, IF_UP, IfName,
    IfaceInfo, IfaceRequest, IfaceResponse, MAX_ICMP_PAYLOAD, ROUTE_GATEWAY, ROUTE_HOST, ROUTE_UP,
    RouteInfo, SOCKET_ICMP, SOCKET_STATE_CLOSE_WAIT, SOCKET_STATE_CLOSED, SOCKET_STATE_CLOSING,
    SOCKET_STATE_ESTABLISHED, SOCKET_STATE_FIN_WAIT_1, SOCKET_STATE_FIN_WAIT_2, SOCKET_STATE_ICMP,
    SOCKET_STATE_LAST_ACK, SOCKET_STATE_LISTEN, SOCKET_STATE_SYN_RECEIVED, SOCKET_STATE_SYN_SENT,
    SOCKET_STATE_TIME_WAIT, SOCKET_STATE_UDP, SOCKET_TCP, SOCKET_UDP, SocketEntry, Transport, path,
    socket::{Call, Endpoint, Response, SockOpt, SocketRequest},
};

/// Mode reported by `FSTAT` for a socket file.
pub const SOCKET_FILE: u32 = S_IFSOCK | 0o666;

const LOOPBACK_NAME: &str = "lo";
const LOOPBACK_MTU: u16 = 65_535;
const NIC_NAME: &str = "eth0";
/// NIC frame rings are drained at least this often even when no stack timer is armed.
const NIC_POLL_MS: u64 = 10;
const MAX_OPEN_FILES: usize = 256;
const MAX_PENDING_ACCEPTS: usize = 64;

/// Link a file belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interface {
    /// The in-process loopback device.
    Loopback,
    /// The persistent `nic:` frame service.
    Nic,
}

/// One readiness transition with the interface that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetEvent {
    /// Interface whose core produced `event`.
    pub interface: Interface,
    /// Readiness transition.
    pub event: Event,
}

/// Static NIC configuration supplied by the target glue after `nic:info`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NicConfig {
    /// Device MAC address.
    pub mac: [u8; 6],
    /// Device MTU, excluding the Ethernet header.
    pub mtu: u16,
    /// IPv4 address configured on the NIC.
    pub addr: [u8; 4],
    /// Prefix length.
    pub prefix: u8,
    /// Default gateway.
    pub gateway: Option<[u8; 4]>,
    /// Start the stack's DHCP client on this interface.
    pub dhcp: bool,
}

trait SocketCore {
    fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>);
    fn dhcp_lease(&self) -> Option<DhcpLease>;
    fn routes(&self) -> Vec<RouteEntry>;
    fn add_route(&mut self, route: RouteEntry) -> Result<(), Errno>;
    fn delete_route(&mut self, dest: Ipv4Addr, prefix_len: u8) -> Result<(), Errno>;
    fn socket_table(&self) -> Vec<CoreSocket>;
    fn interface_counters(&self) -> Option<InterfaceCounters>;
    fn icmp_bind(&mut self, id: u16) -> Result<SocketId, Errno>;
    fn icmp_echo(
        &mut self,
        s: SocketId,
        to: Ipv4Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno>;
    fn icmp_recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno>;
    fn poll(&mut self, now_ms: i64) -> Vec<Event>;
    fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64>;
    fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno>;
    fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno>;
    fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno>;
    fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno>;
    fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno>;
    fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno>;
    fn close(&mut self, s: SocketId) -> Result<(), Errno>;
    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno>;
    fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno>;
    fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno>;
    fn local_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)>;
    fn peer_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)>;
    fn set_socket_option(&mut self, s: SocketId, option: u32, value: i32) -> Result<(), Errno>;
    fn get_socket_option(&self, s: SocketId, option: u32) -> Result<i32, Errno>;
}

impl<D: Device> SocketCore for NetCore<D> {
    fn set_ipv4(&mut self, addr: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        NetCore::set_ipv4(self, addr, prefix, gateway);
    }

    fn dhcp_lease(&self) -> Option<DhcpLease> {
        NetCore::dhcp_lease(self)
    }

    fn routes(&self) -> Vec<RouteEntry> {
        NetCore::routes(self)
    }

    fn add_route(&mut self, route: RouteEntry) -> Result<(), Errno> {
        NetCore::add_route(self, route)
    }

    fn delete_route(&mut self, dest: Ipv4Addr, prefix_len: u8) -> Result<(), Errno> {
        NetCore::delete_route(self, dest, prefix_len)
    }

    fn socket_table(&self) -> Vec<CoreSocket> {
        NetCore::socket_table(self)
    }

    fn interface_counters(&self) -> Option<InterfaceCounters> {
        NetCore::interface_counters(self)
    }

    fn icmp_bind(&mut self, id: u16) -> Result<SocketId, Errno> {
        NetCore::icmp_bind(self, id)
    }

    fn icmp_echo(
        &mut self,
        s: SocketId,
        to: Ipv4Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno> {
        NetCore::icmp_echo(self, s, to, id, seq, payload)
    }

    fn icmp_recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        NetCore::icmp_recv(self, s, buf)
    }

    fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        NetCore::poll(self, now_ms)
    }

    fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        NetCore::next_poll_delay_ms(self, now_ms)
    }

    fn tcp_listen(&mut self, port: u16, backlog: usize) -> Result<SocketId, Errno> {
        NetCore::tcp_listen(self, port, backlog)
    }

    fn tcp_accept(&mut self, listener: SocketId) -> Result<Option<SocketId>, Errno> {
        NetCore::tcp_accept(self, listener)
    }

    fn tcp_connect(
        &mut self,
        remote: (Ipv4Addr, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        NetCore::tcp_connect(self, remote, local_port)
    }

    fn send(&mut self, s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        NetCore::send(self, s, data)
    }

    fn recv(&mut self, s: SocketId, buf: &mut [u8]) -> Result<usize, Errno> {
        NetCore::recv(self, s, buf)
    }

    fn shutdown_write(&mut self, s: SocketId) -> Result<(), Errno> {
        NetCore::shutdown_write(self, s)
    }

    fn close(&mut self, s: SocketId) -> Result<(), Errno> {
        NetCore::close(self, s)
    }

    fn udp_bind(&mut self, port: u16) -> Result<SocketId, Errno> {
        NetCore::udp_bind(self, port)
    }

    fn udp_send_to(
        &mut self,
        s: SocketId,
        data: &[u8],
        to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        NetCore::udp_send_to(self, s, data, to)
    }

    fn udp_recv_from(
        &mut self,
        s: SocketId,
        buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        NetCore::udp_recv_from(self, s, buf)
    }

    fn local_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        NetCore::local_endpoint(self, s)
    }

    fn peer_endpoint(&self, s: SocketId) -> Option<(Ipv4Addr, u16)> {
        NetCore::peer_endpoint(self, s)
    }

    fn set_socket_option(&mut self, s: SocketId, option: u32, value: i32) -> Result<(), Errno> {
        NetCore::set_socket_option(self, s, option, value)
    }

    fn get_socket_option(&self, s: SocketId, option: u32) -> Result<i32, Errno> {
        NetCore::get_socket_option(self, s, option)
    }
}

/// One accepted connection waiting for its client-side file open.
#[derive(Clone, Copy)]
struct Accepted {
    socket: SocketId,
    local: Endpoint,
    remote: Option<Endpoint>,
    transport: Transport,
    interface: Interface,
}

/// One open scheme file.
enum File {
    Control,
    Socket(SocketFile),
}

struct PendingIcmp {
    target: Endpoint,
    sent_ms: i64,
    deadline_ms: i64,
}

struct IcmpState {
    id: Option<u16>,
    pending: BTreeMap<u16, PendingIcmp>,
}

struct SocketFile {
    transport: Transport,
    interface: Interface,
    socket: Option<SocketId>,
    listener: bool,
    path_endpoint: Endpoint,
    local: Endpoint,
    remote: Option<Endpoint>,
    peer: Option<Endpoint>,
    bound: bool,
    read_shutdown: bool,
    write_shutdown: bool,
    reuse_addr: bool,
    rcvbuf: i32,
    sndbuf: i32,
    nodelay: bool,
    icmp: Option<IcmpState>,
}

impl SocketFile {
    fn new(transport: Transport, interface: Interface, path_endpoint: Endpoint) -> Self {
        let icmp = transport == Transport::Icmp;
        Self {
            transport,
            interface,
            socket: None,
            listener: false,
            path_endpoint,
            local: if icmp {
                Endpoint::default()
            } else {
                path_endpoint
            },
            remote: icmp.then_some(path_endpoint),
            peer: None,
            bound: false,
            read_shutdown: false,
            write_shutdown: false,
            reuse_addr: false,
            rcvbuf: 64 * 1024,
            sndbuf: 64 * 1024,
            nodelay: false,
            icmp: icmp.then(|| IcmpState {
                id: None,
                pending: BTreeMap::new(),
            }),
        }
    }

    fn accepted(accepted: Accepted) -> Self {
        Self {
            transport: accepted.transport,
            interface: accepted.interface,
            socket: Some(accepted.socket),
            listener: false,
            path_endpoint: accepted.local,
            local: accepted.local,
            remote: accepted.remote,
            peer: None,
            bound: true,
            read_shutdown: false,
            write_shutdown: false,
            reuse_addr: false,
            rcvbuf: 64 * 1024,
            sndbuf: 64 * 1024,
            nodelay: false,
            icmp: None,
        }
    }

    fn port_for_listen(&self) -> u16 {
        if self.bound {
            self.local.port
        } else {
            self.path_endpoint.port
        }
    }
}

struct Control {
    addr: [u8; 4],
    prefix: u8,
    up: bool,
}

struct NicControl {
    addr: [u8; 4],
    prefix: u8,
    gateway: Option<[u8; 4]>,
    up: bool,
}

/// The host-testable `net:` server.
pub struct Netd<D: Device> {
    core: NetCore<D>,
    nic: Option<Box<dyn SocketCore>>,
    nic_config: Option<NicConfig>,
    nic_control: Option<NicControl>,
    files: BTreeMap<u64, File>,
    next_file: u64,
    accepted: BTreeMap<u32, Accepted>,
    next_accept: u32,
    control: Control,
    now_ms: i64,
}

impl<D: Device> Netd<D> {
    /// Create the server over `device` and configure IPv4 loopback.
    #[must_use]
    pub fn new(device: D, mac: [u8; 6], now_ms: i64) -> Self {
        let mut core = NetCore::new(device, mac, now_ms);
        core.set_ipv4(Ipv4Addr::new(127, 0, 0, 1), 8, None);
        Self {
            core,
            nic: None,
            nic_config: None,
            nic_control: None,
            files: BTreeMap::new(),
            next_file: 1,
            accepted: BTreeMap::new(),
            next_accept: 1,
            control: Control {
                addr: [127, 0, 0, 1],
                prefix: 8,
                up: true,
            },
            now_ms,
        }
    }

    /// Attach the persistent NIC as a second socket core.
    pub fn attach_nic<N: Device + 'static>(&mut self, device: N, config: NicConfig, now_ms: i64) {
        let mut core = NetCore::new(device, config.mac, now_ms);
        core.set_ipv4(
            Ipv4Addr::from_octets(config.addr),
            config.prefix,
            config.gateway.map(Ipv4Addr::from_octets),
        );
        if config.dhcp {
            let _ = core.try_enable_dhcp();
        }
        self.nic = Some(Box::new(core));
        self.nic_config = Some(config);
        self.nic_control = Some(NicControl {
            addr: config.addr,
            prefix: config.prefix,
            gateway: config.gateway,
            up: true,
        });
    }

    /// Whether a NIC core is attached.
    #[must_use]
    pub fn has_nic(&self) -> bool {
        self.nic.is_some()
    }

    /// Static NIC configuration, when attached.
    #[must_use]
    pub fn nic_config(&self) -> Option<NicConfig> {
        self.nic_config
    }

    /// Drive both stacks and return readiness transitions.
    pub fn poll(&mut self, now_ms: i64) -> Vec<NetEvent> {
        self.now_ms = now_ms;
        let mut events: Vec<NetEvent> = self
            .core
            .poll(now_ms)
            .into_iter()
            .map(|event| NetEvent {
                interface: Interface::Loopback,
                event,
            })
            .collect();
        if let Some(nic) = self.nic.as_mut() {
            events.extend(nic.poll(now_ms).into_iter().map(|event| NetEvent {
                interface: Interface::Nic,
                event,
            }));
        }
        events
    }

    /// Time until the next stack poll, in milliseconds.
    pub fn next_poll_delay_ms(&mut self, now_ms: i64) -> Option<u64> {
        let mut delay = self.core.next_poll_delay_ms(now_ms);
        if let Some(nic) = self.nic.as_mut() {
            let nic_delay = nic.next_poll_delay_ms(now_ms);
            delay = match (delay, nic_delay) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            };
            delay = Some(delay.unwrap_or(NIC_POLL_MS).min(NIC_POLL_MS));
        }
        delay
    }

    fn core_mut(&mut self, interface: Interface) -> &mut dyn SocketCore {
        match interface {
            Interface::Loopback => &mut self.core,
            Interface::Nic => self
                .nic
                .as_mut()
                .expect("NIC interface requires an attached NIC core")
                .as_mut(),
        }
    }

    fn core_ref(&self, interface: Interface) -> &dyn SocketCore {
        match interface {
            Interface::Loopback => &self.core,
            Interface::Nic => self
                .nic
                .as_ref()
                .expect("NIC interface requires an attached NIC core")
                .as_ref(),
        }
    }

    /// Number of open socket files (control files are excluded).
    #[must_use]
    pub fn socket_files(&self) -> usize {
        self.files
            .values()
            .filter(|file| matches!(file, File::Socket(_)))
            .count()
    }

    /// Number of accepted connections waiting for an `net:accept/<id>` open.
    #[must_use]
    pub fn pending_accepts(&self) -> usize {
        self.accepted.len()
    }

    /// Socket id behind one open file, if it is a socket file with a live socket.
    #[must_use]
    pub fn socket_id(&self, file: u64) -> Option<SocketId> {
        match self.files.get(&file)? {
            File::Control => None,
            File::Socket(socket) => socket.socket,
        }
    }

    /// Interface behind one open socket file.
    #[must_use]
    pub fn socket_interface(&self, file: u64) -> Option<Interface> {
        match self.files.get(&file)? {
            File::Control => None,
            File::Socket(socket) => Some(socket.interface),
        }
    }

    /// Open files currently attached to `socket` on `interface`.
    #[must_use]
    pub fn files_for_socket(&self, interface: Interface, socket: SocketId) -> Vec<u64> {
        self.files
            .iter()
            .filter_map(|(file, state)| match state {
                File::Socket(state)
                    if state.socket == Some(socket) && state.interface == interface =>
                {
                    Some(*file)
                }
                _ => None,
            })
            .collect()
    }

    /// Open `path` and return its server file id.
    pub fn open(&mut self, path: &str) -> Result<u64, Errno> {
        if self.files.len() >= MAX_OPEN_FILES {
            return Err(Errno::EMFILE);
        }
        let file = if path == "net:" {
            File::Control
        } else if let Some(id) = path::parse_accepted(path) {
            let accepted = self.accepted.remove(&id).ok_or(Errno::ENOENT)?;
            File::Socket(SocketFile::accepted(accepted))
        } else if path.starts_with(path::ACCEPT_PREFIX) {
            return Err(Errno::EINVAL);
        } else {
            let parsed = path::parse(path).map_err(|_| Errno::EINVAL)?;
            let endpoint = parsed.endpoint.ok_or(Errno::EINVAL)?;
            let interface = if self.nic.is_some() && endpoint.ip[0] != 127 {
                Interface::Nic
            } else {
                Interface::Loopback
            };
            File::Socket(SocketFile::new(parsed.transport, interface, endpoint))
        };
        let id = self.next_file;
        self.next_file = self.next_file.checked_add(1).ok_or(Errno::ENFILE)?;
        self.files.insert(id, file);
        Ok(id)
    }

    /// Read stream bytes or one datagram from `file`.
    pub fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        let (interface, transport, id, read_shutdown) = match self.files.get(&file) {
            Some(File::Socket(socket)) => (
                socket.interface,
                socket.transport,
                socket.socket,
                socket.read_shutdown,
            ),
            Some(File::Control) => return Err(Errno::EINVAL),
            None => return Err(Errno::EBADF),
        };
        if read_shutdown {
            return Ok(0);
        }
        let id = id.ok_or(Errno::ENOTCONN)?;
        match transport {
            Transport::Tcp => self.core_mut(interface).recv(id, buffer),
            Transport::Udp => self
                .core_mut(interface)
                .udp_recv_from(id, buffer)
                .map(|(len, _)| len),
            Transport::Icmp | Transport::Control => Err(Errno::EINVAL),
        }
    }

    /// Write stream bytes or one datagram to `file`.
    pub fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
        let (interface, transport, id, peer, write_shutdown) = match self.files.get(&file) {
            Some(File::Socket(socket)) => (
                socket.interface,
                socket.transport,
                socket.socket,
                socket.peer,
                socket.write_shutdown,
            ),
            Some(File::Control) => return Err(Errno::EINVAL),
            None => return Err(Errno::EBADF),
        };
        if write_shutdown {
            return Err(Errno::EPIPE);
        }
        let id = id.ok_or(Errno::ENOTCONN)?;
        match transport {
            Transport::Tcp => self.core_mut(interface).send(id, buffer),
            Transport::Udp => {
                let to = peer.ok_or(Errno::EADDRNOTAVAIL)?;
                self.core_mut(interface).udp_send_to(
                    id,
                    buffer,
                    (Ipv4Addr::from_octets(to.ip), to.port),
                )
            }
            Transport::Icmp | Transport::Control => Err(Errno::EINVAL),
        }
    }

    /// Handle one typed `CALL` request on `file`.
    ///
    /// Socket requests use [`SocketRequest`]; the control file uses [`IfaceRequest`]. Operation
    /// failures are encoded as [`Response::Err`] / [`IfaceResponse::Err`] in `response`; a malformed
    /// request is the scheme errno `EINVAL`.
    pub fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        let kind = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => 0u8,
            File::Socket(_) => 1u8,
        };
        if kind == 0 {
            return self.control_call(request, response);
        }
        let request = SocketRequest::decode(request).map_err(|_| Errno::EINVAL)?;
        match request {
            SocketRequest::Open { .. } => self.write_socket_error(response, Errno::EINVAL),
            SocketRequest::Read { len } => self.typed_read(file, len, response),
            SocketRequest::Write { data, to } => {
                self.typed_write(file, data, to.as_ref().copied(), response)
            }
            SocketRequest::Call(call) => self.socket_call(file, call, response),
            SocketRequest::IcmpEcho {
                id,
                seq,
                timeout_ms,
                data,
            } => self.icmp_echo(file, id, seq, timeout_ms, data, response),
        }
    }

    /// Close `file` and its socket.
    pub fn close(&mut self, file: u64) -> Result<(), Errno> {
        let file = self.files.remove(&file).ok_or(Errno::EBADF)?;
        if let File::Socket(socket) = file
            && let Some(id) = socket.socket
        {
            let _ = self.core_mut(socket.interface).close(id);
        }
        Ok(())
    }

    /// Duplicate a file description; the kernel owns the handle duplication.
    pub fn dup(&mut self, file: u64) -> Result<u64, Errno> {
        self.files.get(&file).ok_or(Errno::EBADF)?;
        Ok(file)
    }

    /// `FSTAT` record for a socket or control file.
    pub fn fstat(&mut self, file: u64) -> Result<Stat, Errno> {
        self.files.get(&file).ok_or(Errno::EBADF)?;
        Ok(Stat {
            ino: file,
            size: 0,
            blocks: 0,
            mode: SOCKET_FILE,
            nlink: 1,
            ..Stat::default()
        })
    }

    fn icmp_echo(
        &mut self,
        file: u64,
        id: u16,
        seq: u16,
        timeout_ms: u32,
        data: &[u8],
        response: &mut [u8],
    ) -> Result<usize, Errno> {
        if data.len() > MAX_ICMP_PAYLOAD {
            return self.write_socket_error(response, Errno::EINVAL);
        }
        let (interface, transport, target, socket, bound, seq_pending) =
            match self.files.get(&file).ok_or(Errno::EBADF)? {
                File::Control => return Err(Errno::EINVAL),
                File::Socket(socket) => (
                    socket.interface,
                    socket.transport,
                    socket.path_endpoint,
                    socket.socket,
                    socket.icmp.as_ref().and_then(|icmp| icmp.id),
                    socket
                        .icmp
                        .as_ref()
                        .is_some_and(|icmp| icmp.pending.contains_key(&seq)),
                ),
            };
        if transport != Transport::Icmp {
            return self.write_socket_error(response, Errno::EINVAL);
        }
        if bound.is_some_and(|bound| bound != id) {
            return self.write_socket_error(response, Errno::EINVAL);
        }
        if seq_pending {
            return self.write_socket_error(response, Errno::EBUSY);
        }
        let socket = match socket {
            Some(socket) => socket,
            None => {
                let socket = self.core_mut(interface).icmp_bind(id)?;
                let Some(File::Socket(state)) = self.files.get_mut(&file) else {
                    return Err(Errno::EBADF);
                };
                state.socket = Some(socket);
                if let Some(icmp) = state.icmp.as_mut() {
                    icmp.id = Some(id);
                }
                socket
            }
        };
        let mut payload = [0u8; MAX_ICMP_PAYLOAD + 4];
        payload[0..2].copy_from_slice(&[0x52, 0x42]);
        payload[2..4].copy_from_slice(&seq.to_be_bytes());
        payload[4..4 + data.len()].copy_from_slice(data);
        let length = 4 + data.len();
        self.core_mut(interface).icmp_echo(
            socket,
            Ipv4Addr::from_octets(target.ip),
            id,
            seq,
            &payload[..length],
        )?;
        let Some(File::Socket(state)) = self.files.get_mut(&file) else {
            return Err(Errno::EBADF);
        };
        if let Some(icmp) = state.icmp.as_mut() {
            icmp.pending.insert(
                seq,
                PendingIcmp {
                    target,
                    sent_ms: self.now_ms,
                    deadline_ms: self.now_ms.saturating_add(i64::from(timeout_ms)),
                },
            );
        }
        self.write_response(response, &Response::Ok)
    }

    fn typed_read_icmp(
        &mut self,
        file: u64,
        len: u32,
        response: &mut [u8],
    ) -> Result<usize, Errno> {
        const HEADER_LEN: usize = 22;
        if len as usize > MAX_ICMP_PAYLOAD {
            return self.write_socket_error(response, Errno::EINVAL);
        }
        let (interface, socket, deadline) = {
            let Some(File::Socket(state)) = self.files.get(&file) else {
                return Err(Errno::EBADF);
            };
            if state.transport != Transport::Icmp {
                return Err(Errno::EINVAL);
            }
            (
                state.interface,
                state.socket.ok_or(Errno::ENOTCONN)?,
                state.icmp.as_ref().and_then(|icmp| {
                    icmp.pending
                        .values()
                        .map(|pending| pending.deadline_ms)
                        .min()
                }),
            )
        };
        let mut raw = [0u8; MAX_ICMP_PAYLOAD + 4];
        match self.core_mut(interface).icmp_recv(socket, &mut raw) {
            Ok(length) if length >= 4 && raw[0..2] == [0x52, 0x42] => {
                let seq = u16::from_be_bytes([raw[2], raw[3]]);
                let payload_len = length - 4;
                if payload_len > len as usize {
                    return self.write_socket_error(response, Errno::E2BIG);
                }
                let (source, sent_ms, id) = {
                    let Some(File::Socket(state)) = self.files.get_mut(&file) else {
                        return Err(Errno::EBADF);
                    };
                    let Some(icmp) = state.icmp.as_mut() else {
                        return Err(Errno::EINVAL);
                    };
                    let Some(pending) = icmp.pending.remove(&seq) else {
                        return self.write_socket_error(response, Errno::EAGAIN);
                    };
                    (pending.target, pending.sent_ms, icmp.id.unwrap_or(0))
                };
                let rtt_ms = u32::try_from(self.now_ms.saturating_sub(sent_ms)).unwrap_or(u32::MAX);
                let header = Response::Icmp {
                    source,
                    id,
                    seq,
                    rtt_ms,
                    len: payload_len as u16,
                };
                header
                    .encode(response.get_mut(..HEADER_LEN).ok_or(Errno::E2BIG)?)
                    .map_err(|_| Errno::E2BIG)?;
                response
                    .get_mut(HEADER_LEN..HEADER_LEN + payload_len)
                    .ok_or(Errno::E2BIG)?
                    .copy_from_slice(&raw[4..length]);
                Ok(HEADER_LEN + payload_len)
            }
            Ok(_) => self.write_socket_error(response, Errno::EAGAIN),
            Err(Errno::EAGAIN) => {
                let now = self.now_ms;
                let Some(File::Socket(state)) = self.files.get_mut(&file) else {
                    return Err(Errno::EBADF);
                };
                if let Some(icmp) = state.icmp.as_mut()
                    && deadline.is_some_and(|deadline| now >= deadline)
                {
                    icmp.pending.retain(|_, pending| pending.deadline_ms > now);
                    return self.write_socket_error(response, Errno::ETIMEDOUT);
                }
                self.write_socket_error(response, Errno::EAGAIN)
            }
            Err(error) => self.write_socket_error(response, error),
        }
    }

    fn typed_read(&mut self, file: u64, len: u32, response: &mut [u8]) -> Result<usize, Errno> {
        if len as usize > raven_proto_net::MAX_DATAGRAM {
            return self.write_socket_error(response, Errno::EINVAL);
        }
        let (interface, transport, socket, read_shutdown) = match self.files.get(&file) {
            Some(File::Socket(socket)) => (
                socket.interface,
                socket.transport,
                socket.socket,
                socket.read_shutdown,
            ),
            Some(File::Control) => return Err(Errno::EINVAL),
            None => return Err(Errno::EBADF),
        };
        if read_shutdown {
            let header = Response::Read {
                len: 0,
                from: (transport == Transport::Udp).then_some(Endpoint::default()),
            };
            return self.write_response(response, &header);
        }
        if transport == Transport::Icmp {
            return self.typed_read_icmp(file, len, response);
        }
        let header_len: usize = if transport == Transport::Udp { 17 } else { 11 };
        let wanted = len as usize;
        let payload_end = header_len.checked_add(wanted).ok_or(Errno::E2BIG)?;
        if response.len() < payload_end {
            return self.write_socket_error(response, Errno::E2BIG);
        }
        let id = socket.ok_or(Errno::ENOTCONN)?;
        let payload = &mut response[header_len..payload_end];
        let core = self.core_mut(interface);
        let (read, from) = match transport {
            Transport::Tcp => (core.recv(id, payload), None),
            Transport::Udp => {
                let result = core.udp_recv_from(id, payload);
                match result {
                    Ok((read, (ip, port))) => (
                        Ok(read),
                        Some(Endpoint {
                            ip: ip.octets(),
                            port,
                        }),
                    ),
                    Err(error) => (Err(error), None),
                }
            }
            Transport::Icmp => return Err(Errno::EINVAL),
            Transport::Control => return Err(Errno::EINVAL),
        };
        let read = match read {
            Ok(read) => read,
            Err(error) => return self.write_socket_error(response, error),
        };
        let header = Response::Read {
            len: read as u32,
            from,
        };
        header
            .encode(&mut response[..header_len])
            .map_err(|_| Errno::E2BIG)?;
        Ok(header_len + read)
    }

    fn typed_write(
        &mut self,
        file: u64,
        data: &[u8],
        to: Option<Endpoint>,
        response: &mut [u8],
    ) -> Result<usize, Errno> {
        let (interface, transport, socket, peer, write_shutdown) = match self.files.get(&file) {
            Some(File::Socket(socket)) => (
                socket.interface,
                socket.transport,
                socket.socket,
                socket.peer,
                socket.write_shutdown,
            ),
            Some(File::Control) => return Err(Errno::EINVAL),
            None => return Err(Errno::EBADF),
        };
        if write_shutdown {
            return self.write_socket_error(response, Errno::EPIPE);
        }
        let id = socket.ok_or(Errno::ENOTCONN)?;
        let core = self.core_mut(interface);
        let result = match transport {
            Transport::Tcp => core.send(id, data),
            Transport::Udp => {
                let endpoint = to.or(peer).ok_or(Errno::EADDRNOTAVAIL)?;
                core.udp_send_to(
                    id,
                    data,
                    (Ipv4Addr::from_octets(endpoint.ip), endpoint.port),
                )
            }
            Transport::Icmp | Transport::Control => Err(Errno::EINVAL),
        };
        match result {
            Ok(len) => self.write_response(response, &Response::Write { len: len as u32 }),
            Err(error) => self.write_socket_error(response, error),
        }
    }

    fn socket_call(&mut self, file: u64, call: Call, response: &mut [u8]) -> Result<usize, Errno> {
        let result = match call {
            Call::Bind(endpoint) => self.bind(file, endpoint),
            Call::Listen { backlog } => self.listen(file, backlog),
            Call::Accept => self.accept(file),
            Call::Connect(endpoint) => self.connect(file, endpoint),
            Call::Shutdown { how } => self.shutdown(file, how as u8),
            Call::GetSockName => self.endpoint_response(file, false),
            Call::GetPeerName => self.endpoint_response(file, true),
            Call::SetSockOpt { opt, value } => self.set_option(file, opt, value),
            Call::GetSockOpt { opt } => self.get_option(file, opt),
            // The target glue creates the S26 event handle and attaches it; the pure core has no
            // kernel handle table.
            Call::Event => Err(Errno::EOPNOTSUPP),
        };
        match result {
            Ok(response_value) => self.write_response(response, &response_value),
            Err(error) => self.write_socket_error(response, error),
        }
    }

    fn bind(&mut self, file: u64, endpoint: Endpoint) -> Result<Response, Errno> {
        let (interface, transport, has_socket) = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => return Err(Errno::EINVAL),
            File::Socket(socket) => (socket.interface, socket.transport, socket.socket.is_some()),
        };
        if has_socket {
            return Err(Errno::EINVAL);
        }
        match transport {
            Transport::Tcp => {
                let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
                    File::Socket(socket) => socket,
                    File::Control => return Err(Errno::EINVAL),
                };
                socket.local = endpoint;
                socket.bound = true;
                Ok(Response::Ok)
            }
            Transport::Udp => {
                let id = self.core_mut(interface).udp_bind(endpoint.port)?;
                let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
                    File::Socket(socket) => socket,
                    File::Control => return Err(Errno::EINVAL),
                };
                socket.socket = Some(id);
                socket.local = endpoint;
                socket.bound = true;
                self.apply_options(file);
                Ok(Response::Ok)
            }
            Transport::Icmp | Transport::Control => Err(Errno::EINVAL),
        }
    }

    fn listen(&mut self, file: u64, backlog: u16) -> Result<Response, Errno> {
        let (interface, transport, port, has_socket) =
            match self.files.get(&file).ok_or(Errno::EBADF)? {
                File::Control => return Err(Errno::EINVAL),
                File::Socket(socket) => (
                    socket.interface,
                    socket.transport,
                    socket.port_for_listen(),
                    socket.socket.is_some(),
                ),
            };
        if has_socket {
            return Err(Errno::EINVAL);
        }
        if transport != Transport::Tcp {
            return Err(Errno::EOPNOTSUPP);
        }
        let id = self
            .core_mut(interface)
            .tcp_listen(port, backlog as usize)?;
        let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
            File::Socket(socket) => socket,
            File::Control => return Err(Errno::EINVAL),
        };
        socket.socket = Some(id);
        socket.listener = true;
        socket.local.port = port;
        socket.bound = true;
        self.apply_options(file);
        Ok(Response::Ok)
    }

    fn accept(&mut self, file: u64) -> Result<Response, Errno> {
        let (interface, listener, local) = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => return Err(Errno::EINVAL),
            File::Socket(socket) => {
                if !socket.listener {
                    return Err(Errno::EINVAL);
                }
                (
                    socket.interface,
                    socket.socket.ok_or(Errno::EBADF)?,
                    socket.local,
                )
            }
        };
        let Some(socket) = self.core_mut(interface).tcp_accept(listener)? else {
            return Err(Errno::EAGAIN);
        };
        if self.accepted.len() >= MAX_PENDING_ACCEPTS {
            let _ = self.core_mut(interface).close(socket);
            return Err(Errno::ENFILE);
        }
        let accepted_local = self
            .core_ref(interface)
            .local_endpoint(socket)
            .map(|(ip, port)| Endpoint {
                ip: ip.octets(),
                port,
            })
            .unwrap_or(local);
        let remote = self
            .core_ref(interface)
            .peer_endpoint(socket)
            .map(|(ip, port)| Endpoint {
                ip: ip.octets(),
                port,
            });
        let accept_id = self.next_accept;
        self.next_accept = self.next_accept.checked_add(1).ok_or(Errno::ENFILE)?;
        self.accepted.insert(
            accept_id,
            Accepted {
                socket,
                local: accepted_local,
                remote,
                transport: Transport::Tcp,
                interface,
            },
        );
        Ok(Response::Socket { id: accept_id })
    }

    fn connect(&mut self, file: u64, endpoint: Endpoint) -> Result<Response, Errno> {
        let (interface, transport, local, bound, has_socket) =
            match self.files.get(&file).ok_or(Errno::EBADF)? {
                File::Control => return Err(Errno::EINVAL),
                File::Socket(socket) => (
                    socket.interface,
                    socket.transport,
                    socket.local,
                    socket.bound,
                    socket.socket.is_some(),
                ),
            };
        if has_socket {
            return Err(Errno::EISCONN);
        }
        match transport {
            Transport::Tcp => {
                let local_port = bound.then_some(local.port);
                let id = self.core_mut(interface).tcp_connect(
                    (Ipv4Addr::from_octets(endpoint.ip), endpoint.port),
                    local_port,
                )?;
                let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
                    File::Socket(socket) => socket,
                    File::Control => return Err(Errno::EINVAL),
                };
                socket.socket = Some(id);
                socket.remote = Some(endpoint);
                self.apply_options(file);
                Ok(Response::Ok)
            }
            Transport::Udp => {
                let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
                    File::Socket(socket) => socket,
                    File::Control => return Err(Errno::EINVAL),
                };
                socket.peer = Some(endpoint);
                Ok(Response::Ok)
            }
            Transport::Icmp | Transport::Control => Err(Errno::EINVAL),
        }
    }

    fn shutdown(&mut self, file: u64, how: u8) -> Result<Response, Errno> {
        let (interface, transport, socket) = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => return Err(Errno::EINVAL),
            File::Socket(socket) => (socket.interface, socket.transport, socket.socket),
        };
        if how > 2 || transport == Transport::Icmp {
            return Err(Errno::EINVAL);
        }
        let write = matches!(how, 1 | 2);
        let read = matches!(how, 0 | 2);
        if write
            && let Some(id) = socket
            && transport == Transport::Tcp
        {
            self.core_mut(interface).shutdown_write(id)?;
        }
        let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
            File::Socket(socket) => socket,
            File::Control => return Err(Errno::EINVAL),
        };
        socket.read_shutdown |= read;
        socket.write_shutdown |= write;
        Ok(Response::Ok)
    }

    fn endpoint_response(&mut self, file: u64, peer: bool) -> Result<Response, Errno> {
        let (interface, socket_id, local, remote, peer_endpoint) =
            match self.files.get(&file).ok_or(Errno::EBADF)? {
                File::Control => return Err(Errno::EINVAL),
                File::Socket(socket) => (
                    socket.interface,
                    socket.socket,
                    socket.local,
                    socket.remote,
                    socket.peer,
                ),
            };
        if let Some(id) = socket_id {
            let core = self.core_ref(interface);
            let endpoint = if peer {
                core.peer_endpoint(id)
            } else {
                core.local_endpoint(id)
            };
            if let Some((ip, port)) = endpoint {
                return Ok(Response::Endpoint(Endpoint {
                    ip: ip.octets(),
                    port,
                }));
            }
        }
        let endpoint = if peer {
            remote.or(peer_endpoint).ok_or(Errno::ENOTCONN)?
        } else {
            local
        };
        Ok(Response::Endpoint(endpoint))
    }

    fn set_option(&mut self, file: u64, opt: SockOpt, value: i32) -> Result<Response, Errno> {
        let (interface, socket_id) = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => return Err(Errno::EINVAL),
            File::Socket(socket) => (socket.interface, socket.socket),
        };
        if !opt.is_writable() || value < 0 {
            return Err(Errno::EINVAL);
        }
        if let Some(id) = socket_id {
            self.core_mut(interface)
                .set_socket_option(id, u32::from(opt.code()), value)?;
        }
        let socket = match self.files.get_mut(&file).ok_or(Errno::EBADF)? {
            File::Socket(socket) => socket,
            File::Control => return Err(Errno::EINVAL),
        };
        match opt {
            SockOpt::ReuseAddr => socket.reuse_addr = value != 0,
            SockOpt::RcvBuf => socket.rcvbuf = value,
            SockOpt::SndBuf => socket.sndbuf = value,
            SockOpt::TcpNoDelay => socket.nodelay = value != 0,
            SockOpt::Error => return Err(Errno::EINVAL),
        }
        Ok(Response::Ok)
    }

    fn get_option(&mut self, file: u64, opt: SockOpt) -> Result<Response, Errno> {
        let socket = match self.files.get(&file).ok_or(Errno::EBADF)? {
            File::Control => return Err(Errno::EINVAL),
            File::Socket(socket) => socket,
        };
        if let Some(id) = socket.socket
            && opt != SockOpt::Error
        {
            return self
                .core_ref(socket.interface)
                .get_socket_option(id, u32::from(opt.code()))
                .map(Response::Value);
        }
        let value = match opt {
            SockOpt::ReuseAddr => i32::from(socket.reuse_addr),
            SockOpt::RcvBuf => socket.rcvbuf,
            SockOpt::SndBuf => socket.sndbuf,
            SockOpt::TcpNoDelay => i32::from(socket.nodelay),
            SockOpt::Error => 0,
        };
        Ok(Response::Value(value))
    }

    fn apply_options(&mut self, file: u64) {
        let Some(File::Socket(socket)) = self.files.get(&file) else {
            return;
        };
        let Some(id) = socket.socket else {
            return;
        };
        let interface = socket.interface;
        let values = [
            (SockOpt::ReuseAddr, i32::from(socket.reuse_addr)),
            (SockOpt::RcvBuf, socket.rcvbuf),
            (SockOpt::SndBuf, socket.sndbuf),
            (SockOpt::TcpNoDelay, i32::from(socket.nodelay)),
        ];
        for (option, value) in values {
            let _ = self
                .core_mut(interface)
                .set_socket_option(id, u32::from(option.code()), value);
        }
    }

    fn control_call(&mut self, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        let request = IfaceRequest::decode(request).map_err(|_| Errno::EINVAL)?;
        match request {
            IfaceRequest::List => {
                let count = if self.nic.is_some() { 2 } else { 1 };
                let mut offset = 0usize;
                append_iface(response, &mut offset, IfaceResponse::Count { count })?;
                append_iface(response, &mut offset, IfaceResponse::Iface(self.iface()))?;
                if let Some(nic) = self.nic_iface() {
                    append_iface(response, &mut offset, IfaceResponse::Iface(nic))?;
                }
                Ok(offset)
            }
            IfaceRequest::Get { name } => {
                let iface = if name.as_str() == LOOPBACK_NAME {
                    Some(self.iface())
                } else if name.as_str() == NIC_NAME {
                    self.nic_iface()
                } else {
                    None
                };
                match iface {
                    Some(iface) => {
                        let mut offset = 0usize;
                        append_iface(response, &mut offset, IfaceResponse::Iface(iface))?;
                        Ok(offset)
                    }
                    None => {
                        let mut offset = 0usize;
                        append_iface(response, &mut offset, IfaceResponse::Err(Errno::ENODEV))?;
                        Ok(offset)
                    }
                }
            }
            IfaceRequest::SetAddress { name, addr, prefix } => {
                if prefix > 32 {
                    return self.iface_error(response, Errno::EINVAL);
                }
                if name.as_str() == LOOPBACK_NAME {
                    self.control.addr = addr;
                    self.control.prefix = prefix;
                    self.core
                        .set_ipv4(Ipv4Addr::from_octets(addr), prefix, None);
                    self.iface_ok(response)
                } else if name.as_str() == NIC_NAME {
                    let Some(nic) = self.nic_control.as_mut() else {
                        return self.iface_error(response, Errno::ENODEV);
                    };
                    nic.addr = addr;
                    nic.prefix = prefix;
                    let gateway = nic.gateway;
                    self.core_mut(Interface::Nic).set_ipv4(
                        Ipv4Addr::from_octets(addr),
                        prefix,
                        gateway.map(Ipv4Addr::from_octets),
                    );
                    self.iface_ok(response)
                } else {
                    self.iface_error(response, Errno::ENODEV)
                }
            }
            IfaceRequest::SetState { name, up } => {
                if name.as_str() == LOOPBACK_NAME {
                    self.control.up = up;
                    self.iface_ok(response)
                } else if name.as_str() == NIC_NAME {
                    let Some(nic) = self.nic_control.as_mut() else {
                        return self.iface_error(response, Errno::ENODEV);
                    };
                    nic.up = up;
                    self.iface_ok(response)
                } else {
                    self.iface_error(response, Errno::ENODEV)
                }
            }
            IfaceRequest::RouteAdd { route } => {
                self.add_route(Interface::Loopback, route)?;
                self.iface_ok(response)
            }
            IfaceRequest::RouteDel { dest, prefix } => {
                self.delete_route(Interface::Loopback, dest, prefix)?;
                self.iface_ok(response)
            }
            IfaceRequest::RouteList => {
                let mut offset = 0usize;
                let mut entries = Vec::new();
                for interface in self.interfaces() {
                    let name = interface_name(interface);
                    for route in self.core_ref(interface).routes() {
                        let mut flags = ROUTE_UP;
                        if route.gateway.is_some() {
                            flags |= ROUTE_GATEWAY;
                        }
                        if route.prefix_len == 32 {
                            flags |= ROUTE_HOST;
                        }
                        entries.push(IfaceResponse::RouteInfo(RouteInfo {
                            dest: route.dest.octets(),
                            prefix: route.prefix_len,
                            flags,
                            iface: IfName::new(name).ok_or(Errno::EINVAL)?,
                            gateway: route.gateway.map(|gateway| gateway.octets()),
                        }));
                    }
                }
                let count = u16::try_from(entries.len()).map_err(|_| Errno::E2BIG)?;
                append_iface(response, &mut offset, IfaceResponse::Count { count })?;
                for entry in entries {
                    append_iface(response, &mut offset, entry)?;
                }
                Ok(offset)
            }
            IfaceRequest::RouteAddOn { name, route } => {
                let Some(interface) = self.interface_for_name(name.as_str()) else {
                    return self.iface_error(response, Errno::ENODEV);
                };
                self.add_route(interface, route)?;
                self.iface_ok(response)
            }
            IfaceRequest::RouteDelOn { name, dest, prefix } => {
                let Some(interface) = self.interface_for_name(name.as_str()) else {
                    return self.iface_error(response, Errno::ENODEV);
                };
                self.delete_route(interface, dest, prefix)?;
                self.iface_ok(response)
            }
            IfaceRequest::DhcpLease { name } => {
                let Some(interface) = self.interface_for_name(name.as_str()) else {
                    return self.iface_error(response, Errno::ENODEV);
                };
                let Some(lease) = self.core_ref(interface).dhcp_lease() else {
                    return self.iface_error(response, Errno::ENOENT);
                };
                let mut offset = 0usize;
                append_iface(
                    response,
                    &mut offset,
                    IfaceResponse::DhcpLease(DhcpLeaseInfo {
                        addr: lease.address.octets(),
                        prefix: lease.prefix_len,
                        router: lease.router.map(|router| router.octets()),
                        dns: lease.dns.iter().map(|server| server.octets()).collect(),
                        renew_ms: lease.renew_in_ms.unwrap_or(DHCP_TIMER_UNKNOWN),
                        rebind_ms: lease.rebind_in_ms.unwrap_or(DHCP_TIMER_UNKNOWN),
                        expires_ms: lease.expires_in_ms.unwrap_or(DHCP_TIMER_UNKNOWN),
                    }),
                )?;
                Ok(offset)
            }
            IfaceRequest::SocketList => {
                let mut offset = 0usize;
                let mut entries = Vec::new();
                for interface in self.interfaces() {
                    let name = interface_name(interface);
                    for socket in self.core_ref(interface).socket_table() {
                        entries.push(IfaceResponse::Socket(socket_entry(name, socket)?));
                    }
                }
                let count = u16::try_from(entries.len()).map_err(|_| Errno::E2BIG)?;
                append_iface(response, &mut offset, IfaceResponse::Count { count })?;
                for entry in entries {
                    append_iface(response, &mut offset, entry)?;
                }
                Ok(offset)
            }
            IfaceRequest::Counters { name } => {
                let Some(interface) = self.interface_for_name(name.as_str()) else {
                    return self.iface_error(response, Errno::ENODEV);
                };
                let Some(counters) = self.core_ref(interface).interface_counters() else {
                    return self.iface_error(response, Errno::EOPNOTSUPP);
                };
                let mut offset = 0usize;
                append_iface(
                    response,
                    &mut offset,
                    IfaceResponse::Counters(CounterInfo {
                        rx_packets: counters.rx_packets,
                        tx_packets: counters.tx_packets,
                        rx_bytes: counters.rx_bytes,
                        tx_bytes: counters.tx_bytes,
                        rx_errors: counters.rx_errors,
                        tx_errors: counters.tx_errors,
                        rx_drops: counters.rx_drops,
                        tx_drops: counters.tx_drops,
                    }),
                )?;
                Ok(offset)
            }
            IfaceRequest::ArpList => {
                let mut offset = 0usize;
                append_iface(response, &mut offset, IfaceResponse::Count { count: 0 })?;
                Ok(offset)
            }
        }
    }

    fn interfaces(&self) -> Vec<Interface> {
        let mut interfaces = alloc::vec![Interface::Loopback];
        if self.nic.is_some() {
            interfaces.push(Interface::Nic);
        }
        interfaces
    }

    fn interface_for_name(&self, name: &str) -> Option<Interface> {
        if name == LOOPBACK_NAME {
            Some(Interface::Loopback)
        } else if name == NIC_NAME && self.nic.is_some() {
            Some(Interface::Nic)
        } else {
            None
        }
    }

    fn interface_mtu(&self, interface: Interface) -> Result<usize, Errno> {
        match interface {
            Interface::Loopback => Ok(usize::from(LOOPBACK_MTU)),
            Interface::Nic => self
                .nic_config
                .map(|config| usize::from(config.mtu))
                .ok_or(Errno::ENODEV),
        }
    }

    fn add_route(
        &mut self,
        interface: Interface,
        route: raven_proto_net::Route,
    ) -> Result<(), Errno> {
        if route.prefix > 32 {
            return Err(Errno::EINVAL);
        }
        let mtu = self.interface_mtu(interface)?;
        self.core_mut(interface).add_route(RouteEntry {
            dest: Ipv4Addr::from_octets(route.dest),
            prefix_len: route.prefix,
            gateway: route.gateway.map(Ipv4Addr::from_octets),
            mtu,
        })
    }

    fn delete_route(
        &mut self,
        interface: Interface,
        dest: [u8; 4],
        prefix: u8,
    ) -> Result<(), Errno> {
        if prefix > 32 {
            return Err(Errno::EINVAL);
        }
        self.core_mut(interface)
            .delete_route(Ipv4Addr::from_octets(dest), prefix)
    }

    fn iface(&self) -> IfaceInfo {
        let flags = IF_LINK_UP | IF_LOOPBACK | if self.control.up { IF_UP } else { 0 };
        IfaceInfo {
            name: IfName::new(LOOPBACK_NAME).unwrap_or(IfName::new("lo").expect("constant name")),
            addr: self.control.addr,
            prefix: self.control.prefix,
            flags,
            mtu: LOOPBACK_MTU,
            mac: [0; 6],
        }
    }

    fn nic_iface(&self) -> Option<IfaceInfo> {
        let config = self.nic_config?;
        let control = self.nic_control.as_ref()?;
        let lease = self.nic.as_ref().and_then(|nic| nic.dhcp_lease());
        let (addr, prefix) = lease
            .as_ref()
            .map(|lease| (lease.address.octets(), lease.prefix_len))
            .unwrap_or((control.addr, control.prefix));
        let flags = IF_LINK_UP | if control.up { IF_UP } else { 0 };
        Some(IfaceInfo {
            name: IfName::new(NIC_NAME)?,
            addr,
            prefix,
            flags,
            mtu: config.mtu,
            mac: config.mac,
        })
    }

    fn iface_ok(&self, response: &mut [u8]) -> Result<usize, Errno> {
        let mut offset = 0usize;
        append_iface(response, &mut offset, IfaceResponse::Ok)?;
        Ok(offset)
    }

    fn iface_error(&self, response: &mut [u8], error: Errno) -> Result<usize, Errno> {
        let mut offset = 0usize;
        append_iface(response, &mut offset, IfaceResponse::Err(error))?;
        Ok(offset)
    }

    fn write_response(&self, response: &mut [u8], value: &Response) -> Result<usize, Errno> {
        value.encode(response).map_err(|_| Errno::E2BIG)
    }

    fn write_socket_error(&self, response: &mut [u8], error: Errno) -> Result<usize, Errno> {
        self.write_response(response, &Response::Err(error))
    }
}

fn interface_name(interface: Interface) -> &'static str {
    match interface {
        Interface::Loopback => LOOPBACK_NAME,
        Interface::Nic => NIC_NAME,
    }
}

fn socket_entry(name: &str, socket: CoreSocket) -> Result<SocketEntry, Errno> {
    let (transport, state) = match (socket.transport, socket.state) {
        (SocketTransport::Tcp, SocketState::Closed) => (SOCKET_TCP, SOCKET_STATE_CLOSED),
        (SocketTransport::Tcp, SocketState::Listen) => (SOCKET_TCP, SOCKET_STATE_LISTEN),
        (SocketTransport::Tcp, SocketState::SynSent) => (SOCKET_TCP, SOCKET_STATE_SYN_SENT),
        (SocketTransport::Tcp, SocketState::SynReceived) => (SOCKET_TCP, SOCKET_STATE_SYN_RECEIVED),
        (SocketTransport::Tcp, SocketState::Established) => (SOCKET_TCP, SOCKET_STATE_ESTABLISHED),
        (SocketTransport::Tcp, SocketState::FinWait1) => (SOCKET_TCP, SOCKET_STATE_FIN_WAIT_1),
        (SocketTransport::Tcp, SocketState::FinWait2) => (SOCKET_TCP, SOCKET_STATE_FIN_WAIT_2),
        (SocketTransport::Tcp, SocketState::CloseWait) => (SOCKET_TCP, SOCKET_STATE_CLOSE_WAIT),
        (SocketTransport::Tcp, SocketState::Closing) => (SOCKET_TCP, SOCKET_STATE_CLOSING),
        (SocketTransport::Tcp, SocketState::LastAck) => (SOCKET_TCP, SOCKET_STATE_LAST_ACK),
        (SocketTransport::Tcp, SocketState::TimeWait) => (SOCKET_TCP, SOCKET_STATE_TIME_WAIT),
        (SocketTransport::Udp, _) => (SOCKET_UDP, SOCKET_STATE_UDP),
        (SocketTransport::Icmp, _) => (SOCKET_ICMP, SOCKET_STATE_ICMP),
        (SocketTransport::Tcp, _) => (SOCKET_TCP, SOCKET_STATE_CLOSED),
    };
    let (local, local_port) = socket.local.unwrap_or((Ipv4Addr::UNSPECIFIED, 0));
    Ok(SocketEntry {
        id: socket.id.0,
        transport,
        state,
        iface: IfName::new(name).ok_or(Errno::EINVAL)?,
        local: local.octets(),
        local_port,
        remote: socket.remote.map(|remote| remote.0.octets()),
        remote_port: socket.remote.map(|remote| remote.1).unwrap_or(0),
        rx_queue: socket.rx_queue,
        tx_queue: socket.tx_queue,
    })
}

fn append_iface(
    response: &mut [u8],
    offset: &mut usize,
    value: IfaceResponse,
) -> Result<(), Errno> {
    let mut record = [0u8; 128];
    let length = value.encode(&mut record).map_err(|_| Errno::E2BIG)?;
    let start = *offset;
    let end = start.checked_add(length).ok_or(Errno::E2BIG)?;
    if end > response.len() {
        return Err(Errno::E2BIG);
    }
    response[start..end].copy_from_slice(record.get(..length).ok_or(Errno::E2BIG)?);
    *offset = end;
    Ok(())
}

#[cfg(test)]
#[path = "netd_ut.rs"]
mod tests;
