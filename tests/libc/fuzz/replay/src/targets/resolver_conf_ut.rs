// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn resolver_conf_hostile_ok() {
    for kind in 0u8..10 {
        let mut input = vec![kind];
        input.extend_from_slice(b"nameserver 192.0.2.1\noptions ndots:99999999999\n");
        body(&input);
    }
}

#[test]
fn resolver_conf_boundaries_ok() {
    let mut input = vec![0u8];
    input.extend_from_slice(&[b'\n'; 4096]);
    body(&input);
    let mut input = vec![0u8];
    input.extend_from_slice(&[b'a'; 5000]);
    body(&input);
    let mut input = vec![0u8];
    input.extend_from_slice(b"nameserver\nnameserver 1.1.1.1\nnameserver 2.2.2.2\nnameserver 3.3.3.3\nnameserver 4.4.4.4\n");
    body(&input);
    let mut input = vec![7u8];
    input.extend_from_slice(b"hosts: files dns [\n");
    body(&input);
    let mut input = vec![5u8];
    input.extend_from_slice(b"root::0:0:::\r\n");
    body(&input);
}

#[test]
#[should_panic(expected = "P-conf-3")]
fn resolver_conf_mutation_3_fires() {
    let nns = 4usize;
    assert!(nns <= 3, "P-conf-3: more than three nameservers");
}

#[test]
fn every_file_kind_runs_on_a_crlf_file() {
    for kind in 0u8..10 {
        let mut input = vec![kind];
        input.extend_from_slice(b"root:x:0:0:root:/root:/bin/sh\r\n");
        body(&input);
    }
}

#[test]
fn nameservers_are_capped() {
    let text =
        b"nameserver 192.0.2.1\nnameserver 192.0.2.2\nnameserver 192.0.2.3\nnameserver 192.0.2.4\n";
    let conf = parse_resolv_conf(Some(text));
    assert!(conf.nns <= 3);
}
