// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the resolv.conf parser and the ring-2 options.

use super::*;
use crate::types::IpAddr;

#[test]
fn the_defaults_are_the_donor_defaults() {
    let conf = ResolvConf::defaults();
    assert_eq!(conf.nns, 1);
    assert_eq!(conf.ndots, 1);
    assert_eq!(conf.timeout, 5);
    assert_eq!(conf.attempts, 2);
    assert_eq!(conf.search, "");
    assert_eq!(
        conf.first_ns(),
        Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53))
    );
}

#[test]
fn none_gives_the_defaults() {
    assert_eq!(parse_resolv_conf(None), ResolvConf::defaults());
}

#[test]
fn an_empty_file_still_has_loopback() {
    let conf = parse_resolv_conf(Some(b""));
    assert_eq!(conf.nns, 1);
    assert_eq!(
        conf.first_ns(),
        Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53))
    );
}

#[test]
fn an_ipv4_nameserver_is_parsed() {
    let conf = parse_resolv_conf(Some(b"nameserver 192.0.2.53\n"));
    assert_eq!(conf.nns, 1);
    assert_eq!(
        conf.first_ns(),
        Some(SockAddr::new(IpAddr::V4([192, 0, 2, 53]), 53))
    );
}

#[test]
fn an_ipv6_nameserver_is_parsed() {
    let conf = parse_resolv_conf(Some(b"nameserver 2001:db8::53\n"));
    assert_eq!(conf.nns, 1);
    let mut bytes = [0u8; 16];
    bytes[0] = 0x20;
    bytes[1] = 0x01;
    bytes[2] = 0x0d;
    bytes[3] = 0xb8;
    bytes[15] = 0x53;
    assert_eq!(conf.first_ns(), Some(SockAddr::new(IpAddr::V6(bytes), 53)));
}

#[test]
fn a_numeric_ipv6_scope_is_parsed() {
    let conf = parse_resolv_conf(Some(b"nameserver fe80::1%7\n"));
    let server = conf.first_ns().expect("server");
    assert_eq!(server.scope, 7);
}

#[test]
fn an_interface_scope_is_not_resolved_here() {
    let conf = parse_resolv_conf(Some(b"nameserver fe80::1%eth0\n"));
    assert_eq!(conf.nns, 1);
    assert_eq!(
        conf.first_ns(),
        Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53))
    );
}

#[test]
fn an_invalid_nameserver_is_skipped() {
    let conf = parse_resolv_conf(Some(b"nameserver not-an-address\n"));
    assert_eq!(conf.nns, 1);
}

#[test]
fn three_nameservers_are_kept() {
    let conf = parse_resolv_conf(Some(
        b"nameserver 192.0.2.1\nnameserver 192.0.2.2\nnameserver 192.0.2.3\n",
    ));
    assert_eq!(conf.nns, 3);
    assert_eq!(
        conf.ns[1],
        Some(SockAddr::new(IpAddr::V4([192, 0, 2, 2]), 53))
    );
}

#[test]
fn the_fourth_nameserver_is_ignored() {
    let conf = parse_resolv_conf(Some(
        b"nameserver 192.0.2.1\nnameserver 192.0.2.2\nnameserver 192.0.2.3\nnameserver 192.0.2.4\n",
    ));
    assert_eq!(conf.nns, 3);
    assert_eq!(
        conf.ns[2],
        Some(SockAddr::new(IpAddr::V4([192, 0, 2, 3]), 53))
    );
}

#[test]
fn ndots_is_parsed() {
    let conf = parse_resolv_conf(Some(b"options ndots:3\n"));
    assert_eq!(conf.ndots, 3);
}

#[test]
fn ndots_is_capped_at_15() {
    let conf = parse_resolv_conf(Some(b"options ndots:99\n"));
    assert_eq!(conf.ndots, 15);
}

#[test]
fn attempts_is_parsed() {
    let conf = parse_resolv_conf(Some(b"options attempts:4\n"));
    assert_eq!(conf.attempts, 4);
}

#[test]
fn attempts_is_capped_at_10() {
    let conf = parse_resolv_conf(Some(b"options attempts:99\n"));
    assert_eq!(conf.attempts, 10);
}

#[test]
fn timeout_is_parsed() {
    let conf = parse_resolv_conf(Some(b"options timeout:9\n"));
    assert_eq!(conf.timeout, 9);
}

#[test]
fn timeout_is_capped_at_60() {
    let conf = parse_resolv_conf(Some(b"options timeout:999\n"));
    assert_eq!(conf.timeout, 60);
}

#[test]
fn a_fractional_timeout_is_ignored() {
    let conf = parse_resolv_conf(Some(b"options timeout:.5\n"));
    assert_eq!(conf.timeout, 5);
}

#[test]
fn several_options_on_one_line() {
    let conf = parse_resolv_conf(Some(b"options ndots:2 attempts:3 timeout:7\n"));
    assert_eq!((conf.ndots, conf.attempts, conf.timeout), (2, 3, 7));
}

#[test]
fn a_search_list_is_parsed() {
    let conf = parse_resolv_conf(Some(b"search example.com corp.example\n"));
    assert_eq!(conf.search, "example.com corp.example");
}

#[test]
fn the_last_search_line_wins() {
    let conf = parse_resolv_conf(Some(b"search one.example\nsearch two.example\n"));
    assert_eq!(conf.search, "two.example");
}

#[test]
fn domain_is_an_alias_of_search() {
    let conf = parse_resolv_conf(Some(b"domain example.org\n"));
    assert_eq!(conf.search, "example.org");
}

#[test]
fn an_over_long_line_is_skipped_whole() {
    let mut file = alloc::vec![b' '; 300];
    file.extend_from_slice(b"\noptions ndots:3\n");
    let conf = parse_resolv_conf(Some(&file));
    assert_eq!(conf.ndots, 3);
}

#[test]
fn an_over_long_first_line_does_not_leak_its_tail() {
    let mut file = alloc::vec![b'#'; 300];
    file.extend_from_slice(b"ndots:9\n");
    let conf = parse_resolv_conf(Some(&file));
    assert_eq!(conf.ndots, 1);
}

#[test]
fn parse_nameserver_accepts_a_dotted_quad() {
    assert_eq!(
        parse_nameserver(b"127.0.0.1"),
        Some(SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53))
    );
}

#[test]
fn parse_nameserver_rejects_a_bad_literal() {
    assert_eq!(parse_nameserver(b"127.0.0.999"), None);
}

#[test]
fn parse_nameserver_accepts_a_numeric_scope_on_a_global_address() {
    assert_eq!(
        parse_nameserver(b"2001:db8::1%7"),
        Some(SockAddr::with_scope(
            IpAddr::V6({
                let mut bytes = [0u8; 16];
                bytes[0] = 0x20;
                bytes[1] = 0x01;
                bytes[2] = 0x0d;
                bytes[3] = 0xb8;
                bytes[15] = 1;
                bytes
            }),
            53,
            7
        ))
    );
}

#[test]
fn ext_defaults_are_all_off() {
    let ext = ConfExt::default();
    assert_eq!(
        (
            ext.edns0,
            ext.use_vc,
            ext.rotate,
            ext.single_request,
            ext.trust_ad,
            ext.no_aaaa
        ),
        (false, false, false, false, false, false)
    );
}

#[test]
fn ext_accepts_every_known_option() {
    let ext = parse_options_ext(b"options edns0 use-vc rotate single-request trust-ad no-aaaa\n");
    assert!(ext.edns0);
    assert!(ext.use_vc);
    assert!(ext.rotate);
    assert!(ext.single_request);
    assert!(ext.trust_ad);
    assert!(ext.no_aaaa);
}

#[test]
fn ext_accepts_tcp_as_use_vc() {
    assert!(parse_options_ext(b"options tcp\n").use_vc);
}

#[test]
fn ext_ignores_unknown_words() {
    let ext = parse_options_ext(b"options edns0 mystery\n");
    assert!(ext.edns0);
    assert!(!ext.use_vc);
}

#[test]
fn ext_ignores_non_options_lines() {
    let ext = parse_options_ext(b"nameserver 127.0.0.1\nsearch edns0\n");
    assert!(!ext.edns0);
}

#[test]
fn ext_handles_several_lines() {
    let ext = parse_options_ext(b"options edns0\noptions no-aaaa\n");
    assert!(ext.edns0);
    assert!(ext.no_aaaa);
}

#[test]
fn ext_requires_a_space_after_options() {
    assert!(!parse_options_ext(b"optionsedns0\n").edns0);
}

#[test]
fn ext_skips_an_over_long_line() {
    let mut file = alloc::vec![b' '; 300];
    file.extend_from_slice(b"\noptions edns0\n");
    assert!(parse_options_ext(&file).edns0);
}
