// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Approved RustCrypto backend: known vectors and signed TCP round trip.

#![cfg(all(feature = "testutil", feature = "tcp-md5", feature = "tcp-ao"))]

use raven_tcpip::{
    AoAlgorithm, AoKey, DigestProvider, Instant, IpAddr, Ipv4Addr, Ipv4Config, NoDigest, Stack,
    StackConfig, hmac_sha1, hmac_sha256, md5,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetFrame};
use raven_tcpip_wire::ipv4::Ipv4Packet;
use raven_tcpip_wire::tcp::{TcpOption, TcpPacket};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn stack(endpoint: Endpoint, value: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    stack.test_set_trace(true);
    stack
}

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 131);
    let link = a.link();
    let mut a = stack(a, A, 1);
    let mut b = stack(b, B, 2);
    a.test_insert_neighbor(Ipv4Addr::new(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(Ipv4Addr::new(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 4).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    for time in [0, 50, 100, 150, 200] {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn");
    (socket, accepted)
}

fn tcp_options(link: &Link) -> Vec<Vec<u8>> {
    link.trace()
        .entries()
        .iter()
        .filter_map(|entry| {
            let ethernet = EthernetFrame::parse(&entry.bytes).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if ipv4.protocol() != 6 {
                return None;
            }
            let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
            let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
            let mut kinds = Vec::new();
            for option in tcp.options().flatten() {
                kinds.push(match option {
                    TcpOption::Md5 { .. } => 19,
                    TcpOption::Authentication { .. } => 29,
                    TcpOption::Timestamps { .. } => 8,
                    _ => 0,
                });
            }
            Some(kinds)
        })
        .collect()
}

#[test]
fn md5_empty_vector() {
    assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
}

#[test]
fn md5_abc_vector() {
    assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
}

#[test]
fn md5_message_digest_vector() {
    assert_eq!(
        hex(&md5(b"message digest")),
        "f96b697d7cb7938d525a2f31aaf161d0"
    );
}

#[test]
fn hmac_sha1_rfc2202_vector_one() {
    let key = [0x0bu8; 20];
    assert_eq!(
        hex(&hmac_sha1(&key, b"Hi There")),
        "b617318655057264e28bc0b6fb378c8ef146be00"
    );
}

#[test]
fn hmac_sha1_rfc2202_vector_two() {
    assert_eq!(
        hex(&hmac_sha1(b"Jefe", b"what do ya want for nothing?")),
        "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
    );
}

#[test]
fn hmac_sha1_rfc2202_vector_three() {
    let key = [0xaau8; 20];
    assert_eq!(
        hex(&hmac_sha1(&key, &[0xddu8; 50])),
        "125d7342b9ac11cd91a39af48aa17b4f63f175d3"
    );
}

#[test]
fn hmac_sha256_rfc4231_vector_one() {
    let key = [0x0bu8; 20];
    assert_eq!(
        hex(&hmac_sha256(&key, b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn hmac_sha256_rfc4231_vector_two() {
    assert_eq!(
        hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn hmac_sha256_rfc4231_vector_three() {
    let key = [0xaau8; 20];
    assert_eq!(
        hex(&hmac_sha256(&key, &[0xddu8; 50])),
        "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"
    );
}

#[test]
fn md5_digest_is_sixteen_bytes() {
    assert_eq!(md5(b"x").len(), 16);
}

#[test]
fn hmac_sha1_digest_is_twenty_bytes() {
    assert_eq!(hmac_sha1(b"k", b"x").len(), 20);
}

#[test]
fn hmac_sha256_digest_is_thirty_two_bytes() {
    assert_eq!(hmac_sha256(b"k", b"x").len(), 32);
}

#[test]
fn hmac_accepts_an_empty_key() {
    assert_eq!(hmac_sha1(b"", b"x").len(), 20);
    assert_eq!(hmac_sha256(b"", b"x").len(), 32);
}

#[test]
fn no_digest_remains_a_zero_placeholder() {
    let digest = NoDigest;
    assert_eq!(digest.md5(b"x"), [0u8; 16]);
}

#[test]
fn ao_algorithm_values_are_distinct() {
    assert_ne!(AoAlgorithm::HmacSha1, AoAlgorithm::HmacSha256);
}

#[test]
fn ao_key_carries_id_algorithm_and_key() {
    let key = AoKey {
        key_id: 7,
        algorithm: AoAlgorithm::HmacSha1,
        key: vec![1, 2, 3],
    };
    assert_eq!(key.key_id, 7);
    assert_eq!(key.algorithm, AoAlgorithm::HmacSha1);
    assert_eq!(key.key, vec![1, 2, 3]);
}

#[test]
fn md5_signed_segment_contains_the_option() {
    let (link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    a.set_md5_key(sa, Some(b"secret")).expect("a key");
    b.set_md5_key(sb, Some(b"secret")).expect("b key");
    let start = link.trace().entries().len();
    a.send(sa, b"md5-data").expect("send");
    a.poll(Instant::from_millis(210));
    let options = tcp_options(&link);
    assert!(options.iter().skip(start).any(|kinds| kinds.contains(&19)));
    let mut nonzero = false;
    for entry in link.trace().entries().iter().skip(start) {
        let Ok(ethernet) = EthernetFrame::parse(&entry.bytes) else {
            continue;
        };
        let Ok(ipv4) = Ipv4Packet::parse(ethernet.payload()) else {
            continue;
        };
        let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
        let Ok(tcp) = TcpPacket::parse(ipv4.payload(), context) else {
            continue;
        };
        for option in tcp.options().flatten() {
            if let TcpOption::Md5 { digest } = option {
                nonzero |= digest.iter().any(|byte| *byte != 0);
            }
        }
    }
    assert!(nonzero, "MD5 digest is zero");
}

#[test]
fn md5_signed_data_reaches_the_peer() {
    let (_link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    a.set_md5_key(sa, Some(b"secret")).expect("a key");
    b.set_md5_key(sb, Some(b"secret")).expect("b key");
    a.send(sa, b"secure").expect("send");
    for time in 210..600 {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let mut buffer = [0u8; 16];
    let mut total = Vec::new();
    for _ in 0..20 {
        match b.recv(sb, &mut buffer) {
            Ok(0) => break,
            Ok(count) => total.extend_from_slice(&buffer[..count]),
            Err(_) => break,
        }
    }
    eprintln!(
        "sig failures {} debug {:?} q {:?}",
        b.test_signature_failures(),
        b.test_tcp_debug(),
        b.recv_queue_len(sb)
    );
    assert_eq!(total, b"secure");
}

#[test]
fn md5_bad_key_is_rejected_silently() {
    let (_link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    a.set_md5_key(sa, Some(b"wrong")).expect("a key");
    b.set_md5_key(sb, Some(b"right")).expect("b key");
    let _ = a.send(sa, b"bad");
    for time in 210..400 {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let mut buffer = [0u8; 8];
    assert!(b.recv(sb, &mut buffer).is_err() || b.recv(sb, &mut buffer) == Ok(0));
}

#[test]
fn ao_signed_segment_contains_the_option() {
    let (link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    let keys = [AoKey {
        key_id: 1,
        algorithm: AoAlgorithm::HmacSha1,
        key: b"ao-key".to_vec(),
    }];
    a.set_ao_keys(sa, &keys).expect("a keys");
    b.set_ao_keys(sb, &keys).expect("b keys");
    let start = link.trace().entries().len();
    a.send(sa, b"ao-data").expect("send");
    a.poll(Instant::from_millis(210));
    let options = tcp_options(&link);
    assert!(options.iter().skip(start).any(|kinds| kinds.contains(&29)));
}

#[test]
fn md5_key_removal_stops_signing() {
    let (link, mut a, mut b) = pair();
    let (sa, _sb) = handshake(&mut a, &mut b);
    a.set_md5_key(sa, Some(b"secret")).expect("key");
    a.set_md5_key(sa, None).expect("remove");
    let start = link.trace().entries().len();
    a.send(sa, b"plain").expect("send");
    a.poll(Instant::from_millis(210));
    let options = tcp_options(&link);
    assert!(!options.iter().skip(start).any(|kinds| kinds.contains(&19)));
}

#[test]
fn md5_is_deterministic() {
    assert_eq!(md5(b"same"), md5(b"same"));
}

#[test]
fn md5_differs_for_different_data() {
    assert_ne!(md5(b"left"), md5(b"right"));
}

#[test]
fn hmac_sha1_is_deterministic() {
    assert_eq!(hmac_sha1(b"k", b"data"), hmac_sha1(b"k", b"data"));
}

#[test]
fn hmac_sha256_is_deterministic() {
    assert_eq!(hmac_sha256(b"k", b"data"), hmac_sha256(b"k", b"data"));
}

#[test]
fn hmac_sha1_differs_for_different_keys() {
    assert_ne!(hmac_sha1(b"one", b"data"), hmac_sha1(b"two", b"data"));
}

#[test]
fn hmac_sha256_differs_for_different_data() {
    assert_ne!(hmac_sha256(b"k", b"one"), hmac_sha256(b"k", b"two"));
}

#[test]
fn md5_empty_key_is_supported() {
    assert_eq!(md5(b"").len(), 16);
}

#[test]
fn hmac_sha1_key_id_sized_keys_are_supported() {
    let key = [0x55u8; 64];
    assert_eq!(hmac_sha1(&key, b"payload").len(), 20);
}
