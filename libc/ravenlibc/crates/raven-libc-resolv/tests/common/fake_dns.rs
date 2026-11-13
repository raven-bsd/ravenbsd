// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A scripted DNS server used by the resolver integration tests.
//!
//! It implements every seam at once ([`FileSource`], [`DnsIo`], [`Clock`], [`Entropy`],
//! [`Probe`], [`IfNames`]), keeps the clock virtual (only `wait` advances it) and answers
//! queries from a rule table. Nothing touches a socket, a file or the real clock.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use raven_libc_nss::FileSource;
use raven_libc_resolv::{
    Clock, DnsIo, Entropy, Env, Family, IfNames, Interest, InterestKind, IpAddr, Probe, Recv,
    SockAddr, TcpSock, UdpSock,
};

/// The default fake server address.
pub const SERVER: [u8; 4] = [192, 0, 2, 53];

/// One resource record the fake can return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// An A record.
    A([u8; 4]),
    /// An AAAA record.
    Aaaa([u8; 16]),
    /// A CNAME record.
    Cname(String),
    /// A PTR record.
    Ptr(String),
}

/// What the fake does with a matching query.
#[derive(Clone, Debug)]
pub enum Action {
    /// A positive answer with the given records.
    Records(Vec<Record>),
    /// RCODE 0 and no answers.
    NoData,
    /// RCODE 3.
    NxDomain,
    /// RCODE 2.
    ServFail,
    /// RCODE 5.
    Refused,
    /// A UDP answer with TC set; the records are used on TCP.
    Truncated(Vec<Record>),
    /// No answer at all.
    Drop,
    /// A positive answer with the wrong query ID.
    WrongId(Vec<Record>),
    /// A positive answer whose question name is changed.
    WrongQuestion(Vec<Record>),
    /// A positive answer from an unconfigured source address.
    WrongSource(Vec<Record>),
    /// The positive answer is delivered twice.
    Duplicate(Vec<Record>),
    /// The answer is delayed by the given number of virtual milliseconds.
    Delay(u64, Vec<Record>),
    /// A TCP response whose 16-bit length field is `n` followed by `n` zero bytes.
    TcpLength(u16),
}

#[derive(Clone, Debug)]
struct Rule {
    name: String,
    qtype: u16,
    action: Action,
}

#[derive(Clone, Debug)]
struct SendRecord {
    at: u64,
    id: u16,
    name: String,
    qtype: u16,
    to: SockAddr,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct UdpEvent {
    at: u64,
    from: SockAddr,
    bytes: Vec<u8>,
    truncated: bool,
}

#[derive(Clone, Debug, Default)]
struct TcpConn {
    input: Vec<u8>,
    response: Vec<u8>,
    sent: usize,
    done: bool,
    server: Option<SockAddr>,
}

/// The scripted fake.
pub struct FakeDns {
    clock: Rc<Cell<u64>>,
    files: RefCell<BTreeMap<String, Result<Vec<u8>, i32>>>,
    rules: RefCell<Vec<Rule>>,
    server_rules: RefCell<Vec<(SockAddr, Rule)>>,
    default_action: RefCell<Action>,
    events: RefCell<Vec<UdpEvent>>,
    sends: RefCell<Vec<SendRecord>>,
    udp_opens: Cell<usize>,
    udp_closes: Cell<usize>,
    tcp_opens: Cell<usize>,
    tcp_closes: Cell<usize>,
    open_ports: RefCell<Vec<u16>>,
    next_port: Cell<u32>,
    tcp: RefCell<BTreeMap<u32, TcpConn>>,
    next_tcp: Cell<u32>,
    v6_usable: Cell<bool>,
    tcp_refused: Cell<bool>,
    tcp_chunk: Cell<usize>,
    entropy_failing: Cell<bool>,
    entropy_count: Cell<u8>,
    routes: RefCell<Vec<(IpAddr, Option<IpAddr>)>>,
    ifaces: RefCell<Vec<(u32, String)>>,
}

impl Default for FakeDns {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDns {
    /// A fake with an empty hosts file, a local resolv.conf and the IANA service subset.
    pub fn new() -> Self {
        let fake = Self {
            clock: Rc::new(Cell::new(0)),
            files: RefCell::new(BTreeMap::new()),
            rules: RefCell::new(Vec::new()),
            server_rules: RefCell::new(Vec::new()),
            default_action: RefCell::new(Action::NoData),
            events: RefCell::new(Vec::new()),
            sends: RefCell::new(Vec::new()),
            udp_opens: Cell::new(0),
            udp_closes: Cell::new(0),
            tcp_opens: Cell::new(0),
            tcp_closes: Cell::new(0),
            open_ports: RefCell::new(Vec::new()),
            next_port: Cell::new(0),
            tcp: RefCell::new(BTreeMap::new()),
            next_tcp: Cell::new(1),
            v6_usable: Cell::new(true),
            tcp_refused: Cell::new(false),
            tcp_chunk: Cell::new(4096),
            entropy_failing: Cell::new(false),
            entropy_count: Cell::new(0),
            routes: RefCell::new(Vec::new()),
            ifaces: RefCell::new(vec![(7, String::from("eth0"))]),
        };
        fake.set_hosts("");
        fake.set_resolv("nameserver 192.0.2.53\n");
        fake.set_services(
            "http\t80/tcp\twww\nhttp\t80/udp\nhttps\t443/tcp\ntftp\t69/udp\n\
             domain\t53/udp\ndomain\t53/tcp\necho\t7/tcp\necho\t7/udp\n",
        );
        fake
    }

    /// The environment bound to this fake.
    pub fn env(&self) -> Env<'_> {
        Env {
            files: self,
            io: self,
            clock: self,
            entropy: self,
            probe: self,
            ifnames: self,
        }
    }

    /// The default fake server.
    pub fn server() -> SockAddr {
        SockAddr::new(IpAddr::V4(SERVER), 53)
    }

    /// Replace `/etc/hosts`.
    pub fn set_hosts(&self, text: &str) {
        self.put_file("/etc/hosts", text.as_bytes());
    }

    /// Replace `/etc/resolv.conf`.
    pub fn set_resolv(&self, text: &str) {
        self.put_file("/etc/resolv.conf", text.as_bytes());
    }

    /// Replace `/etc/services`.
    pub fn set_services(&self, text: &str) {
        self.put_file("/etc/services", text.as_bytes());
    }

    /// Put a file.
    pub fn put_file(&self, path: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(String::from(path), Ok(bytes.to_vec()));
    }

    /// Make a file fail with an errno.
    pub fn fail_file(&self, path: &str, errno: i32) {
        self.files
            .borrow_mut()
            .insert(String::from(path), Err(errno));
    }

    /// Add a rule for a query name and type.
    pub fn add_rule(&self, name: &str, qtype: u16, action: Action) {
        self.rules.borrow_mut().push(Rule {
            name: String::from(name),
            qtype,
            action,
        });
    }

    /// Add a rule answered only by one server.
    pub fn add_server_rule(&self, server: SockAddr, name: &str, qtype: u16, action: Action) {
        self.server_rules.borrow_mut().push((
            server,
            Rule {
                name: String::from(name),
                qtype,
                action,
            },
        ));
    }

    /// The action for unmatched queries.
    pub fn set_default(&self, action: Action) {
        *self.default_action.borrow_mut() = action;
    }

    /// Whether a UDP socket for IPv6 can be created.
    pub fn set_v6_usable(&self, value: bool) {
        self.v6_usable.set(value);
    }

    /// Whether `open_tcp` is refused.
    pub fn set_tcp_refused(&self, value: bool) {
        self.tcp_refused.set(value);
    }

    /// The maximum bytes per `tcp_send`/`tcp_recv` call.
    pub fn set_tcp_chunk(&self, value: usize) {
        self.tcp_chunk.set(value.max(1));
    }

    /// Make `Entropy::fill` fail.
    pub fn set_entropy_failing(&self, value: bool) {
        self.entropy_failing.set(value);
    }

    /// Set the source route for a destination address.
    pub fn set_route(&self, dst: IpAddr, src: Option<IpAddr>) {
        self.routes.borrow_mut().push((dst, src));
    }

    /// Map an interface index to a name.
    pub fn set_iface(&self, index: u32, name: &str) {
        self.ifaces.borrow_mut().push((index, String::from(name)));
    }

    /// The virtual clock.
    pub fn now(&self) -> u64 {
        self.clock.get()
    }

    /// All recorded sends.
    pub fn sends(&self) -> usize {
        self.sends.borrow().len()
    }

    /// The virtual times of all sends.
    pub fn send_times(&self) -> Vec<u64> {
        self.sends.borrow().iter().map(|send| send.at).collect()
    }

    /// The query IDs of all sends.
    pub fn send_ids(&self) -> Vec<u16> {
        self.sends.borrow().iter().map(|send| send.id).collect()
    }

    /// The queried names in order.
    pub fn sent_names(&self) -> Vec<String> {
        self.sends
            .borrow()
            .iter()
            .map(|send| send.name.clone())
            .collect()
    }

    /// The queried types in order.
    pub fn sent_types(&self) -> Vec<u16> {
        self.sends.borrow().iter().map(|send| send.qtype).collect()
    }

    /// The byte lengths of the sent queries.
    pub fn sent_lengths(&self) -> Vec<usize> {
        self.sends
            .borrow()
            .iter()
            .map(|send| send.bytes.len())
            .collect()
    }

    /// Number of opened UDP sockets.
    pub fn udp_opens(&self) -> usize {
        self.udp_opens.get()
    }

    /// Number of closed UDP sockets.
    pub fn udp_closes(&self) -> usize {
        self.udp_closes.get()
    }

    /// Number of opened TCP sockets.
    pub fn tcp_opens(&self) -> usize {
        self.tcp_opens.get()
    }

    /// Number of closed TCP sockets.
    pub fn tcp_closes(&self) -> usize {
        self.tcp_closes.get()
    }

    /// The ephemeral ports seen by the fake.
    pub fn open_ports(&self) -> Vec<u16> {
        self.open_ports.borrow().clone()
    }

    /// Clear every counter and recorded send.
    pub fn reset_counters(&self) {
        self.sends.borrow_mut().clear();
        self.open_ports.borrow_mut().clear();
        self.udp_opens.set(0);
        self.udp_closes.set(0);
        self.tcp_opens.set(0);
        self.tcp_closes.set(0);
    }

    fn action_for(&self, server: &SockAddr, name: &str, qtype: u16) -> Action {
        for (from, rule) in self.server_rules.borrow().iter() {
            if from == server && rule.qtype == qtype && rule.name.eq_ignore_ascii_case(name) {
                return rule.action.clone();
            }
        }
        for rule in self.rules.borrow().iter() {
            if rule.qtype == qtype && rule.name.eq_ignore_ascii_case(name) {
                return rule.action.clone();
            }
        }
        self.default_action.borrow().clone()
    }

    fn schedule(&self, from: SockAddr, bytes: Vec<u8>, at: u64, truncated: bool) {
        self.events.borrow_mut().push(UdpEvent {
            at,
            from,
            bytes,
            truncated,
        });
    }

    fn next_port(&self) -> u16 {
        let value = self.next_port.get();
        self.next_port.set(value.wrapping_add(1));
        let port = 49152u32.wrapping_add(value.wrapping_mul(977) % 16384);
        let port = u16::try_from(port).unwrap_or(49152);
        self.open_ports.borrow_mut().push(port);
        port
    }
}

/// Read one byte, or zero.
fn byte(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or(0)
}

/// The qtype and question name of a query.
fn parse_question(query: &[u8]) -> (String, u16) {
    let mut name = String::new();
    let mut p = 12usize;
    loop {
        let Some(&length) = query.get(p) else {
            return (name, 0);
        };
        if length == 0 {
            p = p.saturating_add(1);
            break;
        }
        if length & 0xc0 != 0 {
            p = p.saturating_add(2);
            break;
        }
        let start = p.saturating_add(1);
        let end = start.saturating_add(usize::from(length));
        let Some(label) = query.get(start..end) else {
            return (name, 0);
        };
        if !name.is_empty() {
            name.push('.');
        }
        name.push_str(&String::from_utf8_lossy(label));
        p = end;
    }
    let qtype = u16::from(byte(query, p)) << 8 | u16::from(byte(query, p.saturating_add(1)));
    (name, qtype)
}

/// The byte just past the question section.
fn question_end(query: &[u8]) -> usize {
    let mut p = 12usize;
    loop {
        let Some(&length) = query.get(p) else {
            return query.len();
        };
        if length == 0 {
            return p.saturating_add(5).min(query.len());
        }
        if length & 0xc0 != 0 {
            return p.saturating_add(6).min(query.len());
        }
        p = p.saturating_add(1).saturating_add(usize::from(length));
    }
}

/// Encode a name without compression.
fn encode_name(out: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        let length = u8::try_from(bytes.len().min(63)).unwrap_or(63);
        out.push(length);
        out.extend_from_slice(bytes.get(..usize::from(length)).unwrap_or(&[]));
    }
    out.push(0);
}

fn rdata(record: &Record) -> (u16, Vec<u8>) {
    match record {
        Record::A(bytes) => (1, bytes.to_vec()),
        Record::Aaaa(bytes) => (28, bytes.to_vec()),
        Record::Cname(name) => {
            let mut out = Vec::new();
            encode_name(&mut out, name);
            (5, out)
        }
        Record::Ptr(name) => {
            let mut out = Vec::new();
            encode_name(&mut out, name);
            (12, out)
        }
    }
}

fn build_response(
    query: &[u8],
    rcode: u8,
    records: &[Record],
    wrong_id: bool,
    wrong_question: bool,
) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(512);
    let original = u16::from(byte(query, 0)).wrapping_shl(8) | u16::from(byte(query, 1));
    let id = if wrong_id { original ^ 0xff } else { original };
    out.push(u8::try_from(id >> 8).unwrap_or(0));
    out.push(u8::try_from(id & 0xff).unwrap_or(0));
    out.push(0x81);
    out.push(0x80 | (rcode & 15));
    out.push(0);
    out.push(1);
    let ancount = u16::try_from(records.len()).unwrap_or(0);
    out.push(u8::try_from(ancount >> 8).unwrap_or(0));
    out.push(u8::try_from(ancount & 0xff).unwrap_or(0));
    out.push(0);
    out.push(0);
    out.push(0);
    out.push(0);
    let end = question_end(query).max(12);
    let mut question = query.get(12..end).unwrap_or(&[]).to_vec();
    if wrong_question
        && question.first().is_some_and(|slot| *slot > 0 && *slot < 64)
        && let Some(letter) = question.get_mut(1)
    {
        // Change the first label byte after the length.
        *letter = if *letter == b'Q' { b'Z' } else { b'Q' };
    }
    out.extend_from_slice(&question);
    for record in records.iter() {
        out.extend_from_slice(&[0xc0, 0x0c]);
        let (rr_type, data) = rdata(record);
        out.extend_from_slice(&rr_type.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&60u32.to_be_bytes());
        out.extend_from_slice(&u16::try_from(data.len()).unwrap_or(0).to_be_bytes());
        out.extend_from_slice(&data);
    }
    out
}

fn tcp_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(u8::try_from(payload.len() >> 8).unwrap_or(0));
    out.push(u8::try_from(payload.len() & 0xff).unwrap_or(0));
    out.extend_from_slice(payload);
    out
}

impl FileSource for FakeDns {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, i32> {
        match self.files.borrow().get(path) {
            Some(Ok(bytes)) => Ok(bytes.clone()),
            Some(Err(errno)) => Err(*errno),
            None => Err(raven_libc_abi::errno::ENOENT),
        }
    }
}

impl DnsIo for FakeDns {
    fn open_udp(&self, _family: Family) -> Result<UdpSock, i32> {
        self.udp_opens.set(self.udp_opens.get() + 1);
        // A fresh socket cannot see datagrams queued for an earlier lookup.
        self.events.borrow_mut().clear();
        let port = self.next_port();
        Ok(UdpSock(u32::from(port)))
    }

    fn send_to(&self, _s: UdpSock, buf: &[u8], to: &SockAddr) -> Result<usize, i32> {
        let (name, qtype) = parse_question(buf);
        let id = u16::from(byte(buf, 0)) << 8 | u16::from(byte(buf, 1));
        self.sends.borrow_mut().push(SendRecord {
            at: self.clock.get(),
            id,
            name: name.clone(),
            qtype,
            to: *to,
            bytes: buf.to_vec(),
        });
        let action = self.action_for(to, &name, qtype);
        let now = self.clock.get();
        match action {
            Action::Drop => {}
            Action::Records(records) => {
                self.schedule(
                    *to,
                    build_response(buf, 0, &records, false, false),
                    now,
                    false,
                );
            }
            Action::WrongId(records) => {
                self.schedule(
                    *to,
                    build_response(buf, 0, &records, true, false),
                    now,
                    false,
                );
            }
            Action::WrongQuestion(records) => {
                self.schedule(
                    *to,
                    build_response(buf, 0, &records, false, true),
                    now,
                    false,
                );
            }
            Action::WrongSource(records) => {
                let from = SockAddr::new(IpAddr::V4([198, 51, 100, 99]), 53);
                self.schedule(
                    from,
                    build_response(buf, 0, &records, false, false),
                    now,
                    false,
                );
            }
            Action::Duplicate(records) => {
                let response = build_response(buf, 0, &records, false, false);
                self.schedule(*to, response.clone(), now, false);
                self.schedule(*to, response, now, false);
            }
            Action::Delay(ms, records) => {
                self.schedule(
                    *to,
                    build_response(buf, 0, &records, false, false),
                    now.saturating_add(ms),
                    false,
                );
            }
            Action::NoData => {
                self.schedule(*to, build_response(buf, 0, &[], false, false), now, false);
            }
            Action::NxDomain => {
                self.schedule(*to, build_response(buf, 3, &[], false, false), now, false);
            }
            Action::ServFail => {
                self.schedule(*to, build_response(buf, 2, &[], false, false), now, false);
            }
            Action::Refused => {
                self.schedule(*to, build_response(buf, 5, &[], false, false), now, false);
            }
            Action::Truncated(_) | Action::TcpLength(_) => {
                self.schedule(*to, build_response(buf, 0, &[], false, false), now, true);
            }
        }
        Ok(buf.len())
    }

    fn recv_from(&self, _s: UdpSock, buf: &mut [u8]) -> Result<Recv, i32> {
        let now = self.clock.get();
        let mut events = self.events.borrow_mut();
        let index = events.iter().position(|event| event.at <= now);
        let Some(index) = index else {
            return Err(raven_libc_abi::errno::EAGAIN);
        };
        let event = events.remove(index);
        let len = event.bytes.len().min(buf.len());
        if let Some(target) = buf.get_mut(..len) {
            target.copy_from_slice(event.bytes.get(..len).unwrap_or(&[]));
        }
        Ok(Recv {
            len,
            from: event.from,
            truncated: event.truncated || event.bytes.len() > buf.len(),
        })
    }

    fn open_tcp(&self, to: &SockAddr) -> Result<TcpSock, i32> {
        if self.tcp_refused.get() {
            return Err(raven_libc_abi::errno::ECONNREFUSED);
        }
        self.tcp_opens.set(self.tcp_opens.get() + 1);
        let id = self.next_tcp.get();
        self.next_tcp.set(id.wrapping_add(1));
        self.tcp.borrow_mut().insert(
            id,
            TcpConn {
                server: Some(*to),
                ..TcpConn::default()
            },
        );
        Ok(TcpSock(id))
    }

    fn tcp_send(&self, s: TcpSock, buf: &[u8]) -> Result<usize, i32> {
        let mut map = self.tcp.borrow_mut();
        let Some(conn) = map.get_mut(&s.0) else {
            return Err(raven_libc_abi::errno::EINVAL);
        };
        if conn.done {
            return Ok(0);
        }
        let take = buf.len().min(self.tcp_chunk.get());
        conn.input.extend_from_slice(buf.get(..take).unwrap_or(&[]));
        if conn.input.len() >= 2 {
            let length = usize::from(byte(&conn.input, 0)) << 8 | usize::from(byte(&conn.input, 1));
            if conn.input.len() >= length.saturating_add(2) {
                let query = conn.input.get(2..2 + length).unwrap_or(&[]).to_vec();
                let (name, qtype) = parse_question(&query);
                let action = match &conn.server {
                    Some(server) => self.action_for(server, &name, qtype),
                    None => Action::NoData,
                };
                let records = match &action {
                    Action::Records(records)
                    | Action::WrongId(records)
                    | Action::WrongQuestion(records)
                    | Action::WrongSource(records)
                    | Action::Duplicate(records)
                    | Action::Delay(_, records)
                    | Action::Truncated(records) => records.clone(),
                    _ => Vec::new(),
                };
                match action {
                    Action::TcpLength(length) => {
                        let mut response = Vec::new();
                        response.push(u8::try_from(length >> 8).unwrap_or(0));
                        response.push(u8::try_from(length & 0xff).unwrap_or(0));
                        response.extend(std::iter::repeat_n(0u8, usize::from(length)));
                        conn.response = response;
                    }
                    _ => {
                        let response = build_response(&query, 0, &records, false, false);
                        conn.response = tcp_frame(&response);
                    }
                }
                conn.done = true;
            }
        }
        Ok(take)
    }

    fn tcp_recv(&self, s: TcpSock, buf: &mut [u8]) -> Result<usize, i32> {
        let mut map = self.tcp.borrow_mut();
        let Some(conn) = map.get_mut(&s.0) else {
            return Err(raven_libc_abi::errno::EINVAL);
        };
        if !conn.done {
            return Err(raven_libc_abi::errno::EAGAIN);
        }
        if conn.sent >= conn.response.len() {
            return Ok(0);
        }
        let take = (conn.response.len() - conn.sent)
            .min(buf.len())
            .min(self.tcp_chunk.get());
        if let (Some(target), Some(source)) = (
            buf.get_mut(..take),
            conn.response.get(conn.sent..conn.sent + take),
        ) {
            target.copy_from_slice(source);
        }
        conn.sent += take;
        Ok(take)
    }

    fn wait(&self, interests: &mut [Interest], timeout_ms: u64) -> Result<(), i32> {
        let now = self.clock.get();
        let due = self
            .events
            .borrow()
            .iter()
            .map(|event| event.at)
            .filter(|at| *at >= now)
            .min();
        let tcp_progress = {
            let tcp = self.tcp.borrow();
            interests.iter().any(|interest| match interest.kind {
                InterestKind::Tcp(sock) => match tcp.get(&sock.0) {
                    Some(_) if interest.write => true,
                    Some(conn) => conn.sent < conn.response.len() || conn.done,
                    None => false,
                },
                InterestKind::Udp(_) => false,
            })
        };
        let target = if tcp_progress {
            now
        } else {
            match due {
                Some(at) if at <= now.saturating_add(timeout_ms) => at,
                _ => now.saturating_add(timeout_ms),
            }
        };
        self.clock.set(target);
        let udp_ready = self.events.borrow().iter().any(|event| event.at <= target);
        let tcp = self.tcp.borrow();
        for interest in interests.iter_mut() {
            match interest.kind {
                InterestKind::Udp(_) => interest.ready_r = udp_ready,
                InterestKind::Tcp(sock) => {
                    if interest.write {
                        interest.ready_w = true;
                    } else if let Some(conn) = tcp.get(&sock.0) {
                        interest.ready_r = conn.sent < conn.response.len() || conn.done;
                    }
                }
            }
        }
        Ok(())
    }

    fn close_udp(&self, _s: UdpSock) {
        self.udp_closes.set(self.udp_closes.get() + 1);
    }

    fn close_tcp(&self, s: TcpSock) {
        self.tcp_closes.set(self.tcp_closes.get() + 1);
        self.tcp.borrow_mut().remove(&s.0);
    }
}

impl Clock for FakeDns {
    fn monotonic_ms(&self) -> u64 {
        self.clock.get()
    }
}

impl Entropy for FakeDns {
    fn fill(&self, buf: &mut [u8]) -> bool {
        if self.entropy_failing.get() {
            return false;
        }
        for slot in buf.iter_mut() {
            let value = self.entropy_count.get();
            self.entropy_count.set(value.wrapping_add(1));
            *slot = value;
        }
        true
    }
}

impl Probe for FakeDns {
    fn udp_route(&self, dst: &SockAddr) -> Option<IpAddr> {
        for (address, route) in self.routes.borrow().iter() {
            if *address == dst.ip {
                return *route;
            }
        }
        match dst.ip {
            IpAddr::V4(_) => Some(IpAddr::V4([0, 0, 0, 0])),
            IpAddr::V6(_) => None,
        }
    }

    fn family_usable(&self, family: Family) -> bool {
        match family {
            Family::V4 => true,
            Family::V6 => self.v6_usable.get(),
        }
    }
}

impl IfNames for FakeDns {
    fn name_to_index(&self, name: &str) -> Option<u32> {
        self.ifaces
            .borrow()
            .iter()
            .find(|(_, candidate)| candidate == name)
            .map(|(index, _)| *index)
    }

    fn index_to_name(&self, index: u32) -> Option<String> {
        self.ifaces
            .borrow()
            .iter()
            .find(|(candidate, _)| *candidate == index)
            .map(|(_, name)| name.clone())
    }
}
