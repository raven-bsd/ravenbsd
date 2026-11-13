// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The deterministic fake netd used by the host tests.
use crate::ErrnoExt;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};
use raven_proto_net::path::{self, Transport};
use raven_proto_net::{
    Call, Endpoint, IF_LINK_UP, IF_LOOPBACK, IF_UP, IfName, IfaceInfo, IfaceRequest, IfaceResponse,
    SockOpt,
};

use crate::{
    CallReply, Caps, FdInfo, NetBackend, NetEnv, ReadDone, Sock, SocketFds, UnixBackend, WaitItem,
};

/// The fake operation selected by [`FakeNetd::fail_next`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakeOp {
    /// `OPEN`.
    Open,
    /// `READ`.
    Read,
    /// `WRITE`.
    Write,
    /// `CALL`.
    Call,
    /// `WAIT`.
    Wait,
    /// Interface control.
    Iface,
}

/// One asserted wire request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireLog {
    /// `OPEN` with a path.
    Open(Vec<u8>),
    /// `READ`.
    Read(usize),
    /// `WRITE` with length and destination.
    Write {
        /// Byte count.
        len: usize,
        /// Destination endpoint.
        to: Option<Endpoint>,
    },
    /// `CALL`.
    Call(Call),
    /// `CLOSE`.
    Close(Sock),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Tcp,
    Udp,
    Unix,
}

#[derive(Clone, Debug)]
struct FakeSock {
    kind: Kind,
    local: Endpoint,
    peer: Option<Endpoint>,
    peer_sock: Option<u32>,
    bound: bool,
    listening: bool,
    connected: bool,
    closed: bool,
    reset: bool,
    read_shut: bool,
    write_shut: bool,
    queue: VecDeque<u8>,
    dgrams: VecDeque<(Option<Endpoint>, Vec<u8>, bool)>,
    accepts: VecDeque<(u32, Endpoint)>,
    opts: BTreeMap<u16, i32>,
    so_error: i32,
    dns_tcp: Vec<u8>,
}

impl FakeSock {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            local: Endpoint {
                ip: [0, 0, 0, 0],
                port: 0,
            },
            peer: None,
            peer_sock: None,
            bound: false,
            listening: false,
            connected: false,
            closed: false,
            reset: false,
            read_shut: false,
            write_shut: false,
            queue: VecDeque::new(),
            dgrams: VecDeque::new(),
            accepts: VecDeque::new(),
            opts: BTreeMap::new(),
            so_error: 0,
            dns_tcp: Vec::new(),
        }
    }
}

#[derive(Default)]
struct FakeState {
    socks: BTreeMap<u32, FakeSock>,
    next_sock: u32,
    next_fd: i32,
    fds: BTreeMap<i32, FdInfo>,
    log: Vec<WireLog>,
    clock_ms: u64,
    entropy: u64,
    ifaces: Vec<IfaceInfo>,
    ifindex: bool,
    dns: Vec<(Vec<u8>, u16, Vec<u8>)>,
    dns_servfail_once: bool,
    dns_drop_once: bool,
    dns_wrong_source: bool,
    dns_wrong_id: bool,
    dns_truncate_udp: bool,
    ephemeral: u16,
    fail: Option<(FakeOp, Errno)>,
    max_read: Option<usize>,
    route_fail: bool,
}

/// A fake netd plus fd table and environment.
#[derive(Clone)]
pub struct FakeNetd {
    caps: Caps,
    state: Rc<RefCell<FakeState>>,
}

impl Default for FakeNetd {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeNetd {
    /// A fake with the "today" capability set and `lo`/`eth0` interfaces.
    #[must_use]
    pub fn new() -> Self {
        let mut state = FakeState {
            next_sock: 1,
            next_fd: 3,
            clock_ms: 1,
            entropy: 0x1234_5678_9abc_def0,
            ephemeral: 49_300,
            ..FakeState::default()
        };
        if let Some(info) = record(
            "lo",
            [127, 0, 0, 1],
            8,
            IF_UP | IF_LINK_UP | IF_LOOPBACK,
            65_535,
            [0; 6],
        ) {
            state.ifaces.push(info);
        }
        if let Some(info) = record(
            "eth0",
            [10, 0, 2, 15],
            24,
            IF_UP | IF_LINK_UP,
            1500,
            [2, 0, 0, 0, 0, 1],
        ) {
            state.ifaces.push(info);
        }
        Self {
            caps: Caps::default(),
            state: Rc::new(RefCell::new(state)),
        }
    }

    /// A fake with the given capability switches.
    #[must_use]
    pub fn with_caps(caps: Caps) -> Self {
        let mut value = Self::new();
        value.caps = caps;
        value
    }

    /// Replace the interface list.
    pub fn set_ifaces(&self, list: Vec<IfaceInfo>) {
        self.state.borrow_mut().ifaces = list;
    }

    /// Add one interface record.
    pub fn add_iface(&self, info: IfaceInfo) {
        self.state.borrow_mut().ifaces.push(info);
    }

    /// Add a DNS answer table row.
    pub fn add_dns_answer(&self, name: &[u8], rtype: u16, rdata: &[u8]) {
        self.state
            .borrow_mut()
            .dns
            .push((name.to_ascii_lowercase(), rtype, rdata.to_vec()));
    }

    /// Drop the first DNS reply.
    pub fn drop_first_dns_reply(&self) {
        self.state.borrow_mut().dns_drop_once = true;
    }

    /// Answer the first DNS query with SERVFAIL.
    pub fn servfail_then_success(&self) {
        self.state.borrow_mut().dns_servfail_once = true;
    }

    /// Answer from the wrong source address.
    pub fn wrong_dns_source(&self) {
        self.state.borrow_mut().dns_wrong_source = true;
    }

    /// Change the reply ID.
    pub fn wrong_dns_id(&self) {
        self.state.borrow_mut().dns_wrong_id = true;
    }

    /// Mark the first UDP DNS reply truncated (TC bit).
    pub fn truncate_first_udp_reply(&self) {
        self.state.borrow_mut().dns_truncate_udp = true;
    }

    /// Make the next call of `op` fail with `error`.
    pub fn fail_next(&self, op: FakeOp, error: Errno) {
        self.state.borrow_mut().fail = Some((op, error));
    }

    /// Make every UDP `CONNECT` fail with `ENETUNREACH` (PG-41 route script).
    pub fn fail_routes(&self) {
        self.state.borrow_mut().route_fail = true;
    }

    /// Limit one `READ` to `count` bytes to exercise `MSG_WAITALL`.
    pub fn set_max_read(&self, count: usize) {
        self.state.borrow_mut().max_read = Some(count);
    }

    /// The local ports of every live UDP socket.
    #[must_use]
    pub fn udp_local_ports(&self) -> Vec<u16> {
        self.state
            .borrow()
            .socks
            .values()
            .filter(|sock| sock.kind == Kind::Udp && sock.local.port != 0)
            .map(|sock| sock.local.port)
            .collect()
    }

    /// Put one datagram into a bound UDP socket's queue.
    pub fn inject_datagram(&self, port: u16, from: Endpoint, data: &[u8]) {
        let mut state = self.state.borrow_mut();
        if let Some(sock) = state
            .socks
            .values_mut()
            .find(|sock| sock.kind == Kind::Udp && sock.local.port == port)
        {
            sock.dgrams.push_back((Some(from), data.to_vec(), false));
        }
    }

    /// Simulate a netd restart: every live handle fails with `ECONNRESET`.
    pub fn restart_netd(&self) {
        let mut state = self.state.borrow_mut();
        for sock in state.socks.values_mut() {
            sock.reset = true;
        }
    }

    /// Advance the fake monotonic clock.
    pub fn advance(&self, milliseconds: u64) {
        let mut state = self.state.borrow_mut();
        state.clock_ms = state.clock_ms.saturating_add(milliseconds);
    }

    /// The current fake clock.
    #[must_use]
    pub fn clock(&self) -> u64 {
        self.state.borrow().clock_ms
    }

    /// The request log.
    #[must_use]
    pub fn log(&self) -> Vec<WireLog> {
        self.state.borrow().log.clone()
    }

    /// Clear the request log.
    pub fn clear_log(&self) {
        self.state.borrow_mut().log.clear();
    }

    /// The number of live backend sockets.
    #[must_use]
    pub fn socket_count(&self) -> usize {
        self.state.borrow().socks.len()
    }

    fn take_failure(state: &mut FakeState, op: FakeOp) -> Option<Errno> {
        match state.fail {
            Some((expected, error)) if expected == op => {
                state.fail = None;
                Some(error)
            }
            _ => None,
        }
    }

    fn open_endpoint(&self, kind: Kind, endpoint: Option<Endpoint>) -> Result<Sock, Errno> {
        let mut state = self.state.borrow_mut();
        let client_id = state.next_sock;
        state.next_sock = state.next_sock.saturating_add(1);
        let mut client = FakeSock::new(kind);
        if let Some(endpoint) = endpoint {
            client.peer = Some(endpoint);
            client.connected = true;
            if kind == Kind::Tcp {
                client.local = endpoint;
                client.local.port = state.ephemeral;
                client.bound = true;
                state.ephemeral = state.ephemeral.saturating_add(1);
                let listener_id = state
                    .socks
                    .iter()
                    .find(|(_, socket)| {
                        socket.listening
                            && (socket.local.port == endpoint.port
                                || socket.peer.map(|peer| peer.port) == Some(endpoint.port))
                    })
                    .map(|(id, _)| *id);
                if let Some(listener_id) = listener_id {
                    let server_id = state.next_sock;
                    state.next_sock = state.next_sock.saturating_add(1);
                    let mut server = FakeSock::new(Kind::Tcp);
                    server.local = endpoint;
                    server.connected = true;
                    server.peer = Some(client.local);
                    server.peer_sock = Some(client_id);
                    client.peer_sock = Some(server_id);
                    if let Some(listener) = state.socks.get_mut(&listener_id) {
                        listener.accepts.push_back((server_id, client.local));
                    }
                    state.socks.insert(server_id, server);
                }
            }
        }
        state.socks.insert(client_id, client);
        Ok(Sock(client_id))
    }
}

impl NetBackend for FakeNetd {
    fn caps(&self) -> Caps {
        let mut caps = self.caps;
        caps.ifindex = self.state.borrow().ifindex;
        caps
    }

    fn open(&self, text: &str) -> Result<Sock, Errno> {
        self.state
            .borrow_mut()
            .log
            .push(WireLog::Open(text.as_bytes().to_vec()));
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Open) {
            return Err(error);
        }
        if text == "net:tcp" || text == "net:udp" {
            if !self.caps.bare_open {
                return Err(Errno::EINVAL);
            }
            let kind = if text == "net:tcp" {
                Kind::Tcp
            } else {
                Kind::Udp
            };
            return self.open_endpoint(kind, None);
        }
        let parsed = path::parse(text).map_err(|_| Errno::EINVAL)?;
        match parsed.transport {
            Transport::Tcp => self.open_endpoint(Kind::Tcp, parsed.endpoint),
            Transport::Udp => self.open_endpoint(Kind::Udp, parsed.endpoint),
            _ => Err(Errno::EINVAL),
        }
    }

    fn read(&self, s: Sock, buf: &mut [u8]) -> Result<ReadDone, Errno> {
        self.state.borrow_mut().log.push(WireLog::Read(buf.len()));
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Read) {
            return Err(error);
        }
        let mut state = self.state.borrow_mut();
        let limit = state.max_read.unwrap_or(usize::MAX);
        let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
        if sock.reset {
            return Err(Errno::ECONNRESET);
        }
        if sock.kind == Kind::Udp {
            if let Some((from, data, truncated)) = sock.dgrams.pop_front() {
                let len = data.len().min(buf.len());
                if let (Some(target), Some(source)) = (buf.get_mut(..len), data.get(..len)) {
                    target.copy_from_slice(source);
                }
                return Ok(ReadDone {
                    len,
                    from,
                    truncated: truncated || len < data.len(),
                });
            }
            if sock.closed {
                return Ok(ReadDone {
                    len: 0,
                    from: None,
                    truncated: false,
                });
            }
            return Err(Errno::EAGAIN);
        }
        if sock.queue.is_empty() {
            if sock.closed || sock.read_shut {
                return Ok(ReadDone {
                    len: 0,
                    from: None,
                    truncated: false,
                });
            }
            return Err(Errno::EAGAIN);
        }
        let len = sock.queue.len().min(buf.len()).min(limit);
        for slot in buf.iter_mut().take(len) {
            if let Some(byte) = sock.queue.pop_front() {
                *slot = byte;
            }
        }
        Ok(ReadDone {
            len,
            from: None,
            truncated: false,
        })
    }

    fn peek(&self, s: Sock, buf: &mut [u8]) -> Result<ReadDone, Errno> {
        let mut state = self.state.borrow_mut();
        let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
        if sock.kind == Kind::Udp {
            if let Some((from, data, truncated)) = sock.dgrams.front() {
                let len = data.len().min(buf.len());
                if let (Some(target), Some(source)) = (buf.get_mut(..len), data.get(..len)) {
                    target.copy_from_slice(source);
                }
                return Ok(ReadDone {
                    len,
                    from: *from,
                    truncated: *truncated || len < data.len(),
                });
            }
            return Err(Errno::EAGAIN);
        }
        if sock.queue.is_empty() {
            if sock.closed || sock.read_shut {
                return Ok(ReadDone {
                    len: 0,
                    from: None,
                    truncated: false,
                });
            }
            return Err(Errno::EAGAIN);
        }
        let len = sock.queue.len().min(buf.len());
        for (index, slot) in buf.iter_mut().take(len).enumerate() {
            if let Some(byte) = sock.queue.get(index) {
                *slot = *byte;
            }
        }
        Ok(ReadDone {
            len,
            from: None,
            truncated: false,
        })
    }

    fn write(&self, s: Sock, data: &[u8], to: Option<Endpoint>) -> Result<usize, Errno> {
        self.state.borrow_mut().log.push(WireLog::Write {
            len: data.len(),
            to,
        });
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Write) {
            return Err(error);
        }
        let (kind, peer, peer_sock, closed, write_shut, reset, dns_so_far) = {
            let state = self.state.borrow();
            let sock = state.socks.get(&s.0).ok_or(Errno::EBADF)?;
            (
                sock.kind,
                sock.peer,
                sock.peer_sock,
                sock.closed,
                sock.write_shut,
                sock.reset,
                sock.dns_tcp.clone(),
            )
        };
        if reset {
            return Err(Errno::ECONNRESET);
        }
        if write_shut || closed {
            return Err(Errno::EPIPE);
        }
        let destination = to.or(peer).unwrap_or(Endpoint {
            ip: [0, 0, 0, 0],
            port: 0,
        });
        if kind == Kind::Udp {
            if destination.port == 53 {
                let mut state = self.state.borrow_mut();
                if let Some(response) = dns_response(&mut state, data, false) {
                    let source = if state.dns_wrong_source {
                        state.dns_wrong_source = false;
                        Endpoint {
                            ip: [203, 0, 113, 9],
                            port: 53,
                        }
                    } else {
                        destination
                    };
                    if let Some(sock) = state.socks.get_mut(&s.0) {
                        sock.dgrams.push_back((Some(source), response, false));
                    }
                }
                return Ok(data.len());
            }
            if let Some(peer_id) = peer_sock {
                if let Some(peer) = self.state.borrow_mut().socks.get_mut(&peer_id) {
                    peer.dgrams
                        .push_back((Some(destination), data.to_vec(), false));
                }
            } else {
                let mut state = self.state.borrow_mut();
                if let Some(target) = state
                    .socks
                    .values_mut()
                    .find(|sock| sock.kind == Kind::Udp && sock.local.port == destination.port)
                {
                    target
                        .dgrams
                        .push_back((Some(destination), data.to_vec(), false));
                }
            }
            return Ok(data.len());
        }
        if kind == Kind::Unix {
            if let Some(peer_id) = peer_sock
                && let Some(peer) = self.state.borrow_mut().socks.get_mut(&peer_id)
            {
                for byte in data {
                    peer.queue.push_back(*byte);
                }
            }
            return Ok(data.len());
        }
        if destination.port == 53 {
            let mut buffer = dns_so_far;
            buffer.extend_from_slice(data);
            let mut cursor = 0usize;
            let mut outputs: Vec<Vec<u8>> = Vec::new();
            while cursor.saturating_add(2) <= buffer.len() {
                let length = (usize::from(buffer.get(cursor).copied().unwrap_or(0)) << 8)
                    | usize::from(buffer.get(cursor.saturating_add(1)).copied().unwrap_or(0));
                if buffer.len() < cursor.saturating_add(2).saturating_add(length) {
                    break;
                }
                let query = buffer
                    .get(cursor.saturating_add(2)..cursor.saturating_add(2).saturating_add(length))
                    .unwrap_or(&[])
                    .to_vec();
                cursor = cursor.saturating_add(2).saturating_add(length);
                let mut state = self.state.borrow_mut();
                if let Some(response) = dns_response(&mut state, &query, true) {
                    outputs.push(response);
                }
            }
            let remaining = buffer.get(cursor..).unwrap_or(&[]).to_vec();
            let mut state = self.state.borrow_mut();
            if let Some(sock) = state.socks.get_mut(&s.0) {
                sock.dns_tcp = remaining;
                for response in outputs {
                    sock.queue.push_back((response.len() >> 8) as u8);
                    sock.queue.push_back(response.len() as u8);
                    for byte in response {
                        sock.queue.push_back(byte);
                    }
                }
            }
            return Ok(data.len());
        }
        let peer_id = peer_sock.ok_or(Errno::ENOTCONN)?;
        if let Some(peer) = self.state.borrow_mut().socks.get_mut(&peer_id) {
            for byte in data {
                peer.queue.push_back(*byte);
            }
        }
        Ok(data.len())
    }

    fn call(&self, s: Sock, c: Call) -> Result<CallReply, Errno> {
        self.state.borrow_mut().log.push(WireLog::Call(c));
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Call) {
            return Err(error);
        }
        let mut state = self.state.borrow_mut();
        match c {
            Call::Bind(endpoint) => {
                let local = if endpoint.port == 0 {
                    let port = state.ephemeral;
                    state.ephemeral = state.ephemeral.saturating_add(1);
                    Endpoint {
                        ip: endpoint.ip,
                        port,
                    }
                } else {
                    endpoint
                };
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                sock.local = local;
                sock.bound = true;
                Ok(CallReply::Ok)
            }
            Call::Listen { backlog } => {
                if backlog == 0 || backlog > 4096 {
                    return Err(Errno::EINVAL);
                }
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                sock.listening = true;
                Ok(CallReply::Ok)
            }
            Call::Accept => {
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                if !sock.listening {
                    return Err(Errno::EINVAL);
                }
                if let Some((id, _peer)) = sock.accepts.pop_front() {
                    Ok(CallReply::Accepted(Sock(id)))
                } else {
                    Err(Errno::EAGAIN)
                }
            }
            Call::Connect(endpoint) => {
                if state.route_fail {
                    return Err(Errno::ENETUNREACH);
                }
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                sock.peer = Some(endpoint);
                sock.connected = true;
                Ok(CallReply::Ok)
            }
            Call::Shutdown { how } => {
                let code = how.code();
                let peer = state.socks.get(&s.0).and_then(|sock| sock.peer_sock);
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                if code == 0 || code == 2 {
                    sock.read_shut = true;
                }
                if code == 1 || code == 2 {
                    sock.write_shut = true;
                }
                if (code == 1 || code == 2)
                    && let Some(peer) = peer
                    && let Some(other) = state.socks.get_mut(&peer)
                {
                    other.read_shut = true;
                }
                Ok(CallReply::Ok)
            }
            Call::GetSockName => {
                let sock = state.socks.get(&s.0).ok_or(Errno::EBADF)?;
                Ok(CallReply::Endpoint(sock.local))
            }
            Call::GetPeerName => {
                let sock = state.socks.get(&s.0).ok_or(Errno::EBADF)?;
                sock.peer
                    .map_or(Err(Errno::ENOTCONN), |peer| Ok(CallReply::Endpoint(peer)))
            }
            Call::SetSockOpt { opt, value } => {
                let sock = state.socks.get_mut(&s.0).ok_or(Errno::EBADF)?;
                if !opt.is_writable() || value < 0 {
                    return Err(Errno::EINVAL);
                }
                sock.opts.insert(opt.code(), value);
                Ok(CallReply::Ok)
            }
            Call::GetSockOpt { opt } => {
                let sock = state.socks.get(&s.0).ok_or(Errno::EBADF)?;
                if opt == SockOpt::Error {
                    return Ok(CallReply::Value(sock.so_error));
                }
                Ok(CallReply::Value(
                    sock.opts.get(&opt.code()).copied().unwrap_or(0),
                ))
            }
            Call::Event => Ok(CallReply::Ok),
        }
    }

    fn close(&self, s: Sock) {
        let mut state = self.state.borrow_mut();
        state.log.push(WireLog::Close(s));
        if let Some(sock) = state.socks.get_mut(&s.0) {
            sock.closed = true;
        }
    }

    fn wait(&self, items: &mut [WaitItem], timeout_ms: Option<u64>) -> Result<usize, Errno> {
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Wait) {
            return Err(error);
        }
        let mut state = self.state.borrow_mut();
        let mut fired = 0usize;
        for item in items.iter_mut() {
            let Some(sock) = state.socks.get(&item.sock.0) else {
                item.got_err = true;
                fired = fired.saturating_add(1);
                continue;
            };
            if item.want_read {
                let ready = if sock.kind == Kind::Tcp {
                    !sock.queue.is_empty()
                        || sock.closed
                        || sock.read_shut
                        || sock.reset
                        || (sock.listening && !sock.accepts.is_empty())
                } else {
                    !sock.dgrams.is_empty() || sock.closed || sock.reset
                };
                if ready {
                    item.got_read = true;
                    fired = fired.saturating_add(1);
                }
                if sock.closed || sock.reset {
                    item.got_hup = true;
                }
            }
            if item.want_write && (sock.connected || !sock.write_shut) {
                item.got_write = true;
                fired = fired.saturating_add(1);
            }
            if sock.reset {
                item.got_err = true;
            }
        }
        if fired == 0
            && let Some(timeout) = timeout_ms
            && timeout > 0
        {
            state.clock_ms = state.clock_ms.saturating_add(timeout);
        }
        Ok(fired)
    }

    fn iface(&self, req: IfaceRequest) -> Result<IfaceResponse, Errno> {
        if let Some(error) = Self::take_failure(&mut self.state.borrow_mut(), FakeOp::Iface) {
            return Err(error);
        }
        let state = self.state.borrow();
        match req {
            IfaceRequest::List => Ok(IfaceResponse::Count {
                count: u16::try_from(state.ifaces.len()).unwrap_or(0),
            }),
            IfaceRequest::Get { name } => {
                let wanted = name.as_str();
                state
                    .ifaces
                    .iter()
                    .find(|info| info.name.as_str() == wanted)
                    .copied()
                    .map(IfaceResponse::Iface)
                    .ok_or(Errno::ENODEV)
            }
            _ => Err(Errno::ENOSYS),
        }
    }

    fn iface_list(&self) -> Result<Vec<IfaceResponse>, Errno> {
        let state = self.state.borrow();
        let mut out = Vec::new();
        out.push(IfaceResponse::Count {
            count: u16::try_from(state.ifaces.len()).unwrap_or(0),
        });
        for info in state.ifaces.iter() {
            out.push(IfaceResponse::Iface(*info));
        }
        Ok(out)
    }
}

impl SocketFds for FakeNetd {
    fn install(&self, s: Option<Sock>, cloexec: bool, nonblock: bool) -> Result<i32, Errno> {
        let _ = cloexec;
        let mut state = self.state.borrow_mut();
        let fd = state.next_fd;
        state.next_fd = state.next_fd.saturating_add(1);
        let ofd = u32::try_from(fd).unwrap_or(1).saturating_add(1);
        state.fds.insert(
            fd,
            FdInfo {
                ofd,
                sock: s,
                nonblock,
            },
        );
        Ok(fd)
    }

    fn attach(&self, ofd: u32, s: Sock) {
        let mut state = self.state.borrow_mut();
        for info in state.fds.values_mut() {
            if info.ofd == ofd {
                info.sock = Some(s);
            }
        }
    }

    fn resolve(&self, fd: i32) -> Result<FdInfo, Errno> {
        self.state
            .borrow()
            .fds
            .get(&fd)
            .copied()
            .ok_or(Errno::EBADF)
    }

    fn release(&self, ofd: u32) {
        let mut state = self.state.borrow_mut();
        state.fds.retain(|_, info| info.ofd != ofd);
    }
}

impl NetEnv for FakeNetd {
    fn monotonic_ms(&self) -> u64 {
        self.state.borrow().clock_ms
    }

    fn entropy(&self, buf: &mut [u8]) -> bool {
        let mut state = self.state.borrow_mut();
        for byte in buf.iter_mut() {
            state.entropy ^= state.entropy << 13;
            state.entropy ^= state.entropy >> 7;
            state.entropy ^= state.entropy << 17;
            *byte = state.entropy as u8;
        }
        true
    }

    fn now_secs(&self) -> i64 {
        1_000_000i64.saturating_add((self.state.borrow().clock_ms / 1000) as i64)
    }
}

impl UnixBackend for FakeNetd {
    fn pair(&self, ty: i32) -> Result<(Sock, Sock), Errno> {
        if ty != SOCK_STREAM && ty != SOCK_DGRAM {
            return Err(Errno::ESOCKTNOSUPPORT);
        }
        let mut state = self.state.borrow_mut();
        let first = state.next_sock;
        state.next_sock = state.next_sock.saturating_add(1);
        let second = state.next_sock;
        state.next_sock = state.next_sock.saturating_add(1);
        let mut left = FakeSock::new(Kind::Unix);
        left.connected = true;
        left.peer_sock = Some(second);
        let mut right = FakeSock::new(Kind::Unix);
        right.connected = true;
        right.peer_sock = Some(first);
        state.socks.insert(first, left);
        state.socks.insert(second, right);
        Ok((Sock(first), Sock(second)))
    }

    fn bind(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }

    fn connect(&self, _path: &[u8]) -> Result<Sock, Errno> {
        Err(Errno::EAFNOSUPPORT)
    }
}

/// Build a fake interface record.
#[must_use]
pub fn record(
    name: &str,
    addr: [u8; 4],
    prefix: u8,
    flags: u8,
    mtu: u16,
    mac: [u8; 6],
) -> Option<IfaceInfo> {
    let name = IfName::new(name)?;
    Some(IfaceInfo {
        name,
        addr,
        prefix,
        flags,
        mtu,
        mac,
    })
}

/// Build a validated fake interface name.
#[must_use]
pub fn ifname(text: &str) -> Option<IfName> {
    IfName::new(text)
}

fn dns_response(state: &mut FakeState, query: &[u8], tcp: bool) -> Option<Vec<u8>> {
    let name = dns_name(query)?;
    let qtype = dns_qtype(query)?;
    let id = [*query.first()?, *query.get(1)?];
    if state.dns_drop_once && !tcp {
        state.dns_drop_once = false;
        return None;
    }
    if state.dns_servfail_once {
        state.dns_servfail_once = false;
        return Some(dns_message(id, query, 2, &[]));
    }
    if state.dns_truncate_udp && !tcp {
        state.dns_truncate_udp = false;
        let mut response = dns_message(id, query, 3, &[]);
        if let Some(flag) = response.get_mut(2) {
            *flag |= 0x02;
        }
        return Some(response);
    }
    let answer = state
        .dns
        .iter()
        .find(|(candidate, rtype, _)| *rtype == qtype && candidate.as_slice() == name.as_slice());
    if let Some((_, _, rdata)) = answer {
        return Some(dns_message(id, query, 0, rdata));
    }
    let mut response = dns_message(id, query, 3, &[]);
    if state.dns_wrong_id {
        state.dns_wrong_id = false;
        if let Some(first) = response.first_mut() {
            *first ^= 0xff;
        }
    }
    if state.dns_truncate_udp && !tcp {
        state.dns_truncate_udp = false;
        if let Some(flag) = response.get_mut(2) {
            *flag |= 0x02;
        }
    }
    Some(response)
}

fn dns_message(id: [u8; 2], query: &[u8], rcode: u8, rdata: &[u8]) -> Vec<u8> {
    let question_end = question_end(query).unwrap_or(12);
    let question = query.get(12..question_end).unwrap_or(&[]);
    let answers = if rcode == 0 { 1u16 } else { 0 };
    let mut out = alloc::vec![
        id[0],
        id[1],
        0x81,
        0x80 | (rcode & 0x0f),
        0,
        1,
        (answers >> 8) as u8,
        answers as u8,
        0,
        0,
        0,
        0
    ];
    out.extend_from_slice(question);
    if rcode == 0 {
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&dns_qtype(query).unwrap_or(1).to_be_bytes());
        out.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }
    out
}

fn dns_name(query: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut position = 12usize;
    loop {
        let length = usize::from(*query.get(position)?);
        position = position.checked_add(1)?;
        if length == 0 {
            break;
        }
        let label = query.get(position..position.checked_add(length)?)?;
        out.extend_from_slice(label);
        out.push(b'.');
        position = position.checked_add(length)?;
    }
    while out.last() == Some(&b'.') {
        out.pop();
    }
    Some(out.to_ascii_lowercase())
}

fn question_end(query: &[u8]) -> Option<usize> {
    let mut position = 12usize;
    loop {
        let length = usize::from(*query.get(position)?);
        position = position.checked_add(1)?;
        if length == 0 {
            break;
        }
        position = position.checked_add(length)?;
    }
    position.checked_add(4)
}

fn dns_qtype(query: &[u8]) -> Option<u16> {
    let end = question_end(query)?;
    let start = end.checked_sub(4)?;
    let bytes = query.get(start..start.checked_add(2)?)?;
    Some(u16::from_be_bytes([*bytes.first()?, *bytes.get(1)?]))
}

#[cfg(test)]
#[path = "fake_ut.rs"]
mod tests;
