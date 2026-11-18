// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Redirect, timeout and credential-policy tests with fixed responses.

use super::Client;
use crate::request::RequestBuilder;
use crate::{Clock, Error, Transport, TransportError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

struct Mock {
    responses: Vec<Vec<u8>>,
    index: usize,
    current: Vec<u8>,
    position: usize,
    reads: Vec<(String, u16)>,
    writes: Vec<Vec<u8>>,
    read_error: Option<TransportError>,
    connect_error: bool,
}

impl Mock {
    fn new(responses: Vec<Vec<u8>>) -> Self {
        Self {
            responses,
            index: 0,
            current: Vec::new(),
            position: 0,
            reads: Vec::new(),
            writes: Vec::new(),
            read_error: None,
            connect_error: false,
        }
    }

    fn sent(&self, index: usize) -> String {
        String::from_utf8(self.writes[index].clone()).expect("utf-8 request")
    }
}

impl Transport for Mock {
    fn connect(&mut self, host: &str, port: u16, _timeout_ms: u32) -> Result<(), TransportError> {
        if self.connect_error {
            return Err(TransportError::Io);
        }
        self.reads.push((host.to_string(), port));
        self.current = self
            .responses
            .get(self.index)
            .cloned()
            .ok_or(TransportError::Io)?;
        self.index += 1;
        self.position = 0;
        self.writes.push(Vec::new());
        Ok(())
    }

    fn write(&mut self, bytes: &[u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        let Some(write) = self.writes.last_mut() else {
            return Err(TransportError::Io);
        };
        write.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn read(&mut self, bytes: &mut [u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        if let Some(error) = self.read_error {
            return Err(error);
        }
        let left = self.current.len().saturating_sub(self.position);
        let count = left.min(bytes.len());
        bytes[..count].copy_from_slice(&self.current[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

struct FixedClock(u64);

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut out = alloc::format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn redirect(status: &str, location: &str) -> Vec<u8> {
    response(status, &alloc::format!("Location: {location}\r\n"), b"")
}

fn request(url: &str) -> crate::Request {
    RequestBuilder::new(url).expect("builder").build()
}

#[test]
fn performs_one_get_and_returns_the_body() {
    let mut mock = Mock::new(vec![response("200 OK", "X-Test: 1\r\n", b"ok")]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    let response = client.get(&request("http://example.com/")).expect("get");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
    assert_eq!(mock.reads, vec![(String::from("example.com"), 80)]);
    assert!(mock.sent(0).starts_with("GET / HTTP/1.1\r\n"));
    assert!(mock.sent(0).contains("Host: example.com\r\n"));
    assert!(mock.sent(0).contains("Connection: close\r\n"));
}

#[test]
fn follows_a_relative_redirect() {
    let mut mock = Mock::new(vec![
        redirect("302 Found", "/next"),
        response("200 OK", "", b"done"),
    ]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    let response = client.get(&request("http://example.com/a")).expect("get");
    assert_eq!(response.body, b"done");
    assert_eq!(mock.reads.len(), 2);
    assert!(mock.sent(1).starts_with("GET /next HTTP/1.1\r\n"));
}

#[test]
fn follows_303_as_get() {
    let mut mock = Mock::new(vec![
        redirect("303 See Other", "http://example.com/done"),
        response("200 OK", "", b"done"),
    ]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    let response = client
        .get(&request("http://example.com/start"))
        .expect("get");
    assert_eq!(response.body, b"done");
    assert!(mock.sent(1).starts_with("GET /done HTTP/1.1\r\n"));
}

#[test]
fn drops_credentials_only_on_a_cross_origin_redirect() {
    let mut mock = Mock::new(vec![
        redirect("302 Found", "/same"),
        redirect("302 Found", "http://other.test/x"),
        response("200 OK", "", b"done"),
    ]);
    let clock = FixedClock(0);
    let request = RequestBuilder::new("http://example.com/start")
        .expect("builder")
        .header("Authorization", "Bearer secret")
        .expect("auth")
        .header("Cookie", "session=1")
        .expect("cookie")
        .header("X-Keep", "yes")
        .expect("keep")
        .max_redirects(2)
        .build();
    let mut client = Client::new(&mut mock, &clock);
    client.get(&request).expect("get");
    assert!(mock.sent(0).contains("Authorization: Bearer secret"));
    assert!(mock.sent(1).contains("Authorization: Bearer secret"));
    assert!(!mock.sent(2).contains("Authorization"));
    assert!(!mock.sent(2).contains("Cookie"));
    assert!(mock.sent(2).contains("X-Keep: yes"));
    assert_eq!(mock.reads[2], (String::from("other.test"), 80));
}

#[test]
fn rejects_an_https_redirect() {
    let mut mock = Mock::new(vec![redirect("302 Found", "https://example.com/x")]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    assert_eq!(
        client.get(&request("http://example.com/")),
        Err(Error::UnsupportedScheme)
    );
    assert_eq!(mock.reads.len(), 1);
}

#[test]
fn rejects_a_redirect_without_location() {
    let mut mock = Mock::new(vec![response("302 Found", "", b"")]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    assert_eq!(
        client.get(&request("http://example.com/")),
        Err(Error::MissingLocation)
    );
}

#[test]
fn enforces_the_redirect_budget() {
    let mut mock = Mock::new(vec![
        redirect("301 Moved Permanently", "/one"),
        redirect("308 Permanent Redirect", "/two"),
        response("200 OK", "", b"never"),
    ]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    let request = RequestBuilder::new("http://example.com/")
        .expect("builder")
        .max_redirects(1)
        .build();
    assert_eq!(client.get(&request), Err(Error::TooManyRedirects));
    assert_eq!(mock.reads.len(), 2);
}

#[test]
fn zero_redirect_budget_rejects_the_first_redirect() {
    let mut mock = Mock::new(vec![redirect("302 Found", "/next")]);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    let request = RequestBuilder::new("http://example.com/")
        .expect("builder")
        .max_redirects(0)
        .build();
    assert_eq!(client.get(&request), Err(Error::TooManyRedirects));
    assert_eq!(mock.reads.len(), 1);
}

#[test]
fn maps_connect_and_read_failures() {
    let mut mock = Mock::new(vec![response("200 OK", "", b"ok")]);
    mock.connect_error = true;
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    assert_eq!(
        client.get(&request("http://example.com/")),
        Err(Error::Transport)
    );

    let mut mock = Mock::new(vec![response("200 OK", "", b"ok")]);
    mock.read_error = Some(TransportError::Timeout);
    let clock = FixedClock(0);
    let mut client = Client::new(&mut mock, &clock);
    assert_eq!(
        client.get(&request("http://example.com/")),
        Err(Error::Timeout)
    );
}

#[test]
fn a_zero_timeout_fails_before_connecting() {
    let mut mock = Mock::new(vec![response("200 OK", "", b"ok")]);
    let clock = FixedClock(10);
    let request = RequestBuilder::new("http://example.com/")
        .expect("builder")
        .timeout_ms(0)
        .build();
    let mut client = Client::new(&mut mock, &clock);
    assert_eq!(client.get(&request), Err(Error::Timeout));
    assert!(mock.reads.is_empty());
}
