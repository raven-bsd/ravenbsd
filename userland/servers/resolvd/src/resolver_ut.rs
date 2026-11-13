// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Resolver loop tests with a scripted exchange.

use alloc::vec;
use alloc::vec::Vec;

use super::*;

struct Fake {
    responses: Vec<Result<Vec<u8>, Errno>>,
    calls: usize,
}

impl Fake {
    fn new(responses: Vec<Result<Vec<u8>, Errno>>) -> Self {
        Self {
            responses,
            calls: 0,
        }
    }
}

impl Exchange for Fake {
    fn query(
        &mut self,
        _server: [u8; 4],
        request: &[u8],
        response: &mut [u8],
        _timeout_ms: u32,
    ) -> Result<usize, Errno> {
        self.calls += 1;
        let result = self.responses.remove(0);
        let mut bytes = result?;
        if bytes.len() >= 2 && request.len() >= 2 {
            bytes[0..2].copy_from_slice(&request[0..2]);
        }
        response[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}

fn put_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

fn dns_response(id: u16, rcode: u16, answers: &[(u16, u32, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&(0x8180u16 | rcode).to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(answers.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    put_name(&mut out, "query.test");
    out.extend_from_slice(&dns::TYPE_A.to_be_bytes());
    out.extend_from_slice(&dns::CLASS_IN.to_be_bytes());
    for (kind, ttl, data) in answers {
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&dns::CLASS_IN.to_be_bytes());
        out.extend_from_slice(&ttl.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

fn id_of(request: &[u8]) -> u16 {
    u16::from_be_bytes([request[0], request[1]])
}

#[test]
fn resolves_an_a_record() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 500, 1);
    let response = dns_response(id_of(&[0, 1]), 0, &[(dns::TYPE_A, 60, &[192, 0, 2, 10])]);
    let mut fake = Fake::new(vec![Ok(response)]);
    let answers = resolver
        .resolve(&mut fake, "a.test", dns::TYPE_A)
        .expect("resolve");
    assert_eq!(answers, vec![(60, Address::V4([192, 0, 2, 10]))]);
    assert_eq!(fake.calls, 1);
}

#[test]
fn follows_one_cname() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 500, 1);
    let mut target = Vec::new();
    put_name(&mut target, "a.test");
    let first = dns_response(id_of(&[0, 1]), 0, &[(dns::TYPE_CNAME, 60, &target)]);
    let second = dns_response(id_of(&[0, 2]), 0, &[(dns::TYPE_A, 30, &[192, 0, 2, 11])]);
    let mut fake = Fake::new(vec![Ok(first), Ok(second)]);
    let answers = resolver
        .resolve(&mut fake, "alias.test", dns::TYPE_A)
        .expect("cname");
    assert_eq!(answers, vec![(30, Address::V4([192, 0, 2, 11]))]);
    assert_eq!(fake.calls, 2);
}

#[test]
fn nxdomain_stops_with_not_found() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 500, 1);
    let response = dns_response(id_of(&[0, 1]), 3, &[]);
    let mut fake = Fake::new(vec![Ok(response)]);
    assert_eq!(
        resolver.resolve(&mut fake, "missing.test", dns::TYPE_A),
        Err(ResolveError::NotFound)
    );
}

#[test]
fn one_retry_and_a_second_server() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 100, 2);
    let response = dns_response(id_of(&[0, 1]), 0, &[(dns::TYPE_A, 60, &[192, 0, 2, 12])]);
    let mut fake = Fake::new(vec![Err(Errno::ETIMEDOUT), Ok(response)]);
    let answers = resolver
        .resolve(&mut fake, "a.test", dns::TYPE_A)
        .expect("retry");
    assert_eq!(answers, vec![(60, Address::V4([192, 0, 2, 12]))]);
    assert_eq!(fake.calls, 2);

    let mut resolver = Resolver::new(vec![[127, 0, 0, 1], [192, 0, 2, 53]], 100, 1);
    let response = dns_response(id_of(&[0, 1]), 0, &[(dns::TYPE_A, 60, &[192, 0, 2, 13])]);
    let mut fake = Fake::new(vec![Err(Errno::ETIMEDOUT), Ok(response)]);
    let answers = resolver
        .resolve(&mut fake, "a.test", dns::TYPE_A)
        .expect("second server");
    assert_eq!(answers, vec![(60, Address::V4([192, 0, 2, 13]))]);
    assert_eq!(fake.calls, 2);
}

#[test]
fn timeouts_and_malformed_packets_are_reported() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 100, 1);
    let mut fake = Fake::new(vec![Err(Errno::ETIMEDOUT)]);
    assert_eq!(
        resolver.resolve(&mut fake, "a.test", dns::TYPE_A),
        Err(ResolveError::Timeout)
    );

    let bad = vec![0u8; 3];
    let mut fake = Fake::new(vec![Ok(bad)]);
    assert_eq!(
        resolver.resolve(&mut fake, "a.test", dns::TYPE_A),
        Err(ResolveError::Proto)
    );
}

#[test]
fn a_query_outside_a_or_aaaa_is_rejected() {
    let mut resolver = Resolver::new(vec![[127, 0, 0, 1]], 100, 1);
    let mut fake = Fake::new(vec![]);
    assert_eq!(
        resolver.resolve(&mut fake, "a.test", dns::TYPE_CNAME),
        Err(ResolveError::NoData)
    );
}
