// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `resolv:` client for the resolver qtest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_rt::entry;
    use raven_scheme::Client;
    use resolvd::proto::{Address, QTYPE_A, QTYPE_AAAA, Query, Reply, STATUS_NXDOMAIN, STATUS_OK};

    entry!(main);

    const READ: u32 = 1;
    const MAX_PACKET: usize = 512;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("b15-resolv: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("b15-resolv: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let handle = client
            .open("resolv:", READ)
            .map_err(|error| format!("open resolv:: {}", error.description()))?;
        let a = query(&client, handle, "a.test", QTYPE_A)?;
        match (a.status, a.records.first().map(|record| record.address)) {
            (STATUS_OK, Some(Address::V4(address))) => {
                raven_rt::println!(
                    "b15-resolv: a.test={}.{}.{}.{}",
                    address[0],
                    address[1],
                    address[2],
                    address[3]
                );
            }
            _ => return Err(format!("a.test status {}", a.status)),
        }

        let aaaa = query(&client, handle, "aaaa.test", QTYPE_AAAA)?;
        match (
            aaaa.status,
            aaaa.records.first().map(|record| record.address),
        ) {
            (STATUS_OK, Some(Address::V6(address))) => {
                raven_rt::println!(
                    "b15-resolv: aaaa.test={:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
                    u16::from_be_bytes([address[0], address[1]]),
                    u16::from_be_bytes([address[2], address[3]]),
                    u16::from_be_bytes([address[4], address[5]]),
                    u16::from_be_bytes([address[6], address[7]]),
                    u16::from_be_bytes([address[8], address[9]]),
                    u16::from_be_bytes([address[10], address[11]]),
                    u16::from_be_bytes([address[12], address[13]]),
                    u16::from_be_bytes([address[14], address[15]]),
                );
            }
            _ => return Err(format!("aaaa.test status {}", aaaa.status)),
        }

        let alias = query(&client, handle, "alias.test", QTYPE_A)?;
        match (
            alias.status,
            alias.records.first().map(|record| record.address),
        ) {
            (STATUS_OK, Some(Address::V4(address))) => {
                raven_rt::println!(
                    "b15-resolv: alias.test={}.{}.{}.{}",
                    address[0],
                    address[1],
                    address[2],
                    address[3]
                );
            }
            _ => return Err(format!("alias.test status {}", alias.status)),
        }

        let missing = query(&client, handle, "missing.test", QTYPE_A)?;
        if missing.status != STATUS_NXDOMAIN {
            return Err(format!("missing.test status {}", missing.status));
        }
        raven_rt::println!("b15-resolv: missing.test=NXDOMAIN");
        let _ = client.close(handle);
        Ok(())
    }

    fn query(
        client: &Client,
        handle: raven_abi::types::Handle,
        name: &str,
        qtype: u8,
    ) -> Result<Reply, String> {
        let query = Query { qtype, name };
        let mut encoded = [0u8; 512];
        let length = query
            .encode(&mut encoded)
            .map_err(|_| "encode query".to_string())?;
        let mut response = [0u8; MAX_PACKET];
        for attempt in 0..20 {
            let length = client
                .call(handle, &encoded[..length], &mut response)
                .map_err(|error| format!("query {name}: {}", error.description()))?;
            let reply =
                Reply::decode(&response[..length]).map_err(|_| format!("decode {name} reply"))?;
            if reply.status != 2 {
                return Ok(reply);
            }
            if attempt != 19 {
                let _ = raven_rt::nanosleep(100_000_000);
            }
        }
        Err(format!("{name}: resolver returned error"))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
