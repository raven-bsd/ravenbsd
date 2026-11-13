// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end resolver scenarios over the scripted fake server (ticket 7.2).

#[path = "common/fake_dns.rs"]
mod fake_dns;

use fake_dns::{Action, FakeDns, Record, SERVER};
use raven_libc_nss::Source;
use raven_libc_resolv::{
    AF_INET, AF_INET6, AF_UNSPEC, AI_ADDRCONFIG, AI_ALL, AI_NUMERICSERV, AI_PASSIVE, AI_V4MAPPED,
    AddrInfoRec, Eai, Hints, HostOrder, IpAddr, NI_NAMEREQD, NI_NOFQDN, NI_NUMERICHOST,
    NI_NUMERICSERV, OrderEntry, SOCK_STREAM, SockAddr, getaddrinfo, getnameinfo,
};

fn hints(flags: i32, family: i32, socktype: i32, protocol: i32) -> Hints {
    Hints {
        flags,
        family,
        socktype,
        protocol,
    }
}

fn default_hints() -> Hints {
    Hints::default()
}

fn aaaa(last: u8) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0] = 0x20;
    bytes[1] = 0x01;
    bytes[2] = 0x0d;
    bytes[3] = 0xb8;
    bytes[15] = last;
    bytes
}

fn a_record(last: u8) -> Record {
    Record::A([192, 0, 2, last])
}

fn v6_record(last: u8) -> Record {
    Record::Aaaa(aaaa(last))
}

fn run(
    fake: &FakeDns,
    host: Option<&str>,
    serv: Option<&str>,
    hints: &Hints,
    order: &HostOrder,
) -> Result<Vec<AddrInfoRec>, Eai> {
    let env = fake.env();
    getaddrinfo(host, serv, hints, &env, order)
}

fn run_default(
    fake: &FakeDns,
    host: Option<&str>,
    serv: Option<&str>,
    hints: &Hints,
) -> Result<Vec<AddrInfoRec>, Eai> {
    run(fake, host, serv, hints, &HostOrder::defaults())
}

fn nameinfo(
    fake: &FakeDns,
    addr: &SockAddr,
    hostlen: usize,
    servlen: usize,
    flags: i32,
) -> Result<(Option<String>, Option<String>), Eai> {
    let env = fake.env();
    getnameinfo(addr, hostlen, servlen, flags, &env, &HostOrder::defaults())
}

fn files_only() -> HostOrder {
    HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }])
}

#[test]
fn a_only() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("a.example", 1, Action::Records(vec![a_record(7)]));
    let records = run_default(&fake, Some("a.example"), None, &default_hints()).expect("gai");
    assert!(!records.is_empty());
    assert!(
        records
            .iter()
            .all(|record| record.addr.ip == IpAddr::V4([192, 0, 2, 7]))
    );
    assert_eq!(records[0].canonname.as_deref(), Some("a.example"));
    assert_eq!(fake.udp_closes(), fake.udp_opens());
}

#[test]
fn aaaa_only() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("aaaa.example", 28, Action::Records(vec![v6_record(7)]));
    let records = run_default(&fake, Some("aaaa.example"), None, &default_hints()).expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET6));
    assert!(
        records
            .iter()
            .all(|record| record.addr.ip == IpAddr::V6(aaaa(7)))
    );
}

#[test]
fn a_and_aaaa_are_sorted() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("both.example", 1, Action::Records(vec![a_record(7)]));
    fake.add_rule("both.example", 28, Action::Records(vec![v6_record(7)]));
    fake.set_route(IpAddr::V4([192, 0, 2, 7]), Some(IpAddr::V4([192, 0, 2, 1])));
    fake.set_route(IpAddr::V6(aaaa(7)), Some(IpAddr::V6(aaaa(1))));
    let records = run_default(&fake, Some("both.example"), None, &default_hints()).expect("gai");
    assert!(records.iter().any(|record| record.family == AF_INET));
    assert!(records.iter().any(|record| record.family == AF_INET6));
    assert_eq!(records[0].family, AF_INET6);
}

#[test]
fn an_unroutable_aaaa_sorts_last() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("mixed.example", 1, Action::Records(vec![a_record(7)]));
    fake.add_rule("mixed.example", 28, Action::Records(vec![v6_record(7)]));
    fake.set_route(IpAddr::V4([192, 0, 2, 7]), Some(IpAddr::V4([192, 0, 2, 1])));
    fake.set_route(IpAddr::V6(aaaa(7)), None);
    let records = run_default(&fake, Some("mixed.example"), None, &default_hints()).expect("gai");
    assert_eq!(records[0].family, AF_INET);
    assert_eq!(records[records.len() - 1].family, AF_INET6);
}

#[test]
fn a_one_hop_cname_chain_sets_the_canonical_name() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule(
        "alias.example",
        1,
        Action::Records(vec![Record::Cname(String::from("c1.example")), a_record(7)]),
    );
    let records = run_default(&fake, Some("alias.example"), None, &default_hints()).expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("c1.example"));
}

#[test]
fn a_three_hop_cname_chain_sets_the_last_target() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule(
        "alias3.example",
        1,
        Action::Records(vec![
            Record::Cname(String::from("c1.example")),
            Record::Cname(String::from("c2.example")),
            Record::Cname(String::from("c3.example")),
            a_record(7),
        ]),
    );
    let records = run_default(&fake, Some("alias3.example"), None, &default_hints()).expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("c3.example"));
}

#[test]
fn an_eight_hop_cname_chain_sets_the_last_target() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    let mut answers = Vec::new();
    for hop in 1..=8u8 {
        answers.push(Record::Cname(format!("c{hop}.example")));
    }
    answers.push(a_record(7));
    fake.add_rule("alias8.example", 1, Action::Records(answers));
    let records = run_default(&fake, Some("alias8.example"), None, &default_hints()).expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("c8.example"));
}

#[test]
fn an_invalid_cname_target_is_rejected() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule(
        "badtarget.example",
        1,
        Action::Records(vec![Record::Cname(String::from("bad name")), a_record(7)]),
    );
    let records =
        run_default(&fake, Some("badtarget.example"), None, &default_hints()).expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("badtarget.example"));
}

#[test]
fn nxdomain_on_the_first_search_domain_then_success_on_the_second() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\nsearch first.example second.example\n");
    fake.set_default(Action::NxDomain);
    fake.add_rule("host.first.example", 1, Action::NxDomain);
    fake.add_rule("host.second.example", 1, Action::Records(vec![a_record(7)]));
    let records = run_default(&fake, Some("host"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
    assert_eq!(records[0].canonname.as_deref(), Some("host.second.example"));
    assert_eq!(
        fake.sent_names(),
        vec![
            String::from("host.first.example"),
            String::from("host.second.example")
        ]
    );
}

#[test]
fn nxdomain_everywhere_is_noname() {
    let fake = FakeDns::new();
    fake.set_default(Action::NxDomain);
    assert_eq!(
        run_default(
            &fake,
            Some("missing.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::NoName)
    );
}

#[test]
fn nodata_for_one_family_with_the_other_answering() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("v6only.example", 28, Action::Records(vec![v6_record(7)]));
    let records = run_default(&fake, Some("v6only.example"), None, &default_hints()).expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET6));
}

#[test]
fn nodata_everywhere_is_nodata() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    assert_eq!(
        run_default(&fake, Some("empty.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::NoData)
    );
}

#[test]
fn nodata_on_the_first_search_domain_stops_like_the_donor() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\nsearch first.example second.example\n");
    fake.set_default(Action::NoData);
    fake.add_rule("host.second.example", 1, Action::Records(vec![a_record(7)]));
    assert_eq!(
        run_default(&fake, Some("host"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::NoData)
    );
    assert_eq!(fake.sent_names(), vec![String::from("host.first.example")]);
}

#[test]
fn servfail_on_one_server_and_success_on_another() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\nnameserver 192.0.2.54\n");
    fake.set_default(Action::NoData);
    fake.add_server_rule(
        SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53),
        "multi.example",
        1,
        Action::ServFail,
    );
    fake.add_server_rule(
        SockAddr::new(IpAddr::V4([192, 0, 2, 54]), 53),
        "multi.example",
        1,
        Action::Records(vec![a_record(7)]),
    );
    let records =
        run_default(&fake, Some("multi.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
}

#[test]
fn servfail_everywhere_is_again() {
    let fake = FakeDns::new();
    fake.set_default(Action::ServFail);
    assert_eq!(
        run_default(&fake, Some("fail.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::Again)
    );
}

#[test]
fn refused_is_fail() {
    let fake = FakeDns::new();
    fake.set_default(Action::Refused);
    assert_eq!(
        run_default(
            &fake,
            Some("refused.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::Fail)
    );
}

#[test]
fn truncated_udp_then_tcp_success_with_one_byte_chunks() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.set_tcp_chunk(1);
    fake.add_rule("tcp.example", 1, Action::Truncated(vec![a_record(7)]));
    let records =
        run_default(&fake, Some("tcp.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
    assert_eq!(fake.tcp_opens(), 1);
    assert_eq!(fake.tcp_closes(), 1);
}

#[test]
fn tcp_refused_leaves_the_query_unanswered_and_is_again() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.set_tcp_refused(true);
    fake.add_rule("tcp.example", 1, Action::Truncated(vec![a_record(7)]));
    assert_eq!(
        run_default(&fake, Some("tcp.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::Again)
    );
    assert_eq!(fake.tcp_opens(), 0);
}

#[test]
fn a_tcp_length_field_below_13_aborts() {
    for length in [0u16, 12] {
        let fake = FakeDns::new();
        fake.set_default(Action::NoData);
        fake.set_tcp_chunk(1);
        fake.add_rule("short.example", 1, Action::TcpLength(length));
        assert_eq!(
            run_default(&fake, Some("short.example"), None, &hints(0, AF_INET, 0, 0)),
            Err(Eai::Again)
        );
        assert!(fake.tcp_opens() >= 1);
        assert_eq!(fake.tcp_opens(), fake.tcp_closes());
    }
}

#[test]
fn a_tcp_length_of_13_passes_the_gate_but_fails_the_question_check() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.set_tcp_chunk(1);
    fake.add_rule("short.example", 1, Action::TcpLength(13));
    assert_eq!(
        run_default(&fake, Some("short.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::Again)
    );
    assert!(fake.tcp_opens() >= 1);
    assert_eq!(fake.tcp_opens(), fake.tcp_closes());
}

#[test]
fn a_total_timeout_finishes_at_exactly_the_configured_time() {
    let fake = FakeDns::new();
    fake.set_default(Action::Drop);
    let started = fake.now();
    assert_eq!(
        run_default(&fake, Some("slow.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::Again)
    );
    assert_eq!(fake.now() - started, 5000);
}

#[test]
fn four_attempts_send_at_zero_500_1000_1500() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\noptions timeout:2 attempts:4\n");
    fake.set_default(Action::Drop);
    assert_eq!(
        run_default(&fake, Some("slow.example"), None, &hints(0, AF_INET, 0, 0)),
        Err(Eai::Again)
    );
    assert_eq!(fake.send_times(), vec![0, 500, 1000, 1500]);
}

#[test]
fn a_reply_from_an_unconfigured_source_is_ignored() {
    let fake = FakeDns::new();
    fake.set_default(Action::Drop);
    fake.add_rule(
        "wrongsource.example",
        1,
        Action::WrongSource(vec![a_record(7)]),
    );
    assert_eq!(
        run_default(
            &fake,
            Some("wrongsource.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::Again)
    );
}

#[test]
fn a_reply_with_the_wrong_id_is_ignored() {
    let fake = FakeDns::new();
    fake.set_default(Action::Drop);
    fake.add_rule("wrongid.example", 1, Action::WrongId(vec![a_record(7)]));
    assert_eq!(
        run_default(
            &fake,
            Some("wrongid.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::Again)
    );
}

#[test]
fn a_reply_with_the_wrong_question_is_ignored() {
    let fake = FakeDns::new();
    fake.set_default(Action::Drop);
    fake.add_rule(
        "wrongq.example",
        1,
        Action::WrongQuestion(vec![a_record(7)]),
    );
    assert_eq!(
        run_default(
            &fake,
            Some("wrongq.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::Again)
    );
}

#[test]
fn a_duplicate_answer_is_harmless() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("dup.example", 1, Action::Duplicate(vec![a_record(7)]));
    let records =
        run_default(&fake, Some("dup.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
}

#[test]
fn a_delayed_answer_within_the_timeout_is_accepted() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("delay.example", 1, Action::Delay(1200, vec![a_record(7)]));
    let records =
        run_default(&fake, Some("delay.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
}

#[test]
fn the_a_and_aaaa_ids_differ_between_lookups() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("ids.example", 1, Action::Records(vec![a_record(7)]));
    fake.add_rule("ids.example", 28, Action::Records(vec![v6_record(7)]));
    run_default(&fake, Some("ids.example"), None, &default_hints()).expect("first");
    let first: Vec<u16> = fake.send_ids();
    assert_eq!(first.len(), 2);
    assert_ne!(first[0], first[1]);
    fake.reset_counters();
    run_default(&fake, Some("ids.example"), None, &default_hints()).expect("second");
    let second: Vec<u16> = fake.send_ids();
    assert_eq!(second.len(), 2);
    assert_ne!(second[0], second[1]);
    assert!(first.iter().all(|id| !second.contains(id)));
}

#[test]
fn an_entropy_failure_is_fail_and_sends_nothing() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("entropy.example", 1, Action::Records(vec![a_record(7)]));
    fake.set_entropy_failing(true);
    assert_eq!(
        run_default(
            &fake,
            Some("entropy.example"),
            None,
            &hints(0, AF_INET, 0, 0)
        ),
        Err(Eai::Fail)
    );
    assert_eq!(fake.sends(), 0);
    assert_eq!(fake.udp_opens(), 0);
}

#[test]
fn every_lookup_opens_and_closes_its_own_socket() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("one.example", 1, Action::Records(vec![a_record(7)]));
    for _ in 0..3 {
        run_default(&fake, Some("one.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    }
    assert_eq!(fake.udp_opens(), 3);
    assert_eq!(fake.udp_closes(), 3);
}

#[test]
fn one_hundred_lookups_use_distinct_source_ports() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("port.example", 1, Action::Records(vec![a_record(7)]));
    for _ in 0..100 {
        run_default(&fake, Some("port.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    }
    let ports = fake.open_ports();
    assert_eq!(ports.len(), 100);
    let mut unique = ports.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 100);
    assert!(ports.iter().all(|port| *port >= 49152));
}

#[test]
fn a_numeric_host_never_touches_the_socket_layer() {
    let fake = FakeDns::new();
    let records = run_default(&fake, Some("127.0.0.1"), Some("80"), &default_hints()).expect("gai");
    assert!(!records.is_empty());
    assert_eq!(fake.udp_opens(), 0);
}

#[test]
fn a_hosts_hit_never_touches_the_socket_layer() {
    let fake = FakeDns::new();
    fake.set_hosts("192.0.2.9 hosts.example\n");
    let records = run_default(
        &fake,
        Some("hosts.example"),
        Some("80"),
        &hints(0, AF_INET, 0, 0),
    )
    .expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 9]));
    assert_eq!(fake.udp_opens(), 0);
}

#[test]
fn files_notfound_return_stops_before_dns() {
    let fake = FakeDns::new();
    fake.set_hosts("");
    fake.add_rule("stop.example", 1, Action::Records(vec![a_record(7)]));
    let order = HostOrder::from_sources(&[
        OrderEntry {
            source: Source::Files,
            on_notfound_return: true,
        },
        OrderEntry {
            source: Source::Dns,
            on_notfound_return: false,
        },
    ]);
    assert_eq!(
        run(
            &fake,
            Some("stop.example"),
            None,
            &hints(0, AF_INET, 0, 0),
            &order
        ),
        Err(Eai::NoName)
    );
    assert_eq!(fake.sends(), 0);
}

#[test]
fn dns_first_order_queries_dns_before_files() {
    let fake = FakeDns::new();
    fake.set_hosts("192.0.2.9 order.example\n");
    fake.add_rule("order.example", 1, Action::Records(vec![a_record(7)]));
    let order = HostOrder::from_sources(&[
        OrderEntry {
            source: Source::Dns,
            on_notfound_return: false,
        },
        OrderEntry {
            source: Source::Files,
            on_notfound_return: false,
        },
    ]);
    let records = run(
        &fake,
        Some("order.example"),
        None,
        &hints(0, AF_INET, 0, 0),
        &order,
    )
    .expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
    assert_eq!(fake.sends(), 1);
}

#[test]
fn localhost_resolves_without_dns() {
    let fake = FakeDns::new();
    for name in ["localhost", "LOCALHOST.", "foo.localhost"] {
        let records =
            run_default(&fake, Some(name), None, &hints(0, AF_INET, 0, 0)).expect("localhost");
        assert_eq!(records[0].addr.ip, IpAddr::V4([127, 0, 0, 1]));
    }
    assert_eq!(fake.sends(), 0);
}

#[test]
fn addrconfig_without_v6_drops_the_v6_family() {
    let fake = FakeDns::new();
    fake.set_v6_usable(false);
    let records = run_default(
        &fake,
        Some("127.0.0.1"),
        Some("80"),
        &hints(AI_ADDRCONFIG, AF_UNSPEC, 0, 0),
    )
    .expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET));
}

#[test]
fn v4mapped_and_all_keep_both_families() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("map.example", 1, Action::Records(vec![a_record(7)]));
    fake.add_rule("map.example", 28, Action::Records(vec![v6_record(7)]));
    let records = run_default(
        &fake,
        Some("map.example"),
        Some("80"),
        &hints(AI_V4MAPPED | AI_ALL, AF_INET6, SOCK_STREAM, 0),
    )
    .expect("gai");
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .any(|record| record.addr.ip.as_v4_mapped() == Some([192, 0, 2, 7]))
    );
    assert!(
        records
            .iter()
            .any(|record| record.addr.ip == IpAddr::V6(aaaa(7)))
    );
}

#[test]
fn passive_without_a_host_uses_unspecified_addresses() {
    let fake = FakeDns::new();
    let records =
        run_default(&fake, None, Some("80"), &hints(AI_PASSIVE, AF_UNSPEC, 0, 0)).expect("gai");
    assert!(records.iter().all(|record| record.addr.ip.is_unspecified()));
}

#[test]
fn bad_flags_family_and_socktype_are_reported() {
    let fake = FakeDns::new();
    assert_eq!(
        run_default(
            &fake,
            Some("127.0.0.1"),
            Some("80"),
            &hints(0x8000, 0, 0, 0)
        ),
        Err(Eai::BadFlags)
    );
    assert_eq!(
        run_default(&fake, Some("127.0.0.1"), Some("80"), &hints(0, 99, 0, 0)),
        Err(Eai::Family)
    );
    assert_eq!(
        run_default(&fake, Some("127.0.0.1"), Some("http"), &hints(0, 0, 99, 0)),
        Err(Eai::Service)
    );
}

#[test]
fn services_numeric_name_unknown_and_protocol_mismatch() {
    let fake = FakeDns::new();
    let numeric =
        run_default(&fake, Some("127.0.0.1"), Some("80"), &default_hints()).expect("numeric");
    assert_eq!(numeric.len(), 2);
    let named =
        run_default(&fake, Some("127.0.0.1"), Some("http"), &default_hints()).expect("named");
    assert_eq!(named[0].addr.port, 80);
    assert_eq!(
        run_default(&fake, Some("127.0.0.1"), Some("nope"), &default_hints()),
        Err(Eai::Service)
    );
    assert_eq!(
        run_default(
            &fake,
            Some("127.0.0.1"),
            Some("http"),
            &hints(0, 0, SOCK_STREAM, 17)
        ),
        Err(Eai::Service)
    );
    assert_eq!(
        run_default(
            &fake,
            Some("127.0.0.1"),
            Some("http"),
            &hints(AI_NUMERICSERV, 0, 0, 0)
        ),
        Err(Eai::NoName)
    );
}

#[test]
fn at_most_48_addresses_are_returned() {
    let fake = FakeDns::new();
    let mut hosts = String::new();
    for index in 1..=60u8 {
        hosts.push_str(&format!("10.0.0.{index} many.example\n"));
    }
    fake.set_hosts(&hosts);
    let records = run_default(
        &fake,
        Some("many.example"),
        Some("80"),
        &hints(0, AF_INET, SOCK_STREAM, 0),
    )
    .expect("gai");
    assert_eq!(records.len(), 48);
}

#[test]
fn getnameinfo_uses_hosts_before_dns() {
    let fake = FakeDns::new();
    fake.set_hosts("192.0.2.1 host.example\n");
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("host.example"));
    assert_eq!(fake.sends(), 0);
}

#[test]
fn getnameinfo_can_use_a_ptr_query() {
    let fake = FakeDns::new();
    fake.set_hosts("");
    fake.add_rule(
        "1.2.0.192.in-addr.arpa",
        12,
        Action::Records(vec![Record::Ptr(String::from("ptr.example"))]),
    );
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("ptr.example"));
    assert_eq!(fake.sends(), 1);
}

#[test]
fn getnameinfo_rejects_an_invalid_ptr_target() {
    let fake = FakeDns::new();
    fake.set_hosts("");
    fake.add_rule(
        "1.2.0.192.in-addr.arpa",
        12,
        Action::Records(vec![Record::Ptr(String::from("bad name"))]),
    );
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("192.0.2.1"));
}

#[test]
fn getnameinfo_numerichost_skips_every_lookup() {
    let fake = FakeDns::new();
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NUMERICHOST,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("192.0.2.1"));
    assert_eq!(fake.sends(), 0);
}

#[test]
fn getnameinfo_namereqd_fails_without_a_name() {
    let fake = FakeDns::new();
    fake.set_default(Action::Drop);
    let error = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NAMEREQD,
    );
    assert_eq!(error, Err(Eai::NoName));
}

#[test]
fn getnameinfo_formats_a_link_local_scope() {
    let fake = FakeDns::new();
    fake.set_iface(7, "eth0");
    let mut bytes = [0u8; 16];
    bytes[0] = 0xfe;
    bytes[1] = 0x80;
    bytes[15] = 1;
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::with_scope(IpAddr::V6(bytes), 0, 7),
        255,
        0,
        NI_NUMERICHOST,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fe80::1%eth0"));
}

#[test]
fn getnameinfo_reverses_a_service() {
    let fake = FakeDns::new();
    let (_, service) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 80),
        0,
        32,
        0,
    )
    .expect("nameinfo");
    assert_eq!(service.as_deref(), Some("http"));
}

#[test]
fn getnameinfo_buffer_sizes() {
    let fake = FakeDns::new();
    fake.set_hosts("192.0.2.1 host.example\n");
    let addr = SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 80);
    let (host, _) = nameinfo(&fake, &addr, 13, 0, 0).expect("exact");
    assert_eq!(host.as_deref(), Some("host.example"));
    assert_eq!(nameinfo(&fake, &addr, 12, 0, 0), Err(Eai::Overflow));
    assert_eq!(nameinfo(&fake, &addr, 1, 0, 0), Err(Eai::Overflow));
    assert_eq!(
        nameinfo(&fake, &addr, 0, 2, NI_NUMERICSERV),
        Err(Eai::Overflow)
    );
    let (_, service) = nameinfo(&fake, &addr, 0, 3, NI_NUMERICSERV).expect("exact serv");
    assert_eq!(service.as_deref(), Some("80"));
}

#[test]
fn getnameinfo_nofqdn_strips_the_domain() {
    let fake = FakeDns::new();
    fake.set_hosts("192.0.2.1 host.example\n");
    let (host, _) = nameinfo(
        &fake,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NOFQDN,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("host"));
}

#[test]
fn the_legacy_host_order_helper_is_not_needed_for_numeric_lookups() {
    let fake = FakeDns::new();
    let order = files_only();
    let records = run(
        &fake,
        Some("127.0.0.1"),
        Some("80"),
        &default_hints(),
        &order,
    )
    .expect("gai");
    assert!(!records.is_empty());
}

#[test]
fn a_host_with_a_dotted_name_sends_one_bare_query() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("bare.example", 1, Action::Records(vec![a_record(7)]));
    run_default(&fake, Some("bare.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(fake.sent_names(), vec![String::from("bare.example")]);
}

#[test]
fn a_trailing_dot_suppresses_the_search_list() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\nsearch search.example\n");
    fake.set_default(Action::NoData);
    fake.add_rule("bare.example", 1, Action::Records(vec![a_record(7)]));
    run_default(&fake, Some("bare.example."), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(fake.sent_names(), vec![String::from("bare.example")]);
}

#[test]
fn ndots_suppresses_the_search_list() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\nsearch search.example\noptions ndots:1\n");
    fake.set_default(Action::NoData);
    fake.add_rule("two.example", 1, Action::Records(vec![a_record(7)]));
    run_default(&fake, Some("two.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(fake.sent_names(), vec![String::from("two.example")]);
}

#[test]
fn edns0_is_off_by_default() {
    let fake = FakeDns::new();
    fake.set_default(Action::NoData);
    fake.add_rule("edns.example", 1, Action::Records(vec![a_record(7)]));
    run_default(&fake, Some("edns.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(fake.sent_lengths(), vec![30]);
}

#[test]
fn edns0_option_adds_an_opt_record() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\noptions edns0\n");
    fake.set_default(Action::NoData);
    fake.add_rule("edns.example", 1, Action::Records(vec![a_record(7)]));
    run_default(&fake, Some("edns.example"), None, &hints(0, AF_INET, 0, 0)).expect("gai");
    assert_eq!(fake.sent_lengths(), vec![41]);
}

#[test]
fn no_aaaa_suppresses_the_aaaa_query() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.53\noptions no-aaaa\n");
    fake.set_default(Action::NoData);
    fake.add_rule("v4.example", 1, Action::Records(vec![a_record(7)]));
    fake.add_rule("v4.example", 28, Action::Records(vec![v6_record(7)]));
    let records = run_default(
        &fake,
        Some("v4.example"),
        None,
        &hints(0, AF_UNSPEC, SOCK_STREAM, 0),
    )
    .expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET));
    assert_eq!(fake.sent_types(), vec![1]);
}

#[test]
fn the_server_is_the_configured_nameserver() {
    let fake = FakeDns::new();
    fake.set_resolv("nameserver 192.0.2.54\n");
    fake.set_default(Action::NoData);
    fake.add_rule("server.example", 1, Action::Records(vec![a_record(7)]));
    let records = run_default(
        &fake,
        Some("server.example"),
        None,
        &hints(0, AF_INET, 0, 0),
    )
    .expect("gai");
    assert_eq!(records[0].addr.ip, IpAddr::V4([192, 0, 2, 7]));
    assert_eq!(fake.sends(), 1);
}

#[test]
fn the_default_server_address_is_the_real_one() {
    assert_eq!(FakeDns::server(), SockAddr::new(IpAddr::V4(SERVER), 53));
}
