// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fixed-response parser tests.

use super::{Reader, Response, parse};
use crate::{Clock, Error, Transport, TransportError};
use alloc::string::String;
use alloc::vec::Vec;

struct Mock {
    data: Vec<u8>,
    position: usize,
    timeout: bool,
    chunk: usize,
}

impl Mock {
    fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            position: 0,
            timeout: false,
            chunk: 7,
        }
    }
}

impl Transport for Mock {
    fn connect(&mut self, _host: &str, _port: u16, _timeout_ms: u32) -> Result<(), TransportError> {
        Ok(())
    }

    fn write(&mut self, bytes: &[u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        Ok(bytes.len())
    }

    fn read(&mut self, bytes: &mut [u8], _timeout_ms: u32) -> Result<usize, TransportError> {
        if self.timeout {
            return Err(TransportError::Timeout);
        }
        let left = self.data.len().saturating_sub(self.position);
        let count = left.min(bytes.len()).min(self.chunk);
        bytes[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

struct ZeroClock;

impl Clock for ZeroClock {
    fn now_ms(&self) -> u64 {
        0
    }
}

fn run(data: &[u8], max_body: usize) -> Result<Response, Error> {
    let mut mock = Mock::new(data);
    let clock = ZeroClock;
    let mut reader = Reader::new(&mut mock, &clock, 1_000_000);
    parse(&mut reader, max_body)
}

#[test]
fn parses_content_length_and_case_insensitive_headers() {
    let response = run(
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-Trace: abc\r\n\r\nhello",
        64,
    )
    .expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(response.reason, "OK");
    assert_eq!(response.body, b"hello");
    assert_eq!(response.header("content-length"), Some("5"));
    assert_eq!(response.header("X-TRACE"), Some("abc"));
    assert!(response.is_success());
    assert!(!response.is_redirect());
}

#[test]
fn parses_chunked_with_extension_and_trailer() {
    let response = run(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
          5\r\nhello\r\n6;foo=bar\r\n world\r\n0\r\nX-Trailer: y\r\n\r\n",
        64,
    )
    .expect("response");
    assert_eq!(response.body, b"hello world");
}

#[test]
fn parses_close_delimited_body() {
    let response = run(b"HTTP/1.1 200 OK\r\n\r\nuntil-close", 64).expect("response");
    assert_eq!(response.body, b"until-close");
}

#[test]
fn parses_http_1_0_and_identity_transfer_encoding() {
    let response = run(
        b"HTTP/1.0 200 OK\r\nTransfer-Encoding: identity\r\nContent-Length: 3\r\n\r\nabc",
        8,
    )
    .expect("response");
    assert_eq!(response.body, b"abc");
}

#[test]
fn no_body_statuses_ignore_the_remaining_stream() {
    for status in ["204 No Content", "304 Not Modified"] {
        let data = alloc::format!("HTTP/1.1 {status}\r\nContent-Length: 5\r\n\r\n");
        let response = run(data.as_bytes(), 8).expect("response");
        assert!(response.body.is_empty(), "{status}");
    }
}

#[test]
fn skips_informational_responses() {
    let response = run(
        b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok",
        8,
    )
    .expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

#[test]
fn rejects_transfer_encoding_with_content_length() {
    assert_eq!(
        run(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n0\r\n\r\n",
            8,
        ),
        Err(Error::InvalidResponse)
    );
}

#[test]
fn rejects_conflicting_content_lengths() {
    assert_eq!(
        run(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\nhello!",
            8,
        ),
        Err(Error::InvalidResponse)
    );
}

#[test]
fn accepts_identical_duplicate_content_lengths() {
    let response = run(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok",
        8,
    )
    .expect("response");
    assert_eq!(response.body, b"ok");
}

#[test]
fn rejects_a_body_above_the_limit_in_both_framings() {
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n123456789", 8),
        Err(Error::BodyLimit)
    );
    assert_eq!(
        run(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\n123456789\r\n0\r\n\r\n",
            8,
        ),
        Err(Error::BodyLimit)
    );
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\n\r\n123456789", 8),
        Err(Error::BodyLimit)
    );
}

#[test]
fn rejects_truncated_and_malformed_chunks() {
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhel", 8),
        Err(Error::Truncated)
    );
    assert_eq!(
        run(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
            8
        ),
        Err(Error::Chunked)
    );
    assert_eq!(
        run(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcX\r\n",
            8
        ),
        Err(Error::Chunked)
    );
}

#[test]
fn rejects_obs_fold_and_bad_headers() {
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\nX-A: 1\r\n folded\r\n\r\n", 8),
        Err(Error::InvalidResponse)
    );
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\nBad Header: 1\r\n\r\n", 8),
        Err(Error::InvalidResponse)
    );
    assert_eq!(
        run(b"HTTP/1.1 200 OK\r\nX: 1\r\n\r\n", 0),
        Ok(Response {
            status: 200,
            reason: alloc::string::String::from("OK"),
            headers: alloc::vec![crate::Header {
                name: String::from("X"),
                value: String::from("1"),
            }],
            body: Vec::new(),
        })
    );
}

#[test]
fn rejects_header_count_and_size_limits() {
    let mut many = String::from("HTTP/1.1 200 OK\r\n");
    for index in 0..(crate::MAX_HEADERS + 1) {
        many.push_str(&alloc::format!("X-{index}: v\r\n"));
    }
    many.push_str("\r\n");
    assert_eq!(run(many.as_bytes(), 8), Err(Error::HeaderLimit));

    let mut line = String::from("HTTP/1.1 200 OK\r\nX-Long: ");
    line.push_str(&"a".repeat(crate::MAX_HEADER_LINE));
    line.push_str("\r\n\r\n");
    assert_eq!(run(line.as_bytes(), 8), Err(Error::HeaderLimit));
}

#[test]
fn rejects_bad_status_lines() {
    for data in [
        b"FTP/1.1 200 OK\r\n\r\n".as_slice(),
        b"HTTP/2.0 200 OK\r\n\r\n".as_slice(),
        b"HTTP/1.1 20 OK\r\n\r\n".as_slice(),
        b"HTTP/1.1 2000 OK\r\n\r\n".as_slice(),
        b"HTTP/1.1 abc OK\r\n\r\n".as_slice(),
    ] {
        assert_eq!(run(data, 8), Err(Error::InvalidResponse), "{data:?}");
    }
}

#[test]
fn maps_a_transport_timeout() {
    let mut mock = Mock::new(b"");
    mock.timeout = true;
    let clock = ZeroClock;
    let mut reader = Reader::new(&mut mock, &clock, 1_000_000);
    assert_eq!(parse(&mut reader, 8), Err(Error::Timeout));
}
