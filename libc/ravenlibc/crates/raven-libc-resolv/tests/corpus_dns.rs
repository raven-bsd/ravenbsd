// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic and randomized mutation corpus for every fuzz entry point (ticket 7.3).
//!
//! The test does not create a `cargo fuzz` target; it mutates valid messages and arbitrary
//! bytes and asserts that every shipped parser is panic-free with bounded output.

use std::panic::{AssertUnwindSafe, catch_unwind};

use raven_libc_resolv::{
    AF_INET, AF_INET6, AF_UNSPEC, IfNames, LineReader, dn_comp, dn_expand, dn_skipname, dns_parse,
    dns_parse_offset, inet_aton, inet_ntop, inet_pton, is_valid_hostname, lookup_ipliteral,
    ns_get16, ns_get32, ns_initparse, ns_parserr, pack_edns0, parse_hosts, parse_hosts_line,
    parse_net_line, parse_options_ext, parse_protocols, parse_resolv_conf, parse_serv_line,
    question_matches, res_mkquery,
};

struct NoIfNames;

impl IfNames for NoIfNames {
    fn name_to_index(&self, _name: &str) -> Option<u32> {
        None
    }
    fn index_to_name(&self, _idx: u32) -> Option<String> {
        None
    }
}

fn query(name: &[u8], qtype: u16) -> Vec<u8> {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, name, 1, qtype, 0x1234, &mut buf).expect("query");
    buf[..len].to_vec()
}

fn response(request: &[u8], records: &[(u16, &[u8])]) -> Vec<u8> {
    let mut out = request.to_vec();
    out[2] |= 0x80;
    out[3] = 0x80;
    out[6] = 0;
    out[7] = u8::try_from(records.len()).unwrap_or(0);
    for (rr_type, data) in records {
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&rr_type.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&60u32.to_be_bytes());
        out.extend_from_slice(&u16::try_from(data.len()).unwrap_or(0).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

fn seeds() -> Vec<Vec<u8>> {
    let a = query(b"example.org", 1);
    let aaaa = query(b"example.org", 28);
    let ptr = query(b"1.2.0.192.in-addr.arpa", 12);
    let mut compression = Vec::new();
    compression.extend_from_slice(&[1, b'F', 3, b'I', b'S', b'I', 4, b'A', b'R', b'P', b'A', 0]);
    compression.extend_from_slice(&[3, b'F', b'O', b'O', 0xc0, 0x00, 0xc0, 0x06]);
    vec![
        a.clone(),
        aaaa.clone(),
        ptr,
        response(&a, &[(1, &[192, 0, 2, 1])]),
        response(
            &aaaa,
            &[(
                28,
                &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            )],
        ),
        response(&a, &[(5, &[1, b'c', 0]), (1, &[192, 0, 2, 2])]),
        compression,
        vec![0u8; 12],
        b"nameserver 127.0.0.1\nsearch example.com\noptions ndots:2 timeout:3\n".to_vec(),
        b"127.0.0.1 localhost localhost.localdomain # comment\n::1 localhost\n".to_vec(),
        b"http 80/tcp www\nhttps 443/tcp\n".to_vec(),
        b"ip 0 IP\ntcp 6\nudp 17\n".to_vec(),
        b"loopback 127.0.0.0\n".to_vec(),
        b"0.0.0.0\0".to_vec(),
        b"::1\0".to_vec(),
    ]
}

fn exercise(data: &[u8], query: &[u8]) {
    let ifnames = NoIfNames;
    let positions = [0usize, 1, 12, data.len() / 2, data.len().saturating_sub(1)];
    for position in positions {
        let mut out = [0u8; 256];
        if let Some((_, len)) = dn_expand(data, position, &mut out) {
            assert!(len <= 254);
        }
        let _ = dn_skipname(data, position);
    }

    let mut records = 0usize;
    let _ = dns_parse(data, |_, rdata, packet| {
        records = records.saturating_add(1);
        assert!(rdata.len() <= packet.len());
        0
    });
    assert!(records <= 65535);

    let _ = dns_parse_offset(data, |_, rdata, packet, offset| {
        assert!(offset <= packet.len());
        assert!(rdata.len() <= packet.len());
        0
    });

    if let Some(mut handle) = ns_initparse(data) {
        for section in 0..4i32 {
            let count = handle.count(section).unwrap_or(0);
            for rrnum in 0..i32::from(count.min(64)) {
                if let Some(rr) = ns_parserr(&mut handle, section, rrnum) {
                    assert!(rr.name.len() <= 254);
                    assert!(rr.rdata.len() <= data.len());
                }
            }
        }
    }

    let _ = question_matches(query, data);
    let _ = question_matches(data, query);

    let conf = parse_resolv_conf(Some(data));
    assert!(conf.nns <= 3);
    assert!(conf.ndots <= 15);
    assert!(conf.timeout <= 60);
    assert!(conf.attempts <= 10);
    let _ = parse_resolv_conf(None);
    let _ = parse_options_ext(data);

    let entries = parse_hosts(data, &ifnames);
    assert!(entries.len() <= data.len().saturating_add(1));
    let _ = parse_hosts_line(data, &ifnames);
    let _ = parse_protocols(data);
    let mut reader = LineReader::new(data, 4096);
    while let Some(line) = reader.next_line() {
        let _ = parse_serv_line(line);
        let _ = parse_net_line(line);
        let _ = parse_hosts_line(line, &ifnames);
    }

    let _ = inet_aton(data);
    let mut address = [0u8; 16];
    let _ = inet_pton(AF_INET, data, &mut address);
    let _ = inet_pton(AF_INET6, data, &mut address);
    let mut text = [0u8; 64];
    if let Ok(len) = inet_ntop(AF_INET, data, &mut text) {
        assert!(len <= text.len());
    }
    if let Ok(len) = inet_ntop(AF_INET6, data, &mut text) {
        assert!(len <= text.len());
    }
    let _ = lookup_ipliteral(data, AF_UNSPEC, &ifnames);
    let _ = is_valid_hostname(&String::from_utf8_lossy(data));

    let mut query_buffer = [0u8; 280];
    if let Some(len) = res_mkquery(0, data, 1, 1, 0xffff, &mut query_buffer) {
        assert!(len <= query_buffer.len());
    }
    let mut compressed = [0u8; 280];
    if let Some(len) = dn_comp(data, &mut compressed, &mut [], None) {
        assert!(len <= compressed.len());
    }
    let mut edns = [0u8; 16];
    assert_eq!(pack_edns0(1232, true, &mut edns), Some(11));
    let _ = ns_get16(data);
    let _ = ns_get32(data);
}

#[test]
fn the_seed_corpus_is_exercised() {
    let queries = seeds();
    let base = query(b"example.org", 1);
    for seed in queries.iter() {
        exercise(seed, &base);
    }
}

#[test]
fn truncation_at_every_length() {
    let queries = seeds();
    let base = query(b"example.org", 1);
    for seed in queries.iter() {
        for length in 0..=seed.len() {
            exercise(&seed[..length], &base);
        }
    }
}

#[test]
fn every_byte_flipped_once() {
    let queries = seeds();
    let base = query(b"example.org", 1);
    for seed in queries.iter() {
        for index in 0..seed.len() {
            let mut mutated = seed.clone();
            mutated[index] ^= 0xff;
            exercise(&mutated, &base);
            mutated[index] = 0;
            exercise(&mutated, &base);
            mutated[index] = 0xff;
            exercise(&mutated, &base);
        }
    }
}

#[test]
fn every_count_and_length_field_set_to_zero_one_and_max() {
    let queries = seeds();
    let base = query(b"example.org", 1);
    for seed in queries.iter() {
        for index in 0..seed.len().min(12) {
            for value in [0u8, 1, 0xff] {
                let mut mutated = seed.clone();
                mutated[index] = value;
                exercise(&mutated, &base);
            }
        }
    }
}

#[test]
fn rdlength_past_the_end_is_rejected() {
    let a = query(b"example.org", 1);
    let mut packet = response(&a, &[(1, &[192, 0, 2, 1])]);
    let len = packet.len();
    packet[len - 6] = 0;
    packet[len - 5] = 0xff;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
    exercise(&packet, &a);
}

#[test]
fn ancount_and_qdcount_past_the_end_are_rejected() {
    let a = query(b"example.org", 1);
    let mut packet = response(&a, &[(1, &[192, 0, 2, 1])]);
    packet[7] = 0xff;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
    let mut packet = response(&a, &[(1, &[192, 0, 2, 1])]);
    packet[5] = 0xff;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
    exercise(&packet, &a);
}

#[test]
fn a_self_pointer_and_a_pair_are_rejected() {
    let mut out = [0u8; 256];
    assert!(dn_expand(&[0xc0, 0x00], 0, &mut out).is_none());
    assert!(dn_expand(&[0xc0, 0x02, 0xc0, 0x00], 0, &mut out).is_none());
}

#[test]
fn a_three_hundred_pointer_chain_is_rejected() {
    let mut message = vec![0u8; 600];
    for index in 0..300usize {
        message[index * 2] = 0xc0;
        message[index * 2 + 1] = u8::try_from((index + 1) * 2).unwrap_or(0);
    }
    let mut out = [0u8; 256];
    assert!(dn_expand(&message, 0, &mut out).is_none());
    exercise(&message, &[]);
}

#[test]
fn an_oversized_label_is_rejected() {
    let mut message = vec![63u8; 1];
    message.extend_from_slice(&[b'a'; 63]);
    message.push(64);
    message.extend_from_slice(&[b'b'; 64]);
    message.push(0);
    let mut out = [0u8; 256];
    assert!(dn_expand(&message, 0, &mut out).is_none());
    exercise(&message, &[]);
}

fn xoshiro(state: &mut [u64; 4]) -> u64 {
    let result = state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let t = state[1] << 17;
    state[2] ^= state[0];
    state[3] ^= state[1];
    state[1] ^= state[2];
    state[0] ^= state[3];
    state[2] ^= t;
    state[3] = state[3].rotate_left(45);
    result
}

fn mutate(seed: &[u8], rng: &mut [u64; 4]) -> Vec<u8> {
    let mut out = seed.to_vec();
    if out.is_empty() {
        out.push(0);
    }
    let operations = 1 + (xoshiro(rng) % 8) as usize;
    for _ in 0..operations {
        let choice = xoshiro(rng) % 6;
        match choice {
            0 => {
                let index = (xoshiro(rng) as usize) % out.len();
                out[index] ^= u8::try_from(xoshiro(rng) & 0xff).unwrap_or(1);
            }
            1 => {
                let index = (xoshiro(rng) as usize) % out.len();
                out[index] = 0;
            }
            2 => {
                let index = (xoshiro(rng) as usize) % out.len();
                out[index] = 0xff;
            }
            3 => {
                let length = (xoshiro(rng) as usize) % out.len().saturating_add(1);
                out.truncate(length);
                if out.is_empty() {
                    out.push(0);
                }
            }
            4 => {
                let index = (xoshiro(rng) as usize) % out.len();
                out.insert(index, u8::try_from(xoshiro(rng) & 0xff).unwrap_or(0));
                if out.len() > 1024 {
                    out.truncate(1024);
                }
            }
            _ => {
                let index = (xoshiro(rng) as usize) % out.len();
                out[index] = u8::try_from(xoshiro(rng) & 0x0f).unwrap_or(0);
            }
        }
    }
    out
}

#[test]
fn two_hundred_thousand_xoshiro_mutations() {
    let corpus = seeds();
    let base = query(b"example.org", 1);
    let mut rng: [u64; 4] = [
        0x5eed_1234_abcd_9876,
        0x0123_4567_89ab_cdef,
        0xfedc_ba98_7654_3210,
        0xdead_beef_cafe_babe,
    ];
    for iteration in 0..200_000usize {
        let index = (xoshiro(&mut rng) as usize) % corpus.len();
        let mutated = mutate(&corpus[index].clone(), &mut rng);
        let outcome = catch_unwind(AssertUnwindSafe(|| exercise(&mutated, &base)));
        if outcome.is_err() {
            panic!(
                "mutation panicked at iteration {iteration}, corpus seed {index}, master seed 0x5eed1234abcd9876, rng state {rng:?}, bytes {mutated:02x?}"
            );
        }
    }
}

#[test]
fn a_mutated_packet_never_exceeds_the_bounded_outputs() {
    let base = query(b"example.org", 1);
    let mut rng: [u64; 4] = [1, 2, 3, 4];
    for _ in 0..2_000usize {
        let mutated = mutate(&base, &mut rng);
        let mut out = [0u8; 256];
        if let Some((_, len)) = dn_expand(&mutated, 12, &mut out) {
            assert!(len <= 254);
        }
        let mut records = 0usize;
        let _ = dns_parse(&mutated, |_, _, _| {
            records += 1;
            0
        });
        assert!(records <= 65535);
    }
}
