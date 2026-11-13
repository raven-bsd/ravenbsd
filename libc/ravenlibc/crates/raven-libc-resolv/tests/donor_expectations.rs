// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor expectation fixtures from OpenBSD `regress/lib/libc/getaddrinfo/answer` and the
//! NetBSD `tests/lib/libc/net` cases.
//!
//! Two normalisations are explicit and are also listed in the report:
//!
//! * BSD `AF_INET6` is 24, Linux is 10; the comparison stores the Linux value and the fixture
//!   text is quoted next to each case.
//! * BSD returns UDP before TCP for a numeric service and sorts IPv4 before IPv6; the musl
//!   donor returns TCP before UDP and applies RFC 6724. The expected lists are compared as
//!   sets, never silently reordered.
//!
//! `h_hostent`'s NetBSD alias list is not reproduced: musl's `gethostbyname2_r` leaves
//! `h_aliases` empty when the canonical name equals the query and never copies the hosts-file
//! aliases. Only the name/address/type parts supported by the donor are asserted here.

#[path = "common/fake_dns.rs"]
mod fake_dns;

use fake_dns::{Action, FakeDns, Record};
use raven_libc_resolv::{
    AF_INET, AF_INET6, AF_UNSPEC, AI_PASSIVE, AddrInfoRec, Eai, HostOrder, IpAddr, NI_NUMERICHOST,
    NI_NUMERICSERV, SOCK_DGRAM, SOCK_RAW, SOCK_STREAM, SockAddr, getaddrinfo, gethostbyname2,
    getnameinfo, inet_ntop,
};

#[derive(Debug, PartialEq, Eq)]
struct Flat {
    family: i32,
    socktype: i32,
    protocol: i32,
    port: u16,
    host: String,
}

fn family_rank(family: i32) -> i32 {
    if family == AF_INET { 0 } else { 1 }
}

fn socktype_rank(socktype: i32) -> i32 {
    match socktype {
        SOCK_DGRAM => 0,
        SOCK_STREAM => 1,
        _ => 2,
    }
}

impl PartialOrd for Flat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Flat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        family_rank(self.family)
            .cmp(&family_rank(other.family))
            .then(socktype_rank(self.socktype).cmp(&socktype_rank(other.socktype)))
            .then(self.protocol.cmp(&other.protocol))
            .then(self.port.cmp(&other.port))
            .then(self.host.cmp(&other.host))
    }
}

fn ip_text(ip: IpAddr) -> String {
    let af = match ip {
        IpAddr::V4(_) => AF_INET,
        IpAddr::V6(_) => AF_INET6,
    };
    let mut out = [0u8; 64];
    match inet_ntop(af, ip.as_bytes(), &mut out) {
        Ok(len) => String::from_utf8_lossy(&out[..len]).into_owned(),
        Err(_) => String::new(),
    }
}

fn flat(records: &[AddrInfoRec]) -> Vec<Flat> {
    let mut values: Vec<Flat> = records
        .iter()
        .map(|record| Flat {
            family: record.family,
            socktype: record.socktype,
            protocol: record.protocol,
            port: record.addr.port,
            host: ip_text(record.addr.ip),
        })
        .collect();
    values.sort();
    values
}

fn record(family: i32, socktype: i32, protocol: i32, port: u16, host: &str) -> Flat {
    Flat {
        family,
        socktype,
        protocol,
        port,
        host: String::from(host),
    }
}

fn hints(flags: i32, family: i32, socktype: i32, protocol: i32) -> raven_libc_resolv::Hints {
    raven_libc_resolv::Hints {
        flags,
        family,
        socktype,
        protocol,
    }
}

/// The OpenBSD answer file environment: http is TCP-only, tftp UDP-only, echo both.
fn openbsd_fake() -> FakeDns {
    let fake = FakeDns::new();
    fake.set_hosts(
        "::1 localhost localhost. localhost.localdomain.\n\
         127.0.0.1 localhost localhost. localhost.localdomain.\n",
    );
    fake.set_services("http 80/tcp www\ntftp 69/udp\necho 7/tcp\necho 7/udp\n");
    fake.set_iface(1, "lo0");
    fake
}

/// The NetBSD v4 answer file environment: http and tftp both transport protocols.
fn netbsd_fake() -> FakeDns {
    let fake = FakeDns::new();
    fake.set_hosts("127.0.0.1 localhost localhost. localhost.localdomain.\n");
    fake.set_services(
        "http 80/tcp\nhttp 80/udp\ntftp 69/tcp\ntftp 69/udp\n\
         echo 7/tcp\necho 7/udp\n",
    );
    fake
}

fn gai(
    fake: &FakeDns,
    host: Option<&str>,
    serv: Option<&str>,
    hints: &raven_libc_resolv::Hints,
) -> Result<Vec<AddrInfoRec>, Eai> {
    let env = fake.env();
    getaddrinfo(host, serv, hints, &env, &HostOrder::defaults())
}

#[test]
fn openbsd_basic_ipv6_http() {
    // "host ::1 serv http / ai1 family 24 socktype 1 protocol 6 serv 80"
    let fake = openbsd_fake();
    let records = gai(&fake, Some("::1"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET6, SOCK_STREAM, 6, 80, "::1")]
    );
    assert_eq!(records[0].flags, 2);
}

#[test]
fn openbsd_basic_ipv4_http() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("127.0.0.1"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1")]
    );
}

#[test]
fn openbsd_localhost_http_has_both_families() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("localhost"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1"),
            record(AF_INET6, SOCK_STREAM, 6, 80, "::1"),
        ]
    );
}

#[test]
fn openbsd_ipv6_echo_has_both_socktypes() {
    // The donor prints UDP first; the expected list is a set.
    let fake = openbsd_fake();
    let records = gai(&fake, Some("::1"), Some("echo"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET6, SOCK_DGRAM, 17, 7, "::1"),
            record(AF_INET6, SOCK_STREAM, 6, 7, "::1"),
        ]
    );
}

#[test]
fn openbsd_ipv6_tftp_is_udp_only() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("::1"), Some("tftp"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET6, SOCK_DGRAM, 17, 69, "::1")]
    );
}

#[test]
fn openbsd_specific_family_v4() {
    let fake = openbsd_fake();
    let records = gai(
        &fake,
        Some("localhost"),
        Some("http"),
        &hints(2, AF_INET, 0, 0),
    )
    .expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1")]
    );
}

#[test]
fn openbsd_specific_family_v6() {
    let fake = openbsd_fake();
    let records = gai(
        &fake,
        Some("localhost"),
        Some("http"),
        &hints(2, AF_INET6, 0, 0),
    )
    .expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET6, SOCK_STREAM, 6, 80, "::1")]
    );
}

#[test]
fn openbsd_empty_hostname_http() {
    let fake = openbsd_fake();
    let records = gai(&fake, None, Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1"),
            record(AF_INET6, SOCK_STREAM, 6, 80, "::1"),
        ]
    );
}

#[test]
fn openbsd_empty_hostname_passive() {
    let fake = openbsd_fake();
    let records = gai(&fake, None, Some("http"), &hints(2 | AI_PASSIVE, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_STREAM, 6, 80, "0.0.0.0"),
            record(AF_INET6, SOCK_STREAM, 6, 80, "::"),
        ]
    );
}

#[test]
fn openbsd_empty_hostname_with_a_socktype() {
    let fake = openbsd_fake();
    let records = gai(&fake, None, Some("80"), &hints(2, 0, SOCK_STREAM, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1"),
            record(AF_INET6, SOCK_STREAM, 6, 80, "::1"),
        ]
    );
}

#[test]
fn openbsd_empty_servname_gives_port_zero() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("127.0.0.1"), None, &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_DGRAM, 17, 0, "127.0.0.1"),
            record(AF_INET, SOCK_STREAM, 6, 0, "127.0.0.1"),
        ]
    );
}

#[test]
fn openbsd_empty_host_and_service_is_noname() {
    let fake = openbsd_fake();
    assert_eq!(gai(&fake, None, None, &hints(2, 0, 0, 0)), Err(Eai::NoName));
}

#[test]
fn openbsd_sock_raw_without_a_service() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("localhost"), None, &hints(2, 0, SOCK_RAW, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_RAW, 0, 0, "127.0.0.1"),
            record(AF_INET6, SOCK_RAW, 0, 0, "::1"),
        ]
    );
}

#[test]
fn openbsd_sock_raw_with_a_protocol() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("localhost"), None, &hints(2, 0, SOCK_RAW, 59)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_RAW, 59, 0, "127.0.0.1"),
            record(AF_INET6, SOCK_RAW, 59, 0, "::1"),
        ]
    );
}

#[test]
fn openbsd_sock_raw_with_a_service_is_service() {
    let fake = openbsd_fake();
    assert_eq!(
        gai(
            &fake,
            Some("localhost"),
            Some("80"),
            &hints(2, 0, SOCK_RAW, 59)
        ),
        Err(Eai::Service)
    );
    assert_eq!(
        gai(
            &fake,
            Some("localhost"),
            Some("www"),
            &hints(2, 0, SOCK_RAW, 59)
        ),
        Err(Eai::Service)
    );
}

#[test]
fn openbsd_unsupported_family() {
    let fake = openbsd_fake();
    assert_eq!(
        gai(&fake, Some("localhost"), None, &hints(2, 99, 0, 0)),
        Err(Eai::Family)
    );
}

#[test]
fn openbsd_scoped_literal_and_numeric_formatting() {
    let fake = openbsd_fake();
    let records = gai(&fake, Some("fe80::1%lo0"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET6, SOCK_STREAM, 6, 80, "fe80::1")]
    );
    assert_eq!(records[0].addr.scope, 1);
    let env = fake.env();
    let (host, service) = getnameinfo(
        &records[0].addr,
        255,
        32,
        NI_NUMERICHOST | NI_NUMERICSERV,
        &env,
        &HostOrder::defaults(),
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fe80::1%lo0"));
    assert_eq!(service.as_deref(), Some("80"));
}

#[test]
fn netbsd_basics_ipv6_http() {
    let fake = netbsd_fake();
    let records = gai(&fake, Some("::1"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET6, SOCK_DGRAM, 17, 80, "::1"),
            record(AF_INET6, SOCK_STREAM, 6, 80, "::1"),
        ]
    );
}

#[test]
fn netbsd_basics_localhost_v4() {
    let fake = netbsd_fake();
    let records = gai(&fake, Some("localhost"), Some("http"), &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET, SOCK_DGRAM, 17, 80, "127.0.0.1"),
            record(AF_INET, SOCK_STREAM, 6, 80, "127.0.0.1"),
        ]
    );
}

#[test]
fn netbsd_no_servname_v6() {
    let fake = netbsd_fake();
    let records = gai(&fake, Some("::1"), None, &hints(2, 0, 0, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![
            record(AF_INET6, SOCK_DGRAM, 17, 0, "::1"),
            record(AF_INET6, SOCK_STREAM, 6, 0, "::1"),
        ]
    );
}

#[test]
fn netbsd_no_servname_both_empty_is_noname() {
    let fake = netbsd_fake();
    assert_eq!(gai(&fake, None, None, &hints(2, 0, 0, 0)), Err(Eai::NoName));
}

#[test]
fn netbsd_unsupported_family() {
    let fake = netbsd_fake();
    assert_eq!(
        gai(&fake, Some("localhost"), None, &hints(2, 99, 0, 0)),
        Err(Eai::Family)
    );
}

#[test]
fn netbsd_sock_raw_v4_without_a_service() {
    let fake = netbsd_fake();
    let records = gai(&fake, Some("localhost"), None, &hints(2, 0, SOCK_RAW, 0)).expect("gai");
    assert_eq!(
        flat(&records),
        vec![record(AF_INET, SOCK_RAW, 0, 0, "127.0.0.1")]
    );
}

#[test]
fn netbsd_sock_raw_with_a_service_is_service() {
    let fake = netbsd_fake();
    assert_eq!(
        gai(
            &fake,
            Some("localhost"),
            Some("80"),
            &hints(2, 0, SOCK_RAW, 59)
        ),
        Err(Eai::Service)
    );
    assert_eq!(
        gai(
            &fake,
            Some("localhost"),
            Some("www"),
            &hints(2, 0, SOCK_RAW, 59)
        ),
        Err(Eai::Service)
    );
}

fn hostent(fake: &FakeDns, name: &str, family: i32) -> raven_libc_resolv::HostEnt {
    let env = fake.env();
    gethostbyname2(name, family, &env, &HostOrder::defaults()).expect("hostent")
}

#[test]
fn netbsd_h_hostent_ipv4_dns_name() {
    // "name=sixthavenue.astron.com, length=4, addrtype=2, aliases=[] addr_list=[38.117.134.16]"
    let fake = FakeDns::new();
    fake.set_hosts("");
    fake.add_rule(
        "sixthavenue.astron.com",
        1,
        Action::Records(vec![Record::A([38, 117, 134, 16])]),
    );
    let entry = hostent(&fake, "sixthavenue.astron.com", AF_INET);
    assert_eq!(entry.name, "sixthavenue.astron.com");
    assert_eq!(entry.addr_type, AF_INET);
    assert!(entry.aliases.is_empty());
    assert_eq!(entry.addrs, vec![vec![38, 117, 134, 16]]);
}

#[test]
fn netbsd_h_hostent_ipv6_dns_name() {
    // "name=sixthavenue.astron.com, length=16, addrtype=24, aliases=[]"
    let fake = FakeDns::new();
    fake.set_hosts("");
    let mut bytes = [0u8; 16];
    bytes[0] = 0x26;
    bytes[1] = 0x20;
    bytes[2] = 0x01;
    bytes[3] = 0x06;
    bytes[4] = 0x30;
    bytes[5] = 0x03;
    bytes[6] = 0x1f;
    bytes[7] = 0x00;
    bytes[8] = 0x3e;
    bytes[9] = 0x4a;
    bytes[10] = 0x92;
    bytes[11] = 0xff;
    bytes[12] = 0xfe;
    bytes[13] = 0xf4;
    bytes[14] = 0xe1;
    bytes[15] = 0x80;
    fake.add_rule(
        "sixthavenue.astron.com",
        28,
        Action::Records(vec![Record::Aaaa(bytes)]),
    );
    let entry = hostent(&fake, "sixthavenue.astron.com", AF_INET6);
    assert_eq!(entry.name, "sixthavenue.astron.com");
    assert_eq!(entry.addr_type, AF_INET6);
    assert!(entry.aliases.is_empty());
    assert_eq!(entry.addrs.len(), 1);
    assert_eq!(entry.addrs[0].len(), 16);
}

#[test]
fn netbsd_h_hostent_localhost_name_and_address() {
    // The NetBSD alias list is not reproduced by the musl-shape hostent; see the module note.
    let fake = netbsd_fake();
    let entry = hostent(&fake, "localhost", AF_INET);
    assert_eq!(entry.name, "localhost");
    assert_eq!(entry.addr_type, AF_INET);
    assert_eq!(entry.addrs, vec![vec![127, 0, 0, 1]]);
}

#[test]
fn netbsd_hosts_fixture_has_the_three_names() {
    let fake = netbsd_fake();
    let entry = hostent(&fake, "localhost.localdomain.", AF_INET);
    assert_eq!(entry.name, "localhost");
    assert_eq!(entry.addrs, vec![vec![127, 0, 0, 1]]);
}

#[test]
fn musl_proto_table_tcp_and_udp() {
    use raven_libc_resolv::{Proto, getprotobyname};
    let fake = FakeDns::new();
    fake.fail_file("/etc/protocols", raven_libc_abi::errno::ENOENT);
    let tcp = getprotobyname(&fake, "tcp").expect("tcp").expect("entry");
    assert_eq!(tcp.proto, 6);
    assert!(tcp.aliases.is_empty());
    let udp = getprotobyname(&fake, "udp").expect("udp").expect("entry");
    assert_eq!(udp.proto, 17);
    assert_eq!(Proto::from_number(6), Some(Proto::Tcp));
}

#[test]
fn the_fake_defaults_to_the_documented_server() {
    assert_eq!(FakeDns::server().port, 53);
    assert_eq!(FakeDns::server().ip, IpAddr::V4(fake_dns::SERVER));
}

#[test]
fn an_unspecified_family_never_reaches_the_fixture() {
    assert_eq!(AF_UNSPEC, 0);
    assert_eq!(
        SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 0).family().af(),
        AF_INET
    );
}
