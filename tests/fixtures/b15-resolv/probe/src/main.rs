// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Multi-phase `resolv:` client for the resolver qtest.
//!
//! `setup` writes the static override, `phase1` exercises the loopback DNS
//! fixture and clears the override, and `phase2` proves the DHCP lease DNS was
//! written to `/etc/resolv.conf` and is actually queried through QEMU user
//! networking.

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

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_scheme::Client;
    use resolvd::config;
    use resolvd::proto::{Address, QTYPE_A, QTYPE_AAAA, Query, Reply, STATUS_NXDOMAIN, STATUS_OK};

    entry!(main);

    const READ: u32 = 1;
    const MAX_PACKET: usize = 512;
    const CONF: &str = "file:/etc/resolv.conf";

    fn main(args: raven_rt::Args) -> i32 {
        let mode = args.get(1).unwrap_or("phase1");
        let result = match mode {
            "setup" => setup(),
            "phase1" => phase1(),
            "phase2" => phase2(),
            other => Err(format!("unknown mode {other}")),
        };
        match result {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("b15-resolv: {message}");
                1
            }
        }
    }

    fn setup() -> Result<(), String> {
        let text = read_text("initfs:/etc/b15-resolv.conf")?;
        write_text(CONF, &text)?;
        raven_rt::println!("b15-resolv: setup static nameserver 127.0.0.1");
        Ok(())
    }

    fn phase1() -> Result<(), String> {
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
                raven_rt::println!("b15-resolv: aaaa.test={}", ipv6_text(&address));
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
        truncate_text(CONF)?;
        raven_rt::println!("b15-resolv: phase1 ok");
        Ok(())
    }

    fn phase2() -> Result<(), String> {
        let config = read_applied_config()?;
        raven_rt::println!(
            "b15-resolv: resolv.conf={}",
            nameserver_lines(&config.servers)
        );

        let client = Client::new();
        let handle = client
            .open("resolv:", READ)
            .map_err(|error| format!("open resolv:: {}", error.description()))?;

        let localhost = query(&client, handle, "localhost", QTYPE_A)?;
        match (
            localhost.status,
            localhost.records.first().map(|record| record.address),
        ) {
            (STATUS_OK, Some(Address::V4(address))) if address == [127, 0, 0, 1] => {
                raven_rt::println!("b15-resolv: localhost=127.0.0.1");
            }
            (status, address) => {
                return Err(format!("localhost status {} answer {:?}", status, address));
            }
        }

        let localhost6 = query(&client, handle, "localhost", QTYPE_AAAA)?;
        match (
            localhost6.status,
            localhost6.records.first().map(|record| record.address),
        ) {
            (STATUS_OK, Some(Address::V6(address)))
                if address == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] =>
            {
                raven_rt::println!("b15-resolv: localhost6={}", ipv6_text(&address));
            }
            (status, address) => {
                return Err(format!(
                    "localhost AAAA status {} answer {:?}",
                    status, address
                ));
            }
        }

        let missing = query(&client, handle, "missing.invalid", QTYPE_A)?;
        if missing.status != STATUS_NXDOMAIN {
            return Err(format!("missing.invalid status {}", missing.status));
        }
        raven_rt::println!("b15-resolv: missing.invalid=NXDOMAIN");
        let _ = client.close(handle);
        raven_rt::println!("b15-resolv: dhcp probe ok");
        Ok(())
    }

    fn read_applied_config() -> Result<config::Config, String> {
        let mut last = String::new();
        for _ in 0..200 {
            if let Ok(text) = read_text(CONF) {
                let config = config::parse(&text);
                if !config.servers.is_empty() {
                    return Ok(config);
                }
                last = text;
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
        Err(format!("no nameserver in {CONF}: {last:?}"))
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

    fn read_text(path: &str) -> Result<String, String> {
        let data =
            raven_rt::fs::read_to_vec(path).map_err(|error| error.description().to_string())?;
        String::from_utf8(data).map_err(|_| format!("{path} is not UTF-8"))
    }

    fn write_text(path: &str, text: &str) -> Result<(), String> {
        let mut file = raven_rt::fs::open(path, O_WRONLY | O_CREAT | O_TRUNC)
            .map_err(|error| error.description().to_string())?;
        let mut offset = 0usize;
        while offset < text.len() {
            let written = file
                .write(&text.as_bytes()[offset..])
                .map_err(|error| error.description().to_string())?;
            if written == 0 {
                return Err(format!("write {path}: zero bytes"));
            }
            offset += written;
        }
        file.close()
            .map_err(|error| error.description().to_string())
    }

    fn truncate_text(path: &str) -> Result<(), String> {
        let file = raven_rt::fs::open(path, O_WRONLY | O_TRUNC)
            .map_err(|error| error.description().to_string())?;
        file.close()
            .map_err(|error| error.description().to_string())
    }

    fn ipv6_text(address: &[u8; 16]) -> String {
        let groups: Vec<String> = (0..8)
            .map(|index| {
                format!(
                    "{:x}",
                    u16::from_be_bytes([address[index * 2], address[index * 2 + 1]])
                )
            })
            .collect();
        groups.join(":")
    }

    fn nameserver_lines(servers: &[[u8; 4]]) -> String {
        servers
            .iter()
            .map(|server| {
                format!(
                    "nameserver {}.{}.{}.{}",
                    server[0], server[1], server[2], server[3]
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
