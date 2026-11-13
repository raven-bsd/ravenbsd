// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the service lookup.

use super::*;
use crate::consts::SOCK_RAW;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use raven_libc_abi::errno::{EACCES, EIO, ENOENT};
use raven_libc_nss::FileSource;

struct Files {
    data: BTreeMap<String, Result<Vec<u8>, crate::Errno>>,
}

impl Files {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(String::from(path), Ok(bytes.to_vec()));
    }

    fn error(&mut self, path: &str, errno: crate::Errno) {
        self.data.insert(String::from(path), Err(errno));
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, crate::Errno> {
        self.data.get(path).cloned().unwrap_or(Err(ENOENT))
    }
}

fn services() -> Files {
    let mut files = Files::new();
    files.put(
        "/etc/services",
        b"# comment\nhttp\t\t80/tcp\t\twww\nhttp\t\t80/udp\nhttps\t\t443/tcp\n\
          domain\t\t53/udp\nssh\t\t22/tcp\n",
    );
    files
}

#[test]
fn a_numeric_port_yields_both_socktypes() {
    let found = lookup_serv(&services(), Some("80"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].port, 80);
    assert_eq!(found[0].proto, IPPROTO_TCP);
    assert_eq!(found[0].socktype, SOCK_STREAM);
    assert_eq!(found[1].proto, IPPROTO_UDP);
    assert_eq!(found[1].socktype, SOCK_DGRAM);
}

#[test]
fn a_numeric_port_with_tcp_yields_one() {
    let found = lookup_serv(&services(), Some("80"), IPPROTO_TCP, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].proto, IPPROTO_TCP);
}

#[test]
fn a_numeric_port_with_udp_yields_one() {
    let found = lookup_serv(&services(), Some("80"), IPPROTO_UDP, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].proto, IPPROTO_UDP);
}

#[test]
fn a_numeric_port_above_65535_is_rejected() {
    assert_eq!(
        lookup_serv(&services(), Some("65536"), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn an_empty_service_name_is_rejected() {
    assert_eq!(
        lookup_serv(&services(), Some(""), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_service_name_is_found() {
    let found = lookup_serv(&services(), Some("http"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].port, 80);
}

#[test]
fn an_alias_matches_as_a_whole_word() {
    let found = lookup_serv(&services(), Some("www"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].port, 80);
}

#[test]
fn a_substring_does_not_match() {
    assert_eq!(
        lookup_serv(&services(), Some("htt"), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_tcp_service_is_filtered_for_udp() {
    let found = lookup_serv(&services(), Some("domain"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].proto, IPPROTO_UDP);
    assert_eq!(
        lookup_serv(&services(), Some("domain"), IPPROTO_TCP, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_udp_service_is_filtered_for_tcp() {
    assert_eq!(
        lookup_serv(&services(), Some("domain"), IPPROTO_TCP, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn numericserv_rejects_a_name() {
    assert_eq!(
        lookup_serv(&services(), Some("http"), 0, 0, AI_NUMERICSERV),
        Err(Eai::NoName)
    );
}

#[test]
fn a_missing_services_file_is_service() {
    let files = Files::new();
    assert_eq!(
        lookup_serv(&files, Some("http"), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn an_eacces_services_file_is_service() {
    let mut files = Files::new();
    files.error("/etc/services", EACCES);
    assert_eq!(
        lookup_serv(&files, Some("http"), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn another_file_error_is_system() {
    let mut files = Files::new();
    files.error("/etc/services", EIO);
    assert_eq!(lookup_serv(&files, Some("http"), 0, 0, 0), Err(Eai::System));
}

#[test]
fn a_stream_with_a_bad_protocol_is_rejected() {
    assert_eq!(
        lookup_serv(&services(), Some("80"), 99, SOCK_STREAM, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_dgram_with_a_bad_protocol_is_rejected() {
    assert_eq!(
        lookup_serv(&services(), Some("80"), 99, SOCK_DGRAM, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_raw_socktype_with_a_name_is_rejected() {
    assert_eq!(
        lookup_serv(&services(), Some("http"), 0, SOCK_RAW, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_raw_socktype_without_a_name_yields_one_record() {
    let found = lookup_serv(&services(), None, 0, SOCK_RAW, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].port, 0);
    assert_eq!(found[0].socktype, SOCK_RAW);
}

#[test]
fn at_most_two_records_are_returned() {
    let mut files = Files::new();
    files.put("/etc/services", b"x 1111/tcp\nx 2222/tcp\nx 3333/tcp\n");
    let found = lookup_serv(&files, Some("x"), IPPROTO_TCP, 0, 0).expect("serv");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].port, 1111);
    assert_eq!(found[1].port, 2222);
}

#[test]
fn an_over_long_services_line_is_skipped_whole() {
    let mut file = alloc::vec![b'y'; 300];
    file.extend_from_slice(b"\nhttp 80/tcp\n");
    let mut files = Files::new();
    files.put("/etc/services", &file);
    let found = lookup_serv(&files, Some("http"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].port, 80);
}

#[test]
fn a_line_without_a_slash_is_ignored() {
    let mut files = Files::new();
    files.put("/etc/services", b"http 80\nhttp 81/tcp\n");
    let found = lookup_serv(&files, Some("http"), 0, 0, 0).expect("serv");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].port, 81);
}

#[test]
fn a_partial_numeric_name_is_treated_as_a_name() {
    assert_eq!(
        lookup_serv(&services(), Some("80abc"), 0, 0, 0),
        Err(Eai::Service)
    );
}

#[test]
fn a_comment_is_ignored() {
    let mut files = Files::new();
    files.put("/etc/services", b"# http 80/tcp\nhttp 81/tcp\n");
    let found = lookup_serv(&files, Some("http"), 0, 0, 0).expect("serv");
    assert_eq!(found[0].port, 81);
}
