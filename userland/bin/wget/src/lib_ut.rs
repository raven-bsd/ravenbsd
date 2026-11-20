// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `wget` command core.

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
    response: Vec<u8>,
    position: usize,
}

impl MockTransport {
    fn new(response: &[u8]) -> Self {
        Self {
            response: Vec::from(response),
            position: 0,
        }
    }
}

impl Transport for MockTransport {
    fn connect(&mut self, _host: &str, _port: u16, _timeout_ms: u32) -> Result<(), TransportError> {
        self.position = 0;
        Ok(())
    }

    fn write(&mut self, bytes: &[u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        Ok(bytes.len())
    }

    fn read(&mut self, bytes: &mut [u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        let remaining = self.response.len().saturating_sub(self.position);
        let take = remaining.min(bytes.len());
        if let (Some(source), Some(target)) = (
            self.response
                .get(self.position..self.position.saturating_add(take)),
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
        quiet: false,
        max_time_secs: None,
        ca_file: String::from(curl::DEFAULT_CA_FILE),
    }
}

#[test]
fn parses_the_supported_options() {
    let parsed = parse(
        [
            "-q",
            "-O",
            "/etc/page",
            "--max-time",
            "9",
            "--cacert",
            "/tls-ca.pem",
            "http://example.test/page",
        ]
        .into_iter(),
    )
    .expect("parse");
    assert_eq!(parsed.url, "http://example.test/page");
    assert_eq!(parsed.output.as_deref(), Some("/etc/page"));
    assert!(parsed.quiet);
    assert_eq!(parsed.max_time_secs, Some(9));
    assert_eq!(parsed.ca_file, "/tls-ca.pem");
    assert_eq!(parsed.timeout_ms(), 9_000);
}

#[test]
fn parse_rejects_bad_options() {
    assert_eq!(
        parse(["-Q", "http://x/"].into_iter()),
        Err(ParseError::UnknownFlag)
    );
    assert_eq!(parse(["-O"].into_iter()), Err(ParseError::MissingValue));
    assert_eq!(
        parse(["--max-time", "0", "http://x/"].into_iter()),
        Err(ParseError::InvalidMaxTime)
    );
    assert_eq!(parse([].into_iter()), Err(ParseError::MissingUrl));
    assert_eq!(
        parse(["http://x/", "http://y/"].into_iter()),
        Err(ParseError::MultipleUrls)
    );
}

#[test]
fn derives_safe_default_names() {
    assert_eq!(
        default_filename("http://example.test/page.txt").expect("name"),
        "page.txt"
    );
    assert_eq!(
        default_filename("http://example.test/dir/").expect("name"),
        "index.html"
    );
    assert_eq!(
        default_filename("http://example.test/").expect("name"),
        "index.html"
    );
    assert_eq!(
        default_filename("https://example.test/a?q=1").expect("name"),
        "a"
    );
    assert_eq!(
        default_filename("http://example.test/.."),
        Err(ParseError::InvalidUrl)
    );
    assert_eq!(
        default_filename("http://example.test/."),
        Err(ParseError::InvalidUrl)
    );
}

#[test]
fn rewrites_https_targets() {
    assert_eq!(
        request_url("https://10.0.2.2:24443/known.txt").expect("url"),
        "http://10.0.2.2:24443/known.txt"
    );
    assert_eq!(
        request_url("http://example.test/").expect("url"),
        "http://example.test/"
    );
}

#[test]
fn fetches_a_body_and_reports_an_error_status() {
    let mut transport = MockTransport::new(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
    let parsed = options("http://example.test/");
    let response = fetch_with(&parsed, &mut transport, &MockClock).expect("fetch");
    assert_eq!(response.body, b"hello");

    let mut transport = MockTransport::new(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
    assert_eq!(
        fetch_with(&parsed, &mut transport, &MockClock),
        Err(FetchError::HttpStatus(404))
    );
}

#[test]
fn formats_the_summary() {
    assert_eq!(
        summary("http://example.test/page", "page", 5),
        "wget: http://example.test/page -> page (5 bytes)"
    );
}

#[test]
fn version_flag_is_recognised() {
    assert!(version_requested(["-V"].into_iter()));
    assert!(version_requested(["--version"].into_iter()));
    assert!(!version_requested(
        ["-q", "http://example.test/"].into_iter()
    ));
}

#[test]
fn version_line_names_the_client() {
    assert!(VERSION_LINE.starts_with("wget "));
    assert!(VERSION_LINE.contains("(RavenBSD)"));
}
