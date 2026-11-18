// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! HTTP/1.1 response parsing with bounded headers and bodies.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::header::{Header, valid_name};
use crate::{
    Clock, Error, MAX_CHUNK_LINE, MAX_HEADER_BYTES, MAX_HEADER_LINE, MAX_HEADERS, MAX_STATUS_LINE,
    Transport, TransportError,
};

/// One parsed HTTP response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// Three-digit status code.
    pub status: u16,
    /// Reason phrase (may be empty).
    pub reason: String,
    /// Header fields in wire order.
    pub headers: Vec<Header>,
    /// Decoded body.
    pub body: Vec<u8>,
}

impl Response {
    /// The first value of `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.is(name))
            .map(|header| header.value.as_str())
    }

    /// Every value of `name`, compared case-insensitively.
    pub fn header_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.headers
            .iter()
            .filter(move |header| header.is(name))
            .map(|header| header.value.as_str())
    }

    /// True for a 2xx status.
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// True for a redirect the client follows.
    pub const fn is_redirect(&self) -> bool {
        matches!(self.status, 301 | 302 | 303 | 307 | 308)
    }

    /// The `Location` value, if present.
    pub fn location(&self) -> Option<&str> {
        self.header("location")
    }
}

/// A bounded buffered reader over one transport connection.
pub(crate) struct Reader<'a, T: Transport, C: Clock> {
    transport: &'a mut T,
    clock: &'a C,
    deadline_ms: u64,
    buffer: [u8; 4096],
    position: usize,
    length: usize,
    eof: bool,
}

impl<'a, T: Transport, C: Clock> Reader<'a, T, C> {
    /// Create a reader whose total deadline is `deadline_ms` on `clock`.
    pub(crate) fn new(transport: &'a mut T, clock: &'a C, deadline_ms: u64) -> Self {
        Self {
            transport,
            clock,
            deadline_ms,
            buffer: [0; 4096],
            position: 0,
            length: 0,
            eof: false,
        }
    }

    fn remaining(&self) -> Result<u32, Error> {
        let now = self.clock.now_ms();
        if now >= self.deadline_ms {
            return Err(Error::Timeout);
        }
        Ok(u32::try_from(self.deadline_ms - now).unwrap_or(u32::MAX))
    }

    fn fill(&mut self) -> Result<(), Error> {
        if self.position < self.length {
            return Ok(());
        }
        if self.eof {
            return Ok(());
        }
        let timeout_ms = self.remaining()?;
        match self.transport.read(&mut self.buffer, timeout_ms) {
            Ok(0) => {
                self.eof = true;
                Ok(())
            }
            Ok(read) if read <= self.buffer.len() => {
                self.position = 0;
                self.length = read;
                Ok(())
            }
            Ok(_) => Err(Error::InvalidResponse),
            Err(TransportError::Timeout) => Err(Error::Timeout),
            Err(TransportError::Io) => Err(Error::Transport),
        }
    }

    fn read_byte(&mut self) -> Result<Option<u8>, Error> {
        self.fill()?;
        if self.position == self.length {
            return Ok(None);
        }
        let byte = self.buffer[self.position];
        self.position += 1;
        Ok(Some(byte))
    }

    /// Read one CRLF- or LF-terminated line and return it without the terminator.
    pub(crate) fn read_line(&mut self, limit: usize) -> Result<Vec<u8>, Error> {
        let mut line = Vec::new();
        loop {
            let Some(byte) = self.read_byte()? else {
                return Err(Error::Truncated);
            };
            if byte == b'\n' {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(line);
            }
            if line.len() >= limit {
                return Err(Error::HeaderLimit);
            }
            line.push(byte);
        }
    }

    pub(crate) fn read_exact(&mut self, out: &mut [u8]) -> Result<(), Error> {
        for slot in out {
            *slot = self.read_byte()?.ok_or(Error::Truncated)?;
        }
        Ok(())
    }

    fn read_to_end(&mut self, limit: usize) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        loop {
            self.fill()?;
            if self.position == self.length {
                return Ok(out);
            }
            let take = self.length - self.position;
            if out.len().saturating_add(take) > limit {
                return Err(Error::BodyLimit);
            }
            out.extend_from_slice(&self.buffer[self.position..self.length]);
            self.position = self.length;
        }
    }

    fn read_crlf(&mut self) -> Result<(), Error> {
        let first = self.read_byte()?.ok_or(Error::Truncated)?;
        let second = self.read_byte()?.ok_or(Error::Truncated)?;
        if first == b'\r' && second == b'\n' {
            Ok(())
        } else {
            Err(Error::Chunked)
        }
    }
}

/// Parse one response, following interim `1xx` responses until a final status.
pub(crate) fn parse<T: Transport, C: Clock>(
    reader: &mut Reader<'_, T, C>,
    max_body: usize,
) -> Result<Response, Error> {
    loop {
        let line = reader.read_line(MAX_STATUS_LINE)?;
        let text = core::str::from_utf8(&line).map_err(|_| Error::InvalidResponse)?;
        let (version, status, reason) = parse_status_line(text)?;
        if version != "HTTP/1.1" && version != "HTTP/1.0" {
            return Err(Error::InvalidResponse);
        }
        let headers = read_headers(reader)?;
        if (100..200).contains(&status) {
            if status == 101 {
                return Err(Error::InvalidResponse);
            }
            continue;
        }
        let body = if status == 204 || status == 304 {
            Vec::new()
        } else {
            read_body(reader, &headers, max_body)?
        };
        return Ok(Response {
            status,
            reason,
            headers,
            body,
        });
    }
}

fn parse_status_line(text: &str) -> Result<(&str, u16, String), Error> {
    let mut parts = text.splitn(3, ' ');
    let version = parts.next().ok_or(Error::InvalidResponse)?;
    let status = parts.next().ok_or(Error::InvalidResponse)?;
    let reason = parts.next().unwrap_or("").trim().to_string();
    if status.len() != 3 || !status.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidResponse);
    }
    let code: u16 = status.parse().map_err(|_| Error::InvalidResponse)?;
    if !(100..=599).contains(&code) {
        return Err(Error::InvalidResponse);
    }
    Ok((version, code, reason))
}

fn read_headers<T: Transport, C: Clock>(
    reader: &mut Reader<'_, T, C>,
) -> Result<Vec<Header>, Error> {
    let mut headers = Vec::new();
    let mut bytes = 0usize;
    loop {
        let line = reader.read_line(MAX_HEADER_LINE)?;
        bytes = bytes.saturating_add(line.len() + 2);
        if bytes > MAX_HEADER_BYTES {
            return Err(Error::HeaderLimit);
        }
        if line.is_empty() {
            return Ok(headers);
        }
        if line
            .first()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            return Err(Error::InvalidResponse);
        }
        if headers.len() >= MAX_HEADERS {
            return Err(Error::HeaderLimit);
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            return Err(Error::InvalidResponse);
        };
        let name = core::str::from_utf8(&line[..colon]).map_err(|_| Error::InvalidResponse)?;
        if !valid_name(name) {
            return Err(Error::InvalidResponse);
        }
        let value = core::str::from_utf8(&line[colon + 1..])
            .map_err(|_| Error::InvalidResponse)?
            .trim_matches(|character| character == ' ' || character == '\t');
        let header = Header::new(name, value).ok_or(Error::InvalidResponse)?;
        headers.push(header);
    }
}

fn read_body<T: Transport, C: Clock>(
    reader: &mut Reader<'_, T, C>,
    headers: &[Header],
    max_body: usize,
) -> Result<Vec<u8>, Error> {
    let transfer = header_values(headers, "transfer-encoding");
    let content_lengths = header_values(headers, "content-length");
    let mut chunked = false;
    if !transfer.is_empty() {
        let mut tokens: Vec<&str> = Vec::new();
        for value in transfer {
            for token in value.split(',') {
                tokens.push(token.trim());
            }
        }
        let Some(last) = tokens.last() else {
            return Err(Error::InvalidResponse);
        };
        let has_chunked = tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("chunked"));
        if has_chunked {
            if !last.eq_ignore_ascii_case("chunked") {
                return Err(Error::InvalidResponse);
            }
            chunked = true;
        } else if !tokens
            .iter()
            .all(|token| token.eq_ignore_ascii_case("identity"))
        {
            return Err(Error::InvalidResponse);
        }
    }

    let mut length: Option<usize> = None;
    for value in content_lengths {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Error::InvalidResponse);
            }
            let parsed: usize = part.parse().map_err(|_| Error::InvalidResponse)?;
            if let Some(existing) = length {
                if existing != parsed {
                    return Err(Error::InvalidResponse);
                }
            } else {
                length = Some(parsed);
            }
        }
    }
    if chunked && length.is_some() {
        return Err(Error::InvalidResponse);
    }
    if let Some(length) = length {
        if length > max_body {
            return Err(Error::BodyLimit);
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        return Ok(body);
    }
    if chunked {
        return read_chunked(reader, max_body);
    }
    reader.read_to_end(max_body)
}

fn read_chunked<T: Transport, C: Clock>(
    reader: &mut Reader<'_, T, C>,
    max_body: usize,
) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    loop {
        let line = reader.read_line(MAX_CHUNK_LINE)?;
        let text = core::str::from_utf8(&line).map_err(|_| Error::Chunked)?;
        let size_text = text.split(';').next().unwrap_or("").trim();
        if size_text.is_empty() || !size_text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Chunked);
        }
        let size = usize::from_str_radix(size_text, 16).map_err(|_| Error::Overflow)?;
        if size == 0 {
            let _trailers = read_headers(reader)?;
            return Ok(body);
        }
        if body
            .len()
            .checked_add(size)
            .is_none_or(|total| total > max_body)
        {
            return Err(Error::BodyLimit);
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        reader.read_crlf()?;
    }
}

fn header_values<'a>(headers: &'a [Header], name: &str) -> Vec<&'a str> {
    headers
        .iter()
        .filter(|header| header.is(name))
        .map(|header| header.value.as_str())
        .collect()
}

#[cfg(test)]
#[path = "response_ut.rs"]
mod tests;
