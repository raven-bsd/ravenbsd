// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the servent/protoent API and the packing helpers.

use super::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use raven_libc_abi::errno::{EINVAL, ENOENT, ERANGE};
use raven_libc_nss::FileSource;

struct Files {
    data: BTreeMap<String, Vec<u8>>,
}

impl Files {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(String::from(path), bytes.to_vec());
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, crate::Errno> {
        self.data.get(path).cloned().ok_or(ENOENT)
    }
}

fn files() -> Files {
    let mut files = Files::new();
    files.put(
        "/etc/services",
        b"http\t80/tcp\twww\nhttp\t80/udp\nhttps\t443/tcp\n",
    );
    files
}

#[test]
fn proto_names_numbers_and_socktypes() {
    assert_eq!(Proto::Tcp.name(), "tcp");
    assert_eq!(Proto::Tcp.number(), 6);
    assert_eq!(Proto::Tcp.socktype(), 1);
    assert_eq!(Proto::Udp.name(), "udp");
    assert_eq!(Proto::Udp.number(), 17);
    assert_eq!(Proto::Udp.socktype(), 2);
}

#[test]
fn proto_from_name_is_case_insensitive() {
    assert_eq!(Proto::from_name("TCP"), Some(Proto::Tcp));
    assert_eq!(Proto::from_name("Udp"), Some(Proto::Udp));
    assert_eq!(Proto::from_name("sctp"), None);
}

#[test]
fn proto_from_number() {
    assert_eq!(Proto::from_number(6), Some(Proto::Tcp));
    assert_eq!(Proto::from_number(17), Some(Proto::Udp));
    assert_eq!(Proto::from_number(132), None);
}

#[test]
fn the_builtin_table_has_the_donor_entries() {
    assert_eq!(BUILTIN_PROTOCOLS.len(), 35);
    assert_eq!(BUILTIN_PROTOCOLS[0], ("ip", 0));
    assert_eq!(BUILTIN_PROTOCOLS[1], ("icmp", 1));
    assert_eq!(BUILTIN_PROTOCOLS[9], ("udp", 17));
}

#[test]
fn builtin_protocol_number_finds_tcp_and_udp() {
    assert_eq!(builtin_protocol_number("tcp"), Some(6));
    assert_eq!(builtin_protocol_number("UDP"), Some(17));
    assert_eq!(builtin_protocol_number("nope"), None);
}

#[test]
fn parse_protocols_parses_names_numbers_and_aliases() {
    let entries = parse_protocols(b"# comment\nip\t0\tIP\nicmp\t1\n");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "ip");
    assert_eq!(entries[0].proto, 0);
    assert_eq!(entries[0].aliases, alloc::vec![String::from("IP")]);
    assert_eq!(entries[1].proto, 1);
}

#[test]
fn parse_protocols_skips_over_long_lines() {
    let mut file = alloc::vec![b'x'; 300];
    file.extend_from_slice(b"\ntcp 6\n");
    let entries = parse_protocols(&file);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "tcp");
}

#[test]
fn getprotobyname_uses_the_file_first() {
    let mut files = Files::new();
    files.put("/etc/protocols", b"custom 200 CUSTOM\n");
    let found = getprotobyname(&files, "custom")
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.proto, 200);
    assert_eq!(found.aliases, alloc::vec![String::from("CUSTOM")]);
}

#[test]
fn getprotobyname_falls_back_to_the_builtin_table() {
    let mut files = Files::new();
    files.put("/etc/protocols", b"custom 200\n");
    let found = getprotobyname(&files, "tcp")
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.proto, 6);
}

#[test]
fn getprotobyname_finds_an_alias() {
    let mut files = Files::new();
    files.put("/etc/protocols", b"custom 200 alias\n");
    assert_eq!(
        getprotobyname(&files, "alias")
            .expect("lookup")
            .map(|entry| entry.proto),
        Some(200)
    );
}

#[test]
fn getprotobynumber_uses_the_file_first() {
    let mut files = Files::new();
    files.put("/etc/protocols", b"custom 200\n");
    assert_eq!(
        getprotobynumber(&files, 200)
            .expect("lookup")
            .map(|entry| entry.name),
        Some(String::from("custom"))
    );
}

#[test]
fn getprotobynumber_falls_back_to_the_builtin_table() {
    let files = Files::new();
    assert_eq!(
        getprotobynumber(&files, 6)
            .expect("lookup")
            .map(|entry| entry.name),
        Some(String::from("tcp"))
    );
}

#[test]
fn getprotobyname_returns_none_for_an_unknown_name() {
    let files = Files::new();
    assert_eq!(getprotobyname(&files, "nope").expect("lookup"), None);
}

#[test]
fn getservbyname_rejects_a_numeric_name() {
    assert_eq!(getservbyname(&files(), "80", None), Err(ENOENT));
}

#[test]
fn getservbyname_rejects_a_bad_protocol() {
    assert_eq!(getservbyname(&files(), "http", Some("sctp")), Err(EINVAL));
}

#[test]
fn getservbyname_finds_the_service_and_lists_the_name() {
    let found = getservbyname(&files(), "http", Some("tcp"))
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.port, 80);
    assert_eq!(found.proto, Proto::Tcp);
    assert_eq!(found.aliases, alloc::vec![String::from("http")]);
}

#[test]
fn getservbyname_returns_enoent_for_an_unknown_name() {
    assert_eq!(getservbyname(&files(), "nope", None), Err(ENOENT));
}

#[test]
fn getservbyport_finds_tcp() {
    let found = getservbyport(&files(), 80, Some("tcp"))
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.name, "http");
    assert_eq!(found.proto, Proto::Tcp);
}

#[test]
fn getservbyport_finds_udp() {
    let found = getservbyport(&files(), 80, Some("udp"))
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.name, "http");
    assert_eq!(found.proto, Proto::Udp);
}

#[test]
fn getservbyport_without_a_protocol_tries_tcp_then_udp() {
    let found = getservbyport(&files(), 443, None)
        .expect("lookup")
        .expect("entry");
    assert_eq!(found.name, "https");
    assert_eq!(found.proto, Proto::Tcp);
}

#[test]
fn getservbyport_rejects_a_bad_protocol() {
    assert_eq!(getservbyport(&files(), 80, Some("sctp")), Err(EINVAL));
}

#[test]
fn getservbyport_returns_enoent_for_an_unknown_port() {
    assert_eq!(getservbyport(&files(), 9999, Some("tcp")), Err(ENOENT));
}

#[test]
fn service_name_reads_the_canonical_name() {
    assert_eq!(
        service_name(&files(), 80, Proto::Tcp).as_deref(),
        Some("http")
    );
    assert_eq!(
        service_name(&files(), 80, Proto::Udp).as_deref(),
        Some("http")
    );
    assert_eq!(service_name(&files(), 81, Proto::Tcp), None);
}

#[test]
fn pack_servent_writes_the_strings_and_tables() {
    let entry = Servent {
        name: String::from("http"),
        aliases: alloc::vec![String::from("www")],
        port: 80,
        proto: Proto::Tcp,
    };
    let mut buf = [0u8; 64];
    let packed = pack_servent(&entry, &mut buf).expect("pack");
    assert_eq!(packed.name_len, 4);
    assert_eq!(packed.port, 80);
    assert_eq!(packed.proto, 6);
    assert_eq!(&buf[..5], b"http\0");
    assert_eq!(
        &buf[packed.alias_table_off..packed.alias_table_off + 4],
        &5u32.to_le_bytes()
    );
    assert_eq!(&buf[5..9], b"www\0");
}

#[test]
fn pack_servent_returns_range_when_too_small() {
    let entry = Servent {
        name: String::from("http"),
        aliases: alloc::vec![String::from("www")],
        port: 80,
        proto: Proto::Tcp,
    };
    let mut buf = [0u8; 4];
    assert_eq!(pack_servent(&entry, &mut buf), Err(ERANGE));
}

#[test]
fn pack_protoent_writes_the_protocol_number() {
    let entry = Protoent {
        name: String::from("tcp"),
        aliases: Vec::new(),
        proto: 6,
    };
    let mut buf = [0u8; 32];
    let packed = pack_protoent(&entry, &mut buf).expect("pack");
    assert_eq!(packed.proto, 6);
    assert_eq!(packed.alias_count, 0);
    assert_eq!(&buf[..4], b"tcp\0");
}

#[test]
fn pack_protoent_returns_range_when_too_small() {
    let entry = Protoent {
        name: String::from("tcp"),
        aliases: Vec::new(),
        proto: 6,
    };
    assert_eq!(pack_protoent(&entry, &mut [0u8; 2]), Err(ERANGE));
}
