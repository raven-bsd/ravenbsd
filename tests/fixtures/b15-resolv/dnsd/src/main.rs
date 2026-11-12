// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A deterministic loopback DNS server for the resolv qtest.
//!
//! It binds UDP 127.0.0.1:53 through netd and answers `a.test` (A),
//! `aaaa.test` (AAAA), `alias.test` (CNAME to a.test) and everything else with
//! NXDOMAIN.  No answer is invented: this is the server the resolver queries.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const ADDR: [u8; 4] = [127, 0, 0, 1];
    const PORT: u16 = 53;
    const MAX_PACKET: usize = 512;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("b15-dnsd: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let handle = client
            .open("net:udp/127.0.0.1:53", READ_WRITE)
            .map_err(|error| format!("open: {}", error.description()))?;
        let request = SocketRequest::Call(Call::Bind(Endpoint {
            ip: ADDR,
            port: PORT,
        }));
        let mut encoded = [0u8; 1024];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "encode bind".to_string())?;
        let mut response = [0u8; 64];
        let length = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| format!("bind call: {}", error.description()))?;
        if Response::decode(&response[..length]).map_err(|_| "decode bind".to_string())?
            != Response::Ok
        {
            return Err("bind rejected".to_string());
        }
        raven_rt::println!("b15-dnsd: ready");
        let mut packet = [0u8; 1024];
        loop {
            let request = SocketRequest::Read {
                len: MAX_PACKET as u32,
            };
            let encoded_length = request
                .encode(&mut encoded)
                .map_err(|_| "encode read".to_string())?;
            match client.call(handle, &encoded[..encoded_length], &mut packet) {
                Ok(length) => {
                    let bytes = &packet[..length];
                    if bytes.get(4..6) == Some(&[6, 0]) {
                        match Response::decode(bytes) {
                            Ok(Response::Err(Errno::EAGAIN)) => {
                                let _ = raven_rt::nanosleep(1_000_000);
                                continue;
                            }
                            Ok(Response::Err(error)) => {
                                return Err(format!("read call: {}", error.description()));
                            }
                            _ => return Err("malformed read error".to_string()),
                        }
                    }
                    let flags = *bytes.get(6).ok_or("read flags")?;
                    let header_len = if flags & 1 != 0 { 17 } else { 11 };
                    let Some(header) = bytes.get(..header_len) else {
                        continue;
                    };
                    let response = Response::decode(header)
                        .map_err(|error| format!("decode read header: {error:?}"))?;
                    let Response::Read {
                        len,
                        from: Some(from),
                    } = response
                    else {
                        continue;
                    };
                    let start = header_len;
                    let end = start + len as usize;
                    let Some(query) = bytes.get(start..end) else {
                        continue;
                    };
                    if let Some(reply) = answer(query) {
                        let mut scratch = [0u8; 64];
                        let request = SocketRequest::Write {
                            data: &reply,
                            to: Some(from),
                        };
                        let encoded_length = request
                            .encode(&mut encoded)
                            .map_err(|_| "encode write".to_string())?;
                        let _ = client.call(handle, &encoded[..encoded_length], &mut scratch);
                    }
                }
                Err(Errno::EAGAIN) => {
                    let _ = raven_rt::nanosleep(1_000_000);
                }
                Err(error) => return Err(format!("read call: {}", error.description())),
            }
        }
    }

    fn answer(query: &[u8]) -> Option<Vec<u8>> {
        if query.len() < 12 || query[2] & 0x80 != 0 {
            return None;
        }
        let (name, qtype, question_end) = parse_question(query)?;
        let mut reply = query[..question_end].to_vec();
        let rcode = match name.as_str() {
            "a.test" if qtype == 1 => 0,
            "aaaa.test" if qtype == 28 => 0,
            "alias.test" if qtype == 1 => 0,
            _ => 3,
        };
        if rcode == 3 {
            reply[2] = 0x81;
            reply[3] = 0x83;
            reply[6..8].copy_from_slice(&0u16.to_be_bytes());
            return Some(reply);
        }
        reply[2] = 0x81;
        reply[3] = 0x80;
        reply[6..8].copy_from_slice(&1u16.to_be_bytes());
        let answer = match name.as_str() {
            "a.test" => record(1, &[192, 0, 2, 10]),
            "aaaa.test" => record(
                28,
                &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            ),
            "alias.test" => {
                let mut data = Vec::new();
                for label in "a.test".split('.') {
                    data.push(label.len() as u8);
                    data.extend_from_slice(label.as_bytes());
                }
                data.push(0);
                record(5, &data)
            }
            _ => return None,
        };
        reply.extend_from_slice(&answer);
        Some(reply)
    }

    fn record(kind: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&60u32.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    fn parse_question(query: &[u8]) -> Option<(String, u16, usize)> {
        let mut offset = 12usize;
        let mut labels = Vec::new();
        loop {
            let length = *query.get(offset)?;
            if length == 0 {
                offset += 1;
                break;
            }
            if length & 0xc0 != 0 {
                return None;
            }
            let start = offset + 1;
            let end = start.checked_add(usize::from(length))?;
            let label = query.get(start..end)?;
            labels.push(core::str::from_utf8(label).ok()?);
            offset = end;
        }
        let qtype = u16::from_be_bytes([*query.get(offset)?, *query.get(offset + 1)?]);
        let class = u16::from_be_bytes([*query.get(offset + 2)?, *query.get(offset + 3)?]);
        if class != 1 {
            return None;
        }
        Some((labels.join("."), qtype, offset + 4))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
