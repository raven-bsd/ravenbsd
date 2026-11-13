// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ping` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_abi::types::clock;
    use raven_ping::{MAX_PAYLOAD, Options};
    use raven_proto_net::socket::{MAX_ICMP_TIMEOUT_MS, Response, SocketRequest};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const RETRY_MILLIS: u64 = 1;
    const ICMP_HEADER: usize = 22;

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<&str> = args.iter().skip(1).collect();
        let options = match raven_ping::parse(&values) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("ping: {}", error.message());
                return 1;
            }
        };
        match run(&options) {
            Ok(true) => 0,
            Ok(false) => 1,
            Err(error) => {
                raven_rt::eprintln!("ping: {error}");
                1
            }
        }
    }

    fn run(options: &Options<'_>) -> Result<bool, String> {
        let target =
            raven_ping::parse_ipv4(options.host).map_err(|error| error.message().to_string())?;
        let id = echo_id()?;
        let client = Client::new();
        let path = alloc::format!("net:icmp/{}", options.host);
        let handle = client
            .open(&path, READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        let result = probe(&client, handle, target, id, options);
        let _ = client.close(handle);
        result
    }

    fn probe(
        client: &Client,
        handle: Handle,
        target: [u8; 4],
        id: u16,
        options: &Options<'_>,
    ) -> Result<bool, String> {
        let started = now_ms()?;
        let deadline = started.saturating_add(options.timeout_secs.saturating_mul(1000));
        let mut payload = [0u8; MAX_PAYLOAD];
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = b'a' + (index % 26) as u8;
        }
        let payload = payload
            .get(..options.payload_len)
            .ok_or("payload length out of range")?;

        let mut sent = 0u16;
        let mut received = 0u16;
        let mut minimum = u32::MAX;
        let mut maximum = 0u32;
        let mut total = 0u64;
        for seq in 1..=options.count {
            let now = now_ms()?;
            if now >= deadline {
                break;
            }
            let remaining = (deadline - now).min(u64::from(MAX_ICMP_TIMEOUT_MS));
            let timeout_ms = u32::try_from(remaining.max(1)).map_err(|_| "timeout overflow")?;
            queue_echo(client, handle, id, seq, timeout_ms, payload)?;
            sent = sent.saturating_add(1);
            match read_reply(client, handle, target, seq, payload.len())? {
                Some(rtt_ms) => {
                    received = received.saturating_add(1);
                    minimum = minimum.min(rtt_ms);
                    maximum = maximum.max(rtt_ms);
                    total = total.saturating_add(u64::from(rtt_ms));
                    raven_rt::println!("ping: {} seq={} rtt={}ms", options.host, seq, rtt_ms);
                }
                None => {
                    raven_rt::println!("ping: {} seq={} timeout", options.host, seq);
                }
            }
        }

        let loss = if sent == 0 {
            100
        } else {
            u32::from(sent.saturating_sub(received))
                .saturating_mul(100)
                .checked_div(u32::from(sent))
                .unwrap_or(100)
        };
        raven_rt::println!(
            "ping: {} packets transmitted, {} received, {}% packet loss",
            sent,
            received,
            loss
        );
        if received > 0 {
            let average = total / u64::from(received);
            raven_rt::println!(
                "ping: rtt min/avg/max = {}/{}/{} ms",
                minimum,
                average,
                maximum
            );
        }
        Ok(sent == options.count && received == sent)
    }

    fn queue_echo(
        client: &Client,
        handle: Handle,
        id: u16,
        seq: u16,
        timeout_ms: u32,
        payload: &[u8],
    ) -> Result<(), String> {
        let request = SocketRequest::IcmpEcho {
            id,
            seq,
            timeout_ms,
            data: payload,
        };
        let mut encoded = [0u8; 1500];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("icmp request encode failed"))?;
        let mut response = [0u8; 64];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        match Response::decode(response.get(..read).ok_or("short echo response")?) {
            Ok(Response::Ok) => Ok(()),
            Ok(Response::Err(error)) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected echo response")),
        }
    }

    fn read_reply(
        client: &Client,
        handle: Handle,
        target: [u8; 4],
        seq: u16,
        payload_len: usize,
    ) -> Result<Option<u32>, String> {
        let mut buffer = [0u8; 1500];
        loop {
            let request = SocketRequest::Read {
                len: u32::try_from(payload_len.max(1)).map_err(|_| "read length")?,
            };
            let mut encoded = [0u8; 32];
            let length = request
                .encode(&mut encoded)
                .map_err(|_| String::from("read request encode failed"))?;
            let read = match client.call(handle, &encoded[..length], &mut buffer) {
                Ok(read) => read,
                Err(Errno::EAGAIN) => {
                    sleep_ms(RETRY_MILLIS);
                    continue;
                }
                Err(Errno::ETIMEDOUT) => return Ok(None),
                Err(error) => return Err(error.description().to_string()),
            };
            let bytes = buffer.get(..read).ok_or("short read response")?;
            if bytes.get(4..6) == Some(&[6, 0]) {
                return match Response::decode(bytes) {
                    Ok(Response::Err(Errno::EAGAIN)) => {
                        sleep_ms(RETRY_MILLIS);
                        continue;
                    }
                    Ok(Response::Err(Errno::ETIMEDOUT)) => Ok(None),
                    Ok(Response::Err(error)) => Err(error.description().to_string()),
                    _ => Err(String::from("unexpected read error response")),
                };
            }
            let header = Response::decode(bytes.get(..ICMP_HEADER).ok_or("short icmp header")?)
                .map_err(|_| String::from("icmp header decode failed"))?;
            let Response::Icmp {
                source,
                seq: reply_seq,
                rtt_ms,
                len,
                ..
            } = header
            else {
                return Err(String::from("unexpected icmp response"));
            };
            if source.ip != target {
                return Err(String::from("icmp reply source differs"));
            }
            if reply_seq != seq {
                return Err(String::from("icmp reply sequence differs"));
            }
            let end = ICMP_HEADER
                .checked_add(usize::from(len))
                .ok_or("icmp payload overflow")?;
            if end > bytes.len() {
                return Err(String::from("short icmp payload"));
            }
            return Ok(Some(rtt_ms));
        }
    }

    fn echo_id() -> Result<u16, String> {
        let stamp = raven_rt::clock_get(clock::MONOTONIC)
            .map_err(|error| error.description().to_string())?;
        let id = ((stamp.tv_sec as u64) ^ (stamp.tv_nsec as u64)) as u16;
        Ok(if id == 0 { 1 } else { id })
    }

    fn now_ms() -> Result<u64, String> {
        let stamp = raven_rt::clock_get(clock::MONOTONIC)
            .map_err(|error| error.description().to_string())?;
        let seconds = u64::try_from(stamp.tv_sec).map_err(|_| "clock overflow")?;
        let nanos = u64::try_from(stamp.tv_nsec).map_err(|_| "clock overflow")?;
        seconds
            .checked_mul(1000)
            .and_then(|value| value.checked_add(nanos / 1_000_000))
            .ok_or_else(|| String::from("clock overflow"))
    }

    fn sleep_ms(milliseconds: u64) {
        let _ = raven_rt::nanosleep(milliseconds.saturating_mul(1_000_000));
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
