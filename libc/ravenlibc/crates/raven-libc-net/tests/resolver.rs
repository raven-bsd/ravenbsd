// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end resolver tests over the fake netd.

use std::collections::BTreeMap;

use raven_libc_net::netdb::getaddrinfo;
use raven_libc_net::{Caps, FakeNetd, Net};
use raven_libc_nss::FileSource;
use raven_libc_resolv::Hints;

struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    fn new() -> Self {
        let mut files = BTreeMap::new();
        files.insert(
            "/etc/resolv.conf".to_string(),
            b"nameserver 192.0.2.7\noptions timeout:2 attempts:2\n".to_vec(),
        );
        Self(files)
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, max: usize) -> Result<Vec<u8>, i32> {
        self.0
            .get(path)
            .filter(|data| data.len() <= max)
            .cloned()
            .ok_or(2)
    }
}

fn net(caps: Caps) -> (Net<FakeNetd, FakeNetd, FakeNetd>, FakeNetd) {
    let fake = FakeNetd::with_caps(caps);
    let net = Net::new(fake.clone(), fake.clone(), fake.clone());
    (net, fake)
}

#[test]
fn getaddrinfo_resolves_over_the_fake_upstream() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"example.org", 1, &[192, 0, 2, 1]);
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("example.org"), Some("80"), &hints).expect("gai");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].addr.port, 80);
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 1]);
    assert!(
        fake.log()
            .iter()
            .any(|entry| matches!(entry, raven_libc_net::fake::WireLog::Open(_)))
    );
}

#[test]
fn getaddrinfo_bare_open_mode() {
    let caps = Caps {
        bare_open: true,
        ..Caps::default()
    };
    let (net, fake) = net(caps);
    fake.add_dns_answer(b"example.org", 1, &[203, 0, 113, 5]);
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("example.org"), Some("443"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[203, 0, 113, 5]);
    assert_eq!(result[0].addr.port, 443);
}

#[test]
fn getaddrinfo_dropped_reply_retries() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"retry.example", 1, &[192, 0, 2, 9]);
    fake.drop_first_dns_reply();
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("retry.example"), Some("80"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 9]);
}

#[test]
fn getaddrinfo_servfail_then_success() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"servfail.example", 1, &[192, 0, 2, 10]);
    fake.servfail_then_success();
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result =
        getaddrinfo(&net, &files, Some("servfail.example"), Some("80"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 10]);
}

#[test]
fn resolv_conf_is_read_per_lookup() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"one.example", 1, &[192, 0, 2, 11]);
    fake.add_dns_answer(b"two.example", 1, &[192, 0, 2, 12]);
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let first = getaddrinfo(&net, &files, Some("one.example"), Some("80"), &hints).expect("one");
    let second = getaddrinfo(&net, &files, Some("two.example"), Some("80"), &hints).expect("two");
    assert_eq!(first[0].addr.ip.as_bytes(), &[192, 0, 2, 11]);
    assert_eq!(second[0].addr.ip.as_bytes(), &[192, 0, 2, 12]);
    assert_ne!(first[0].addr.ip, second[0].addr.ip);
}

#[test]
fn dns_io_direct() {
    use raven_libc_resolv::{DnsIo, Family, IpAddr, SockAddr};
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"example.org", 1, &[192, 0, 2, 1]);
    let dns = net.dns_io();
    let sock = dns.open_udp(Family::V4).expect("open");
    let mut query = vec![0u8; 29];
    query[0] = 0x12;
    query[1] = 0x34;
    query[2] = 1;
    query[4] = 0;
    query[5] = 1;
    query[12] = 7;
    query[13..20].copy_from_slice(b"example");
    query[20] = 3;
    query[21..24].copy_from_slice(b"org");
    query[24] = 0;
    query[25] = 0;
    query[26] = 1;
    query[27] = 0;
    query[28] = 1;
    dns.send_to(sock, &query, &SockAddr::new(IpAddr::V4([192, 0, 2, 7]), 53))
        .expect("send");
    let mut buf = [0u8; 512];
    let recv = dns.recv_from(sock, &mut buf).expect("recv");
    println!("RECV {} {:?}", recv.len, recv.from);
    assert!(recv.len > 0);
}

#[test]
fn wrong_source_is_ignored_then_the_retry_succeeds() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"source.example", 1, &[192, 0, 2, 20]);
    fake.wrong_dns_source();
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result =
        getaddrinfo(&net, &files, Some("source.example"), Some("80"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 20]);
}

#[test]
fn wrong_id_is_ignored_then_the_retry_succeeds() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"id.example", 1, &[192, 0, 2, 21]);
    fake.wrong_dns_id();
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("id.example"), Some("80"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 21]);
}

#[test]
fn truncated_udp_falls_back_to_tcp() {
    let (net, fake) = net(Caps::default());
    fake.add_dns_answer(b"big.example", 1, &[192, 0, 2, 22]);
    fake.truncate_first_udp_reply();
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("big.example"), Some("80"), &hints).expect("gai");
    assert_eq!(result[0].addr.ip.as_bytes(), &[192, 0, 2, 22]);
    assert!(fake
        .log()
        .iter()
        .any(|entry| matches!(entry, raven_libc_net::fake::WireLog::Open(path) if path.starts_with(b"net:tcp/"))));
}

#[test]
fn bare_mode_uses_distinct_ephemeral_ports_and_never_reuses_a_socket() {
    let caps = Caps {
        bare_open: true,
        ..Caps::default()
    };
    let (net, fake) = net(caps);
    fake.add_dns_answer(b"one.example", 1, &[192, 0, 2, 30]);
    fake.add_dns_answer(b"two.example", 1, &[192, 0, 2, 31]);
    let files = Files::new();
    let hints = Hints {
        family: 2,
        socktype: 1,
        ..Hints::default()
    };
    let first = getaddrinfo(&net, &files, Some("one.example"), Some("80"), &hints).expect("one");
    let second = getaddrinfo(&net, &files, Some("two.example"), Some("80"), &hints).expect("two");
    assert_eq!(first[0].addr.ip.as_bytes(), &[192, 0, 2, 30]);
    assert_eq!(second[0].addr.ip.as_bytes(), &[192, 0, 2, 31]);
    let opens = fake
        .log()
        .iter()
        .filter(|entry| matches!(entry, raven_libc_net::fake::WireLog::Open(path) if path == b"net:udp"))
        .count();
    assert!(opens >= 2);
    let mut ports = fake.udp_local_ports();
    ports.sort_unstable();
    ports.dedup();
    assert!(ports.len() >= 2);
}

#[test]
fn addrconfig_without_ipv6_returns_ipv4_localhost_only() {
    let (net, fake) = net(Caps::default());
    let _ = fake;
    let files = Files::new();
    let hints = Hints {
        flags: raven_libc_resolv::consts::AI_ADDRCONFIG,
        family: 0,
        socktype: 1,
        ..Hints::default()
    };
    let result = getaddrinfo(&net, &files, Some("localhost"), Some("80"), &hints).expect("gai");
    assert!(result.iter().all(|record| record.family == 2));
    assert_eq!(result[0].addr.ip.as_bytes(), &[127, 0, 0, 1]);
}
