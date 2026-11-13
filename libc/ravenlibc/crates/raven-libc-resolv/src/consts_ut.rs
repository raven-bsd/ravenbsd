// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the limits and protocol constants.

use super::*;

macro_rules! value {
    ($name:ident, $left:expr, $right:expr) => {
        #[test]
        fn $name() {
            assert_eq!($left, $right);
        }
    };
}

value!(maxns_is_three, MAXNS, 3);
value!(maxaddrs_is_48, MAXADDRS, 48);
value!(maxservs_is_two, MAXSERVS, 2);
value!(dns_name_is_253, DNS_NAME_MAX, 253);
value!(dns_text_is_254, DNS_NAME_TEXT_MAX, 254);
value!(label_is_63, DNS_LABEL_MAX, 63);
value!(abuf_is_4800, ABUF_SIZE, 4800);
value!(udp_query_is_512, UDP_QUERY_MAX, 512);
value!(tcp_message_is_65535, TCP_MSG_MAX, 65535);
value!(resolv_line_is_256, RESOLV_CONF_LINE_MAX, 256);
value!(hosts_line_is_512, HOSTS_LINE_MAX, 512);
value!(db_line_is_128, DB_LINE_MAX, 128);
value!(passwd_line_is_4096, PASSWD_LINE_MAX, 4096);
value!(file_cap_is_one_mib, FILE_READ_MAX, 1024 * 1024);
value!(ni_maxhost_is_255, NI_MAXHOST, 255);
value!(ni_maxserv_is_32, NI_MAXSERV, 32);
value!(if_namesize_is_16, IF_NAMESIZE, 16);
value!(edns0_udp_size_is_1232, EDNS0_UDP_SIZE, 1232);
value!(edns0_len_is_11, EDNS0_LEN, 11);
value!(dns_port_is_53, DNS_PORT, 53);
value!(af_inet_is_2, AF_INET, 2);
value!(af_inet6_is_10, AF_INET6, 10);
value!(af_unspec_is_0, AF_UNSPEC, 0);

#[test]
fn ai_passive_is_one() {
    assert_eq!(AI_PASSIVE, 1);
}

#[test]
fn ai_canonname_is_two() {
    assert_eq!(AI_CANONNAME, 2);
}

#[test]
fn ai_numerichost_is_four() {
    assert_eq!(AI_NUMERICHOST, 4);
}

#[test]
fn ai_v4mapped_is_eight() {
    assert_eq!(AI_V4MAPPED, 8);
}

#[test]
fn ai_all_is_0x10() {
    assert_eq!(AI_ALL, 0x10);
}

#[test]
fn ai_addrconfig_is_0x20() {
    assert_eq!(AI_ADDRCONFIG, 0x20);
}

#[test]
fn ai_numericserv_is_0x400() {
    assert_eq!(AI_NUMERICSERV, 0x400);
}

#[test]
fn ni_flags_match_musl() {
    assert_eq!(
        (
            NI_NUMERICHOST,
            NI_NUMERICSERV,
            NI_NOFQDN,
            NI_NAMEREQD,
            NI_DGRAM,
            NI_NUMERICSCOPE
        ),
        (1, 2, 4, 8, 16, 0x100)
    );
}

#[test]
fn ai_mask_accepts_every_known_flag() {
    for flag in [
        AI_PASSIVE,
        AI_CANONNAME,
        AI_NUMERICHOST,
        AI_V4MAPPED,
        AI_ALL,
        AI_ADDRCONFIG,
        AI_NUMERICSERV,
    ] {
        assert_eq!(flag & AI_MASK, flag);
    }
    assert_ne!(0x8000 & AI_MASK, 0x8000);
}

#[test]
fn ni_mask_accepts_every_known_flag() {
    for flag in [
        NI_NUMERICHOST,
        NI_NUMERICSERV,
        NI_NOFQDN,
        NI_NAMEREQD,
        NI_DGRAM,
        NI_NUMERICSCOPE,
    ] {
        assert_eq!(flag & NI_MASK, flag);
    }
}

#[test]
fn rr_types_match_rfc() {
    assert_eq!(
        (RR_A, RR_CNAME, RR_PTR, RR_AAAA, RR_OPT),
        (1, 5, 12, 28, 41)
    );
}

#[test]
fn class_in_is_one() {
    assert_eq!(CLASS_IN, 1);
}

#[test]
fn herror_values_match_netdb() {
    assert_eq!(
        (HOST_NOT_FOUND, TRY_AGAIN, NO_RECOVERY, NO_DATA),
        (1, 2, 3, 4)
    );
}

#[test]
fn protocol_numbers_match_iana() {
    assert_eq!((IPPROTO_TCP, IPPROTO_UDP), (6, 17));
}

#[test]
fn sock_types_match_linux() {
    assert_eq!((SOCK_STREAM, SOCK_DGRAM, SOCK_RAW), (1, 2, 3));
}

#[test]
fn dns_header_sizes() {
    assert_eq!(DNS_HEADER_LEN, 12);
    assert_eq!(DNS_RR_FIXED, 10);
    assert_eq!(DNS_QUESTION_FIXED, 4);
}

#[test]
fn order_and_engine_bounds() {
    assert_eq!(MAX_ORDER, 8);
    assert_eq!(ENGINE_MAX_STEPS, 4096);
}
