// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the socket/resolver adapter with fixed mock streams.

use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use core::net::Ipv4Addr;

use raven_http::{Client, Clock, Error, RequestBuilder};

use super::HttpTransport;
use crate::{ResolveError, Resolved, Resolver, Socket, SocketError};

struct MockResolver {
    fail: Option<ResolveError>,
    calls: Vec<String>,
}

impl MockResolver {
    fn new() -> Self {
        Self {
            fail: None,
            calls: Vec::new(),
        }
    }
}

impl Resolver for MockResolver {
    fn resolve(&mut self, host: &str, _timeout_ms: u32) -> Result<Resolved, ResolveError> {
        self.calls.push(String::from(host));
        if let Some(error) = self.fail {
            return Err(error);
        }
        let address = match host {
            "a.test" => Ipv4Addr::new(192, 0, 2, 1),
            "b.test" => Ipv4Addr::new(192, 0, 2, 2),
            _ => Ipv4Addr::new(192, 0, 2, 9),
        };
        Ok(Resolved {
            address,
            ttl_secs: 60,
        })
    }
}

struct MockSocket {
    scripts: Vec<Vec<u8>>,
    data: Vec<u8>,
    position: usize,
    chunk: usize,
    connects: Vec<(Ipv4Addr, u16)>,
    writes: Vec<Vec<u8>>,
    closes: Rc<Cell<usize>>,
    recv_calls: usize,
    connect_error: Option<SocketError>,
    send_error: Option<SocketError>,
    recv_error: Option<SocketError>,
}

impl MockSocket {
    fn new(scripts: Vec<Vec<u8>>) -> Self {
        Self {
            scripts,
            data: Vec::new(),
            position: 0,
            chunk: 7,
            connects: Vec::new(),
            writes: Vec::new(),
            closes: Rc::new(Cell::new(0)),
            recv_calls: 0,
            connect_error: None,
            send_error: None,
            recv_error: None,
        }
    }

    fn sent(&self, index: usize) -> String {
        String::from_utf8(self.writes[index].clone()).expect("utf-8 request")
    }
}

impl Socket for MockSocket {
    fn connect(
        &mut self,
        address: Ipv4Addr,
        port: u16,
        _timeout_ms: u32,
    ) -> Result<(), SocketError> {
        self.connects.push((address, port));
        if let Some(error) = self.connect_error {
            return Err(error);
        }
        self.position = 0;
        self.data = self
            .scripts
            .get(self.connects.len().saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        self.writes.push(Vec::new());
        Ok(())
    }

    fn send(&mut self, bytes: &[u8], _timeout_ms: u32) -> Result<usize, SocketError> {
        if let Some(error) = self.send_error {
            return Err(error);
        }
        let Some(write) = self.writes.last_mut() else {
            return Err(SocketError::Io);
        };
        write.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn recv(&mut self, bytes: &mut [u8], _timeout_ms: u32) -> Result<usize, SocketError> {
        self.recv_calls += 1;
        if let Some(error) = self.recv_error {
            return Err(error);
        }
        let left = self.data.len().saturating_sub(self.position);
        let count = left.min(bytes.len()).min(self.chunk);
        bytes[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }

    fn close(&mut self) -> Result<(), SocketError> {
        self.closes.set(self.closes.get() + 1);
        Ok(())
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        0
    }
}

fn response(body: &[u8]) -> Vec<u8> {
    let mut out =
        alloc::format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}

fn redirect(location: &str) -> Vec<u8> {
    alloc::format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n")
        .into_bytes()
}

fn request(url: &str) -> raven_http::Request {
    RequestBuilder::new(url).expect("builder").build()
}

#[test]
fn resolves_connects_writes_and_reads() {
    let socket = MockSocket::new(vec![response(b"ok")]);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let response = {
        let mut client = Client::new(&mut transport, &clock);
        client.get(&request("http://a.test/status")).expect("get")
    };
    assert_eq!(response.body, b"ok");
    assert_eq!(transport.resolver().calls, vec![String::from("a.test")]);
    assert_eq!(
        transport.socket().connects,
        vec![(Ipv4Addr::new(192, 0, 2, 1), 80)]
    );
    assert!(
        transport
            .socket()
            .sent(0)
            .starts_with("GET /status HTTP/1.1\r\n")
    );
}

#[test]
fn ipv4_literal_skips_the_resolver() {
    let socket = MockSocket::new(vec![response(b"ok")]);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    {
        let mut client = Client::new(&mut transport, &clock);
        client.get(&request("http://192.0.2.5:8080/")).expect("get");
    }
    assert!(transport.resolver().calls.is_empty());
    assert_eq!(
        transport.socket().connects,
        vec![(Ipv4Addr::new(192, 0, 2, 5), 8080)]
    );
}

#[test]
fn maps_resolver_failures_to_transport_errors() {
    let socket = MockSocket::new(vec![response(b"ok")]);
    let mut resolver = MockResolver::new();
    resolver.fail = Some(ResolveError::NotFound);
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    {
        let mut client = Client::new(&mut transport, &clock);
        assert_eq!(
            client.get(&request("http://a.test/")),
            Err(Error::Transport)
        );
    }
    assert!(transport.socket().connects.is_empty());
}

#[test]
fn maps_a_resolver_timeout() {
    let socket = MockSocket::new(vec![response(b"ok")]);
    let mut resolver = MockResolver::new();
    resolver.fail = Some(ResolveError::Timeout);
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let mut client = Client::new(&mut transport, &clock);
    assert_eq!(client.get(&request("http://a.test/")), Err(Error::Timeout));
}

#[test]
fn maps_a_connect_failure() {
    let mut socket = MockSocket::new(vec![response(b"ok")]);
    socket.connect_error = Some(SocketError::Io);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let mut client = Client::new(&mut transport, &clock);
    assert_eq!(
        client.get(&request("http://a.test/")),
        Err(Error::Transport)
    );
}

#[test]
fn maps_a_socket_timeout() {
    let mut socket = MockSocket::new(vec![response(b"ok")]);
    socket.recv_error = Some(SocketError::Timeout);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let mut client = Client::new(&mut transport, &clock);
    assert_eq!(client.get(&request("http://a.test/")), Err(Error::Timeout));
}

#[test]
fn maps_a_write_failure() {
    let mut socket = MockSocket::new(vec![response(b"ok")]);
    socket.send_error = Some(SocketError::Io);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let mut client = Client::new(&mut transport, &clock);
    assert_eq!(
        client.get(&request("http://a.test/")),
        Err(Error::Transport)
    );
}

#[test]
fn reassembles_partial_reads_and_a_clean_close() {
    let mut socket = MockSocket::new(vec![response(b"partial-body")]);
    socket.chunk = 1;
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let response = {
        let mut client = Client::new(&mut transport, &clock);
        client.get(&request("http://a.test/")).expect("get")
    };
    assert_eq!(response.body, b"partial-body");
    assert!(transport.socket().recv_calls > 10);
}

#[test]
fn caps_the_body_through_the_adapter() {
    let socket = MockSocket::new(vec![response(b"0123456789")]);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let mut client = Client::new(&mut transport, &clock);
    let request = RequestBuilder::new("http://a.test/")
        .expect("builder")
        .max_body(8)
        .build();
    assert_eq!(client.get(&request), Err(Error::BodyLimit));
}

#[test]
fn follows_a_redirect_to_another_host_and_reconnects() {
    let socket = MockSocket::new(vec![redirect("http://b.test/next"), response(b"done")]);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    let response = {
        let mut client = Client::new(&mut transport, &clock);
        client.get(&request("http://a.test/start")).expect("get")
    };
    assert_eq!(response.body, b"done");
    assert_eq!(
        transport.resolver().calls,
        vec![String::from("a.test"), String::from("b.test")]
    );
    assert_eq!(
        transport.socket().connects,
        vec![
            (Ipv4Addr::new(192, 0, 2, 1), 80),
            (Ipv4Addr::new(192, 0, 2, 2), 80)
        ]
    );
    assert!(
        transport
            .socket()
            .sent(1)
            .starts_with("GET /next HTTP/1.1\r\n")
    );
    assert!(transport.socket().closes.get() >= 1);
}

#[test]
fn dropping_the_transport_closes_the_socket() {
    let socket = MockSocket::new(vec![response(b"ok")]);
    let closes = Rc::clone(&socket.closes);
    let resolver = MockResolver::new();
    let mut transport = HttpTransport::new(socket, resolver);
    let clock = FixedClock;
    {
        let mut client = Client::new(&mut transport, &clock);
        client.get(&request("http://a.test/")).expect("get");
    }
    drop(transport);
    assert!(closes.get() >= 1);
}
