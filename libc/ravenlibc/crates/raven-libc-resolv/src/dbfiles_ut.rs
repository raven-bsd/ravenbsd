// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the iterating database files.

use super::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use raven_libc_abi::errno::ENOENT;
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
        b"http\t80/tcp\twww www2\nhttps\t443/tcp\nbootps\t67/udp\n",
    );
    files.put("/etc/protocols", b"ip\t0\tIP\nicmp\t1\n");
    files.put(
        "/etc/networks",
        b"loopback\t127.0.0.0\nlink-local\t169.254.0.0\n",
    );
    files
}

#[test]
fn parse_serv_line_reads_name_port_and_proto() {
    let entry = parse_serv_line(b"http 80/tcp www").expect("line");
    assert_eq!(entry.name, "http");
    assert_eq!(entry.port, 80);
    assert_eq!(entry.proto, Proto::Tcp);
    assert_eq!(entry.aliases, alloc::vec![String::from("www")]);
}

#[test]
fn parse_serv_line_reads_udp() {
    let entry = parse_serv_line(b"domain 53/udp").expect("line");
    assert_eq!(entry.proto, Proto::Udp);
}

#[test]
fn parse_serv_line_strips_comments() {
    let entry = parse_serv_line(b"http 80/tcp # web").expect("line");
    assert!(entry.aliases.is_empty());
}

#[test]
fn parse_serv_line_rejects_a_missing_name() {
    assert!(parse_serv_line(b"80/tcp").is_none());
}

#[test]
fn parse_serv_line_rejects_a_missing_port() {
    assert!(parse_serv_line(b"http").is_none());
}

#[test]
fn parse_serv_line_rejects_a_bad_port() {
    assert!(parse_serv_line(b"http notaport/tcp").is_none());
    assert!(parse_serv_line(b"http 99999/tcp").is_none());
}

#[test]
fn parse_serv_line_rejects_a_bad_protocol() {
    assert!(parse_serv_line(b"http 80/sctp").is_none());
}

#[test]
fn parse_net_line_reads_name_and_address() {
    let entry = parse_net_line(b"loopback 127.0.0.0").expect("line");
    assert_eq!(entry.name, "loopback");
    assert_eq!(entry.addr, u32::from_ne_bytes([127, 0, 0, 0]));
}

#[test]
fn parse_net_line_reads_aliases() {
    let entry = parse_net_line(b"loopback 127.0.0.0 lo").expect("line");
    assert_eq!(entry.aliases, alloc::vec![String::from("lo")]);
}

#[test]
fn parse_net_line_rejects_a_bad_address() {
    assert!(parse_net_line(b"loopback nope").is_none());
}

#[test]
fn serv_file_iterates_and_rewinds() {
    let files = files();
    let mut entries = ServFile::open(&files);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries.next_entry().expect("first").name, "http");
    assert_eq!(entries.next_entry().expect("second").name, "https");
    assert_eq!(entries.next_entry().expect("third").name, "bootps");
    assert!(entries.next_entry().is_none());
    entries.rewind();
    assert_eq!(entries.next_entry().expect("again").name, "http");
}

#[test]
fn serv_file_next_owned_returns_a_copy() {
    let files = files();
    let mut entries = ServFile::open(&files);
    let owned = entries.next_owned().expect("entry");
    assert_eq!(owned.name, "http");
}

#[test]
fn serv_file_finds_by_name_and_alias() {
    let files = files();
    let entries = ServFile::open(&files);
    assert_eq!(entries.by_name("http").map(|entry| entry.port), Some(80));
    assert_eq!(entries.by_name("www2").map(|entry| entry.port), Some(80));
    assert_eq!(entries.by_name("nope"), None);
}

#[test]
fn serv_file_finds_by_port_and_proto() {
    let files = files();
    let entries = ServFile::open(&files);
    assert_eq!(
        entries
            .by_port(443, Proto::Tcp)
            .map(|entry| entry.name.as_str()),
        Some("https")
    );
    assert_eq!(entries.by_port(443, Proto::Udp), None);
}

#[test]
fn serv_file_skips_over_long_lines() {
    let mut file = alloc::vec![b'x'; 300];
    file.extend_from_slice(b"\nhttp 80/tcp\n");
    let mut files = Files::new();
    files.put("/etc/services", &file);
    let entries = ServFile::open(&files);
    assert_eq!(entries.len(), 1);
}

#[test]
fn serv_file_missing_is_empty() {
    let files = Files::new();
    let entries = ServFile::open(&files);
    assert!(entries.is_empty());
    assert_eq!(entries.len(), 0);
}

#[test]
fn proto_file_iterates() {
    let files = files();
    let mut entries = ProtoFile::open(&files);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.next_entry().expect("first").name, "ip");
    assert_eq!(entries.next_owned().expect("second").proto, 1);
    assert!(entries.next_entry().is_none());
    entries.rewind();
    assert_eq!(entries.next_entry().expect("again").name, "ip");
}

#[test]
fn proto_file_missing_is_empty() {
    let files = Files::new();
    assert!(ProtoFile::open(&files).is_empty());
}

#[test]
fn net_file_iterates() {
    let files = files();
    let mut entries = NetFile::open(&files);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.next_entry().expect("first").name, "loopback");
    entries.rewind();
    assert_eq!(entries.next_entry().expect("again").name, "loopback");
    assert_eq!(entries.entries().len(), 2);
}

#[test]
fn net_file_missing_is_empty() {
    let files = Files::new();
    assert!(NetFile::open(&files).is_empty());
}

#[test]
fn the_getservent_wrappers_work() {
    let files = files();
    let mut state = setservent(&files);
    assert_eq!(getservent(&mut state).expect("first").name, "http");
    endservent(&mut state);
    assert!(getservent(&mut state).is_none());
    let mut again = setservent(&files);
    assert_eq!(getservent(&mut again).expect("again").name, "http");
}

#[test]
fn the_getprotoent_wrappers_work() {
    let files = files();
    let mut state = setprotoent(&files);
    assert_eq!(getprotoent(&mut state).expect("first").name, "ip");
    endprotoent(&mut state);
    assert!(getprotoent(&mut state).is_none());
    let mut again = setprotoent(&files);
    assert_eq!(getprotoent(&mut again).expect("again").name, "ip");
}

#[test]
fn the_getnetent_wrappers_work() {
    let files = files();
    let mut state = setnetent(&files);
    assert_eq!(getnetent(&mut state).expect("first").name, "loopback");
    endnetent(&mut state);
    assert!(getnetent(&mut state).is_none());
    let mut again = setnetent(&files);
    assert_eq!(getnetent(&mut again).expect("again").name, "loopback");
}

#[test]
fn service_of_maps_a_servent() {
    let entry = Servent {
        name: String::from("http"),
        aliases: Vec::new(),
        port: 80,
        proto: Proto::Udp,
    };
    let service = service_of(&entry);
    assert_eq!(service.port, 80);
    assert_eq!(service.proto, 17);
    assert_eq!(service.socktype, 2);
}

#[test]
fn serv_file_entries_are_exposed() {
    let files = files();
    let entries = ServFile::open(&files);
    assert_eq!(entries.entries().len(), 3);
    assert_eq!(entries.entries()[0].name, "http");
}
