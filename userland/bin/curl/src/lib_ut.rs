// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `curl` command core.

use alloc::vec;
use alloc::vec::Vec;

use raven_http::{Clock, Transport, TransportError};

use super::*;

struct MockClock;

impl Clock for MockClock {
    fn now_ms(&self) -> u64 {
        0
    }
}

struct MockTransport {
    responses: Vec<Vec<u8>>,
    index: usize,
    position: usize,
    connects: usize,
}

impl MockTransport {
    fn new(responses: Vec<&[u8]>) -> Self {
        Self {
            responses: responses.into_iter().map(Vec::from).collect(),
            index: 0,
            position: 0,
            connects: 0,
        }
    }
}

impl Transport for MockTransport {
    fn connect(&mut self, _host: &str, _port: u16, _timeout_ms: u32) -> Result<(), TransportError> {
        if self.connects > self.index {
            self.index = self.index.saturating_add(1);
        }
        self.position = 0;
        self.connects = self.connects.saturating_add(1);
        Ok(())
    }

    fn write(&mut self, bytes: &[u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        Ok(bytes.len())
    }

    fn read(&mut self, bytes: &mut [u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        let Some(response) = self.responses.get(self.index) else {
            return Ok(0);
        };
        let remaining = response.len().saturating_sub(self.position);
        let take = remaining.min(bytes.len());
        if let (Some(source), Some(target)) = (
            response.get(self.position..self.position.saturating_add(take)),
            bytes.get_mut(..take),
        ) {
            target.copy_from_slice(source);
        }
        self.position = self.position.saturating_add(take);
        Ok(take)
    }
}

fn options(url: &str) -> Options {
    Options {
        url: String::from(url),
        output: None,
        silent: false,
        fail: false,
        headers_only: false,
        location: false,
        max_time_secs: None,
        headers: Vec::new(),
        ca_file: String::from(DEFAULT_CA_FILE),
    }
}

#[test]
fn parses_the_supported_options() {
    let parsed = parse(
        [
            "-s",
            "-f",
            "-I",
            "-L",
            "-o",
            "/tmp/out",
            "--max-time",
            "7",
            "-H",
            "X-Test: yes",
            "--cacert",
            "/tls-ca.pem",
            "http://example.test/page",
        ]
        .into_iter(),
    )
    .expect("parse");
    assert_eq!(parsed.url, "http://example.test/page");
    assert_eq!(parsed.output.as_deref(), Some("/tmp/out"));
    assert!(parsed.silent && parsed.fail && parsed.headers_only && parsed.location);
    assert_eq!(parsed.max_time_secs, Some(7));
    assert_eq!(parsed.headers.len(), 1);
    assert_eq!(parsed.headers[0].name, "X-Test");
    assert_eq!(parsed.headers[0].value, "yes");
    assert_eq!(parsed.ca_file, "/tls-ca.pem");
    assert_eq!(parsed.timeout_ms(), 7_000);
    assert!(!parsed.writes_stdout());
}

#[test]
fn parse_rejects_bad_options() {
    assert_eq!(
        parse(["-Q", "http://x/"].into_iter()),
        Err(ParseError::UnknownFlag)
    );
    assert_eq!(parse(["-o"].into_iter()), Err(ParseError::MissingValue));
    assert_eq!(
        parse(["--max-time", "0", "http://x/"].into_iter()),
        Err(ParseError::InvalidMaxTime)
    );
    assert_eq!(
        parse(["-H", "Bad"].into_iter()),
        Err(ParseError::InvalidHeader)
    );
    assert_eq!(parse([].into_iter()), Err(ParseError::MissingUrl));
    assert_eq!(
        parse(["http://x/", "http://y/"].into_iter()),
        Err(ParseError::MultipleUrls)
    );
}

#[test]
fn fetch_returns_a_200_body() {
    let mut transport = MockTransport::new(vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-Test: yes\r\n\r\nhello",
    ]);
    let parsed = options("http://example.test/");
    let response = fetch_with(&parsed.url, &parsed, &mut transport, &MockClock).expect("fetch");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    assert_eq!(response.header("x-test"), Some("yes"));
}

#[test]
fn fetch_reports_404_only_with_f() {
    let response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
    let mut transport = MockTransport::new(vec![response]);
    let mut parsed = options("http://example.test/missing");
    assert!(fetch_with(&parsed.url, &parsed, &mut transport, &MockClock).is_ok());
    parsed.fail = true;
    let mut transport = MockTransport::new(vec![response]);
    assert_eq!(
        fetch_with(&parsed.url, &parsed, &mut transport, &MockClock),
        Err(FetchError::HttpStatus(404))
    );
}

#[test]
fn fetch_follows_a_bounded_redirect_with_l() {
    let mut transport = MockTransport::new(vec![
        b"HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nnext",
    ]);
    let mut parsed = options("http://example.test/start");
    parsed.location = true;
    let response = fetch_with(&parsed.url, &parsed, &mut transport, &MockClock).expect("fetch");
    assert_eq!(response.body, b"next");
    assert_eq!(transport.connects, 2);
}

#[test]
fn fetch_without_l_rejects_the_redirect() {
    let mut transport = MockTransport::new(vec![
        b"HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 0\r\n\r\n",
    ]);
    let parsed = options("http://example.test/start");
    assert_eq!(
        fetch_with(&parsed.url, &parsed, &mut transport, &MockClock),
        Err(FetchError::Http(raven_http::Error::TooManyRedirects))
    );
}

#[test]
fn formats_the_status_and_headers() {
    let mut transport = MockTransport::new(vec![
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nX-Test: yes\r\n\r\n",
    ]);
    let parsed = options("http://example.test/");
    let response = fetch_with(&parsed.url, &parsed, &mut transport, &MockClock).expect("fetch");
    let head = format_head(&response);
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(head.contains("Content-Length: 0\r\n"));
    assert!(head.contains("X-Test: yes\r\n"));
    assert!(head.ends_with("\r\n\r\n"));
}

#[test]
fn rewrites_https_to_the_http_server_form() {
    assert_eq!(
        https_http_url("https://example.test/path?q=1").expect("url"),
        "http://example.test:443/path?q=1"
    );
    assert_eq!(
        https_http_url("https://10.0.2.2:24443/known.txt").expect("url"),
        "http://10.0.2.2:24443/known.txt"
    );
    assert_eq!(
        https_http_url("http://example.test/").expect("url"),
        "http://example.test/"
    );
    assert!(is_https("https://example.test/"));
    assert!(!is_https("http://example.test/"));
}

#[test]
fn error_messages_are_stable() {
    assert_eq!(
        FetchError::Http(raven_http::Error::Timeout).message(),
        "request timed out"
    );
    assert_eq!(
        FetchError::HttpStatus(500).message(),
        "HTTP status indicates failure"
    );
    assert_eq!(ParseError::MissingUrl.message(), "missing URL");
}

#[test]
fn version_flag_is_recognised() {
    assert!(version_requested(["-V"].into_iter()));
    assert!(version_requested(["--version"].into_iter()));
    assert!(!version_requested(
        ["-s", "http://example.test/"].into_iter()
    ));
}

#[test]
fn version_line_names_the_client() {
    assert!(VERSION_LINE.starts_with("curl "));
    assert!(VERSION_LINE.contains("(RavenBSD)"));
}
