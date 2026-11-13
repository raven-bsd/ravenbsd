// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the multi-query engine: timing, acceptance rules and socket lifetime.

use super::*;
use crate::types::Recv;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use raven_libc_abi::errno::{EAGAIN, ECONNREFUSED, EINVAL, ENOENT};
use raven_libc_nss::FileSource;

struct Files {
    data: BTreeMap<String, Vec<u8>>,
}

impl Files {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(String::from(path), bytes.to_vec());
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, crate::Errno> {
        self.data.get(path).cloned().ok_or(ENOENT)
    }
}

#[derive(Clone)]
struct Event {
    at: u64,
    from: SockAddr,
    bytes: Vec<u8>,
    truncated: bool,
}

struct Io {
    now: alloc::rc::Rc<Cell<u64>>,
    reply: RefCell<Option<(SockAddr, Vec<u8>, bool)>>,
    delay: Cell<u64>,
    events: RefCell<Vec<Event>>,
    sends: RefCell<Vec<(u64, Vec<u8>)>>,
    opens: Cell<usize>,
    closes: Cell<usize>,
    next_id: Cell<u32>,
}

impl Io {
    fn new(now: alloc::rc::Rc<Cell<u64>>) -> Self {
        Self {
            now,
            reply: RefCell::new(None),
            delay: Cell::new(0),
            events: RefCell::new(Vec::new()),
            sends: RefCell::new(Vec::new()),
            opens: Cell::new(0),
            closes: Cell::new(0),
            next_id: Cell::new(1),
        }
    }

    fn set_reply(&self, from: SockAddr, bytes: Vec<u8>, truncated: bool) {
        *self.reply.borrow_mut() = Some((from, bytes, truncated));
    }
}

impl crate::traits::DnsIo for Io {
    fn open_udp(&self, _family: Family) -> Result<UdpSock, crate::Errno> {
        self.opens.set(self.opens.get() + 1);
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        Ok(UdpSock(id))
    }

    fn send_to(&self, _s: UdpSock, buf: &[u8], _to: &SockAddr) -> Result<usize, crate::Errno> {
        self.sends.borrow_mut().push((self.now.get(), buf.to_vec()));
        if let Some((from, bytes, truncated)) = self.reply.borrow().clone() {
            self.events.borrow_mut().push(Event {
                at: self.now.get().saturating_add(self.delay.get()),
                from,
                bytes,
                truncated,
            });
        }
        Ok(buf.len())
    }

    fn recv_from(&self, _s: UdpSock, buf: &mut [u8]) -> Result<Recv, crate::Errno> {
        let mut events = self.events.borrow_mut();
        let index = events.iter().position(|event| event.at <= self.now.get());
        let Some(index) = index else {
            return Err(EAGAIN);
        };
        let event = events.remove(index);
        let len = event.bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&event.bytes[..len]);
        Ok(Recv {
            len,
            from: event.from,
            truncated: event.truncated,
        })
    }

    fn open_tcp(&self, _to: &SockAddr) -> Result<TcpSock, crate::Errno> {
        Err(ECONNREFUSED)
    }
    fn tcp_send(&self, _s: TcpSock, _buf: &[u8]) -> Result<usize, crate::Errno> {
        Ok(0)
    }
    fn tcp_recv(&self, _s: TcpSock, _buf: &mut [u8]) -> Result<usize, crate::Errno> {
        Ok(0)
    }

    fn wait(&self, interests: &mut [Interest], timeout_ms: u64) -> Result<(), crate::Errno> {
        let now = self.now.get();
        let due = self
            .events
            .borrow()
            .iter()
            .map(|event| event.at)
            .filter(|at| *at >= now)
            .min();
        let target = match due {
            Some(at) if at <= now.saturating_add(timeout_ms) => at,
            _ => now.saturating_add(timeout_ms),
        };
        self.now.set(target);
        let ready = self.events.borrow().iter().any(|event| event.at <= target);
        for interest in interests.iter_mut() {
            if matches!(interest.kind, crate::types::InterestKind::Udp(_)) {
                interest.ready_r = ready;
            }
        }
        Ok(())
    }

    fn close_udp(&self, _s: UdpSock) {
        self.closes.set(self.closes.get() + 1);
    }
    fn close_tcp(&self, _s: TcpSock) {}
}

struct FixedClock(alloc::rc::Rc<Cell<u64>>);

impl crate::traits::Clock for FixedClock {
    fn monotonic_ms(&self) -> u64 {
        self.0.get()
    }
}

struct Entropy(bool);

impl crate::traits::Entropy for Entropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        if self.0 {
            for (index, slot) in buf.iter_mut().enumerate() {
                *slot = u8::try_from(index & 0xff).unwrap_or(0);
            }
        }
        self.0
    }
}

struct Probe;

impl crate::traits::Probe for Probe {
    fn udp_route(&self, _dst: &SockAddr) -> Option<IpAddr> {
        Some(IpAddr::V4([127, 0, 0, 1]))
    }
    fn family_usable(&self, family: Family) -> bool {
        family == Family::V4
    }
}

struct Names;

impl crate::traits::IfNames for Names {
    fn name_to_index(&self, _name: &str) -> Option<u32> {
        None
    }
    fn index_to_name(&self, _idx: u32) -> Option<String> {
        None
    }
}

fn query(id: u16, name: &[u8]) -> Vec<u8> {
    let mut buf = [0u8; 280];
    let len = crate::res_mkquery(0, name, 1, 1, id, &mut buf).expect("query");
    buf[..len].to_vec()
}

fn response(request: &[u8], a: [u8; 4]) -> Vec<u8> {
    let mut packet = request.to_vec();
    packet[2] |= 0x80;
    packet[3] = 0x80;
    packet[6] = 0;
    packet[7] = 1;
    packet.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01]);
    packet.extend_from_slice(&60u32.to_be_bytes());
    packet.extend_from_slice(&4u16.to_be_bytes());
    packet.extend_from_slice(&a);
    packet
}

fn conf() -> crate::conf::ResolvConf {
    let mut conf = crate::conf::ResolvConf::defaults();
    conf.timeout = 2;
    conf.attempts = 4;
    conf.ns = [
        Some(SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53)),
        None,
        None,
    ];
    conf.nns = 1;
    conf
}

fn run(
    io: &Io,
    queries: &[Vec<u8>],
    answers_in: &mut [Vec<u8>],
    alens: &mut [usize],
) -> Result<(), crate::Errno> {
    let clock = FixedClock(io.now.clone());
    let files = Files::new();
    let entropy = Entropy(true);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let query_slices: Vec<&[u8]> = queries.iter().map(Vec::as_slice).collect();
    let mut answer_slices: Vec<&mut [u8]> = answers_in.iter_mut().map(Vec::as_mut_slice).collect();
    msend(
        &env,
        &conf(),
        &ConfExt::default(),
        &query_slices,
        &mut answer_slices,
        alens,
    )
}

#[test]
fn the_send_schedule_is_zero_five_hundred_one_thousand_fifteen_hundred() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now.clone());
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[query(1, b"example.org")], &mut answers, &mut alens).expect("engine");
    let sends: Vec<u64> = io.sends.borrow().iter().map(|(at, _)| *at).collect();
    assert_eq!(sends, alloc::vec![0, 500, 1000, 1500]);
    assert_eq!(now.get(), 2000);
    assert_eq!(alens[0], 0);
}

#[test]
fn no_reply_does_not_error() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    assert!(run(&io, &[query(1, b"example.org")], &mut answers, &mut alens).is_ok());
}

#[test]
fn a_matching_answer_is_accepted() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let request = query(0x1234, b"example.org");
    let server = SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53);
    io.set_reply(server, response(&request, [192, 0, 2, 7]), false);
    let expected = [request[0], request[1], 0x81, 0x80];
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[request], &mut answers, &mut alens).expect("engine");
    assert_eq!(alens[0], 45);
    assert_eq!(&answers[0][..4], &expected);
}

#[test]
fn a_wrong_id_is_ignored() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let request = query(0x1234, b"example.org");
    let mut reply = response(&request, [192, 0, 2, 7]);
    reply[1] ^= 0xff;
    io.set_reply(SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53), reply, false);
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[request], &mut answers, &mut alens).expect("engine");
    assert_eq!(alens[0], 0);
}

#[test]
fn a_reply_from_an_unconfigured_server_is_ignored() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let request = query(0x1234, b"example.org");
    io.set_reply(
        SockAddr::new(IpAddr::V4([198, 51, 100, 53]), 53),
        response(&request, [192, 0, 2, 7]),
        false,
    );
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[request], &mut answers, &mut alens).expect("engine");
    assert_eq!(alens[0], 0);
}

#[test]
fn a_reply_with_a_different_question_is_ignored() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let request = query(0x1234, b"example.org");
    let mut reply = response(&request, [192, 0, 2, 7]);
    reply[13] = b'Q';
    io.set_reply(SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53), reply, false);
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[request], &mut answers, &mut alens).expect("engine");
    assert_eq!(alens[0], 0);
}

#[test]
fn the_socket_is_closed_after_a_success() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let request = query(0x1234, b"example.org");
    io.set_reply(
        SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53),
        response(&request, [192, 0, 2, 7]),
        false,
    );
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[request], &mut answers, &mut alens).expect("engine");
    assert_eq!(io.opens.get(), 1);
    assert_eq!(io.closes.get(), 1);
}

#[test]
fn the_socket_is_closed_after_a_timeout() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let mut answers = alloc::vec![alloc::vec![0u8; 512]];
    let mut alens = [0usize; 1];
    run(&io, &[query(1, b"example.org")], &mut answers, &mut alens).expect("engine");
    assert_eq!(io.opens.get(), 1);
    assert_eq!(io.closes.get(), 1);
}

#[test]
fn every_lookup_opens_its_own_socket() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    for _ in 0..3 {
        let mut answers = alloc::vec![alloc::vec![0u8; 512]];
        let mut alens = [0usize; 1];
        run(&io, &[query(1, b"example.org")], &mut answers, &mut alens).expect("engine");
    }
    assert_eq!(io.opens.get(), 3);
    assert_eq!(io.closes.get(), 3);
}

#[test]
fn zero_attempts_is_invalid() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let clock = FixedClock(io.now.clone());
    let files = Files::new();
    let entropy = Entropy(true);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let request = query(1, b"example.org");
    let queries: [&[u8]; 1] = [&request];
    let mut answer = alloc::vec![0u8; 512];
    let mut answers: [&mut [u8]; 1] = [answer.as_mut_slice()];
    let mut alens = [0usize; 1];
    let mut bad = conf();
    bad.attempts = 0;
    assert_eq!(
        msend(
            &env,
            &bad,
            &ConfExt::default(),
            &queries,
            &mut answers,
            &mut alens
        ),
        Err(EINVAL)
    );
}

#[test]
fn mismatched_answer_slices_are_invalid() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let clock = FixedClock(io.now.clone());
    let files = Files::new();
    let entropy = Entropy(true);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let request = query(1, b"example.org");
    let queries: [&[u8]; 2] = [&request, &request];
    let mut answers: [&mut [u8]; 1] = [&mut [0u8; 512]];
    let mut alens = [0usize; 2];
    assert_eq!(
        msend(
            &env,
            &conf(),
            &ConfExt::default(),
            &queries,
            &mut answers,
            &mut alens
        ),
        Err(EINVAL)
    );
}

#[test]
fn only_ipv6_servers_without_a_usable_family_is_eafnosupport() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let clock = FixedClock(io.now.clone());
    let files = Files::new();
    let entropy = Entropy(true);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let request = query(1, b"example.org");
    let queries: [&[u8]; 1] = [&request];
    let mut answer = alloc::vec![0u8; 512];
    let mut answers: [&mut [u8]; 1] = [answer.as_mut_slice()];
    let mut alens = [0usize; 1];
    let mut conf_v6 = conf();
    conf_v6.ns = [
        Some(SockAddr::new(IpAddr::V6(crate::inet::IN6ADDR_LOOPBACK), 53)),
        None,
        None,
    ];
    conf_v6.nns = 1;
    assert_eq!(
        msend(
            &env,
            &conf_v6,
            &ConfExt::default(),
            &queries,
            &mut answers,
            &mut alens
        ),
        Err(EAFNOSUPPORT)
    );
}

#[test]
fn query_type_parses_the_question() {
    let packet = query(1, b"example.org");
    assert_eq!(query_type(&packet), Some(1));
    assert_eq!(query_type(&[0u8; 11]), None);
}

#[test]
fn no_aaaa_suppresses_the_query() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let clock = FixedClock(io.now.clone());
    let files = Files::new();
    let entropy = Entropy(true);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let mut buf = [0u8; 280];
    let len = crate::res_mkquery(0, b"example.org", 1, 28, 1, &mut buf).expect("query");
    let request = buf[..len].to_vec();
    let queries: [&[u8]; 1] = [&request];
    let mut answer = alloc::vec![0u8; 512];
    let mut answers: [&mut [u8]; 1] = [answer.as_mut_slice()];
    let mut alens = [0usize; 1];
    let ext = ConfExt {
        no_aaaa: true,
        ..ConfExt::default()
    };
    msend(&env, &conf(), &ext, &queries, &mut answers, &mut alens).expect("engine");
    assert!(io.sends.borrow().is_empty());
}

#[test]
fn an_entropy_failure_fails_the_lookup_without_a_socket() {
    let now = alloc::rc::Rc::new(Cell::new(0u64));
    let io = Io::new(now);
    let clock = FixedClock(io.now.clone());
    let mut files = Files::new();
    files.put("/etc/hosts", b"");
    files.put("/etc/resolv.conf", b"nameserver 192.0.2.53\n");
    let entropy = Entropy(false);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    let order = crate::lookup::HostOrder::defaults();
    let result =
        crate::lookup::lookup_name(Some("example.org"), crate::consts::AF_INET, 0, &env, &order);
    assert_eq!(result, Err(crate::errors::Eai::Fail));
    assert_eq!(io.opens.get(), 0);
    assert!(io.sends.borrow().is_empty());
}
