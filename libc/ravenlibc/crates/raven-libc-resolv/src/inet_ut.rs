// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `inet_*` family, the byte-order helpers and the IPv6 constants.

use super::*;
use alloc::vec::Vec;

macro_rules! aton_ok {
    ($name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(inet_aton($input), Some($expected));
        }
    };
}

macro_rules! aton_bad {
    ($name:ident, $input:expr) => {
        #[test]
        fn $name() {
            assert_eq!(inet_aton($input), None);
        }
    };
}

aton_ok!(aton_dotted_quad, b"127.0.0.1", [127, 0, 0, 1]);
aton_ok!(aton_zero, b"0.0.0.0", [0, 0, 0, 0]);
aton_ok!(aton_broadcast, b"255.255.255.255", [255, 255, 255, 255]);
aton_ok!(aton_simple, b"1.2.3.4", [1, 2, 3, 4]);
aton_ok!(aton_three_parts, b"1.2.3", [1, 2, 0, 3]);
aton_ok!(aton_two_parts, b"1.2", [1, 0, 0, 2]);
aton_ok!(aton_one_part, b"1", [0, 0, 0, 1]);
aton_ok!(aton_one_part_256, b"256", [0, 0, 1, 0]);
aton_ok!(aton_one_part_max, b"4294967295", [255, 255, 255, 255]);
aton_ok!(aton_hex_one, b"0x7f.1", [127, 0, 0, 1]);
aton_ok!(aton_hex_max, b"0xffffffff", [255, 255, 255, 255]);
aton_ok!(aton_octal, b"0177.0.0.1", [127, 0, 0, 1]);
aton_ok!(
    aton_octal_full,
    b"0377.0377.0377.0377",
    [255, 255, 255, 255]
);
aton_ok!(aton_octal_short, b"1.2.3.04", [1, 2, 3, 4]);
aton_ok!(aton_zero_only, b"0", [0, 0, 0, 0]);
aton_ok!(aton_class_a_24, b"16777216", [1, 0, 0, 0]);
aton_ok!(aton_two_part_host, b"1.16777215", [1, 255, 255, 255]);

aton_bad!(aton_empty, b"");
aton_bad!(aton_space, b" ");
aton_bad!(aton_letters, b"abc");
aton_bad!(aton_plus, b"+1");
aton_bad!(aton_5_parts, b"1.2.3.4.5");
aton_bad!(aton_octet_256, b"1.2.3.256");
aton_bad!(aton_trailing_dot, b"1.2.3.");
aton_bad!(aton_trailing_text, b"1.2.3.4x");
aton_bad!(aton_double_dot, b"1..2");
aton_bad!(aton_octal_9, b"09");
aton_bad!(aton_hex_no_digits, b"0x");
aton_bad!(aton_hex_tail, b"0x1g");
aton_bad!(aton_too_large, b"4294967296");
aton_bad!(aton_two_part_overflow, b"1.16777216");
aton_bad!(aton_hex_overflow, b"0x100000000");

macro_rules! pton4_ok {
    ($name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let mut out = [0u8; 4];
            assert_eq!(inet_pton(AF_INET, $input, &mut out), 1);
            assert_eq!(out, $expected);
        }
    };
}

macro_rules! pton4_bad {
    ($name:ident, $input:expr) => {
        #[test]
        fn $name() {
            let mut out = [0u8; 4];
            assert_eq!(inet_pton(AF_INET, $input, &mut out), 0);
        }
    };
}

pton4_ok!(pton4_loopback, b"127.0.0.1", [127, 0, 0, 1]);
pton4_ok!(pton4_zero, b"0.0.0.0", [0, 0, 0, 0]);
pton4_ok!(pton4_max, b"255.255.255.255", [255, 255, 255, 255]);

pton4_bad!(pton4_leading_zero, b"01.2.3.4");
pton4_bad!(pton4_leading_zero_mid, b"1.02.3.4");
pton4_bad!(pton4_leading_zero_last, b"1.2.3.04");
pton4_bad!(pton4_three_parts, b"1.2.3");
pton4_bad!(pton4_five_parts, b"1.2.3.4.5");
pton4_bad!(pton4_256, b"256.0.0.1");
pton4_bad!(pton4_four_digits, b"1234.1.1.1");
pton4_bad!(pton4_trailing_space, b"1.2.3.4 ");
pton4_bad!(pton4_trailing_char, b"1.2.3.4x");
pton4_bad!(pton4_empty, b"");
pton4_bad!(pton4_double_dot, b"1..2.3");

#[test]
fn pton4_needs_a_four_byte_output() {
    let mut out = [0u8; 3];
    assert_eq!(inet_pton(AF_INET, b"127.0.0.1", &mut out), 0);
}

#[test]
fn pton_rejects_an_unknown_family() {
    let mut out = [0u8; 16];
    assert_eq!(inet_pton(99, b"1.2.3.4", &mut out), -1);
}

macro_rules! pton6_ok {
    ($name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let mut out = [0u8; 16];
            assert_eq!(inet_pton(AF_INET6, $input, &mut out), 1);
            assert_eq!(out, $expected);
        }
    };
}

macro_rules! pton6_bad {
    ($name:ident, $input:expr) => {
        #[test]
        fn $name() {
            let mut out = [0u8; 16];
            assert_eq!(inet_pton(AF_INET6, $input, &mut out), 0);
        }
    };
}

pton6_ok!(pton6_any, b"::", [0u8; 16]);
pton6_ok!(
    pton6_loopback,
    b"::1",
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
);
pton6_ok!(
    pton6_prefix,
    b"1::",
    [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
);
pton6_ok!(
    pton6_full,
    b"1:2:3:4:5:6:7:8",
    [0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8]
);
pton6_ok!(
    pton6_documentation,
    b"2001:db8::1",
    [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
);
pton6_ok!(
    pton6_v4_mapped,
    b"::ffff:192.0.2.1",
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 1]
);
pton6_ok!(pton6_leading_zeros, b"0:0:0:0:0:0:0:0", [0u8; 16]);
pton6_ok!(
    pton6_seven_with_zero,
    b"1:2:3:4:5:6:7::",
    [0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 0]
);
pton6_ok!(
    pton6_leading_double_colon,
    b"::1:2:3:4:5:6:7",
    [0, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7]
);
pton6_ok!(
    pton6_uppercase,
    b"ABCD::EF",
    [0xab, 0xcd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0xef]
);
pton6_ok!(
    pton6_v4_tail,
    b"::1.2.3.4",
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]
);

pton6_bad!(pton6_v4_alone, b"1.2.3.4");
pton6_bad!(pton6_single_colon, b":");
pton6_bad!(pton6_triple_colon, b":::");
pton6_bad!(pton6_nine_groups, b"1:2:3:4:5:6:7:8:9");
pton6_bad!(pton6_seven_groups, b"1:2:3:4:5:6:7");
pton6_bad!(pton6_trailing_colon, b"1:2:3:4:5:6:7:8:");
pton6_bad!(pton6_scope, b"fe80::1%eth0");
pton6_bad!(pton6_empty, b"");
pton6_bad!(pton6_letters, b"gggg::1");
pton6_bad!(pton6_v4_bad_tail, b"::ffff:256.1.1.1");

#[test]
fn pton6_needs_a_sixteen_byte_output() {
    let mut out = [0u8; 15];
    assert_eq!(inet_pton(AF_INET6, b"::1", &mut out), 0);
}

fn ntop4(a: [u8; 4]) -> alloc::string::String {
    let mut out = [0u8; 64];
    let len = inet_ntop(AF_INET, &a, &mut out).expect("ntop");
    alloc::string::String::from_utf8(out[..len].to_vec()).expect("utf8")
}

fn ntop6(a: [u8; 16]) -> alloc::string::String {
    let mut out = [0u8; 64];
    let len = inet_ntop(AF_INET6, &a, &mut out).expect("ntop");
    alloc::string::String::from_utf8(out[..len].to_vec()).expect("utf8")
}

fn v6(groups: [u16; 8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (index, group) in groups.iter().enumerate() {
        out[index * 2] = u8::try_from(group >> 8).unwrap_or(0);
        out[index * 2 + 1] = u8::try_from(group & 0xff).unwrap_or(0);
    }
    out
}

#[test]
fn ntop4_loopback() {
    assert_eq!(ntop4([127, 0, 0, 1]), "127.0.0.1");
}

#[test]
fn ntop4_zero() {
    assert_eq!(ntop4([0, 0, 0, 0]), "0.0.0.0");
}

#[test]
fn ntop4_broadcast() {
    assert_eq!(ntop4([255, 255, 255, 255]), "255.255.255.255");
}

#[test]
fn ntop6_any() {
    assert_eq!(ntop6([0u8; 16]), "::");
}

#[test]
fn ntop6_loopback() {
    let mut bytes = [0u8; 16];
    bytes[15] = 1;
    assert_eq!(ntop6(bytes), "::1");
}

#[test]
fn ntop6_documentation() {
    assert_eq!(ntop6(v6([0x2001, 0x0db8, 0, 0, 0, 0, 0, 1])), "2001:db8::1");
}

#[test]
fn ntop6_no_long_zero_run() {
    assert_eq!(
        ntop6(v6([0x2001, 0x0db8, 0, 1, 1, 1, 1, 1])),
        "2001:db8:0:1:1:1:1:1"
    );
}

#[test]
fn ntop6_prefers_the_longer_run() {
    assert_eq!(ntop6(v6([0x2001, 0, 0, 1, 0, 0, 0, 1])), "2001:0:0:1::1");
}

#[test]
fn ntop6_v4_mapped() {
    let mut bytes = [0u8; 16];
    bytes[10] = 0xff;
    bytes[11] = 0xff;
    bytes[12] = 127;
    bytes[15] = 1;
    assert_eq!(ntop6(bytes), "::ffff:127.0.0.1");
}

#[test]
fn ntop6_v4_mapped_zero() {
    let mut bytes = [0u8; 16];
    bytes[10] = 0xff;
    bytes[11] = 0xff;
    assert_eq!(ntop6(bytes), "::ffff:0.0.0.0");
}

#[test]
fn ntop6_link_local() {
    assert_eq!(ntop6(v6([0xfe80, 0, 0, 0, 0, 0, 0, 1])), "fe80::1");
}

#[test]
fn ntop6_trailing_run() {
    assert_eq!(ntop6(v6([1, 2, 3, 4, 5, 6, 0, 0])), "1:2:3:4:5:6::");
}

#[test]
fn ntop6_first_tie_wins() {
    assert_eq!(ntop6(v6([1, 0, 0, 2, 0, 0, 3, 4])), "1::2:0:0:3:4");
}

#[test]
fn ntop_v6_exact_fit_is_enospc_free() {
    let mut bytes = [0u8; 16];
    bytes[15] = 1;
    let mut out = [0u8; 3];
    assert_eq!(inet_ntop(AF_INET6, &bytes, &mut out), Ok(3));
    assert_eq!(&out, b"::1");
}

#[test]
fn ntop_enospc_for_a_v4_buffer() {
    let mut out = [0u8; 7];
    assert_eq!(inet_ntop(AF_INET, &[127, 0, 0, 1], &mut out), Err(ENOSPC));
}

#[test]
fn ntop_rejects_an_unknown_family() {
    let mut out = [0u8; 64];
    assert_eq!(inet_ntop(99, &[0u8; 16], &mut out), Err(EAFNOSUPPORT));
}

#[test]
fn ntop_rejects_a_short_address() {
    let mut out = [0u8; 64];
    assert_eq!(inet_ntop(AF_INET, &[1, 2, 3], &mut out), Err(EAFNOSUPPORT));
    assert_eq!(inet_ntop(AF_INET6, &[0u8; 15], &mut out), Err(EAFNOSUPPORT));
}

#[test]
fn byte_order_16_round_trips() {
    assert_eq!(ntohs(htons(0x1234)), 0x1234);
    assert_eq!(htons(0x1234), 0x1234u16.to_be());
}

#[test]
fn byte_order_32_round_trips() {
    assert_eq!(ntohl(htonl(0x1234_5678)), 0x1234_5678);
}

#[test]
fn htons_is_big_endian() {
    assert_eq!(htons(0x0102), 0x0102u16.to_be());
}

#[test]
fn htonl_is_big_endian() {
    assert_eq!(htonl(0x0102_0304), 0x0102_0304u32.to_be());
}

#[test]
fn in6addr_any_is_zero() {
    assert_eq!(IN6ADDR_ANY, [0u8; 16]);
}

#[test]
fn in6addr_loopback_is_one() {
    assert_eq!(IN6ADDR_LOOPBACK[15], 1);
    assert_eq!(&IN6ADDR_LOOPBACK[..15], &[0u8; 15][..]);
}

#[test]
fn inet_addr_returns_the_c_integer_convention() {
    assert_eq!(inet_addr(b"127.0.0.1"), u32::from_ne_bytes([127, 0, 0, 1]));
}

#[test]
fn inet_addr_invalid_is_inaddr_none() {
    assert_eq!(inet_addr(b"not-an-address"), u32::MAX);
}

#[test]
fn inet_network_strips_the_host_part() {
    assert_eq!(
        inet_network(b"127.0.0.1"),
        u32::from_be_bytes([127, 0, 0, 1])
    );
}

#[test]
fn inet_ntoa_formats_a_quad() {
    assert_eq!(inet_ntoa([192, 0, 2, 1]), "192.0.2.1");
}

#[test]
fn inet_makeaddr_class_a() {
    assert_eq!(inet_makeaddr(10, 1), 0x0a00_0001);
}

#[test]
fn inet_makeaddr_class_b() {
    assert_eq!(inet_makeaddr(0x8000, 1), 0x8000_0001);
}

#[test]
fn inet_makeaddr_class_c() {
    assert_eq!(inet_makeaddr(0xc00000, 1), 0xc000_0001);
}

#[test]
fn inet_lnaof_class_a() {
    assert_eq!(inet_lnaof(0x0a00_0001), 1);
}

#[test]
fn inet_lnaof_class_b() {
    assert_eq!(inet_lnaof(0x8000_0001), 1);
}

#[test]
fn inet_lnaof_class_c() {
    assert_eq!(inet_lnaof(0xc000_0001), 1);
}

#[test]
fn inet_netof_class_a() {
    assert_eq!(inet_netof(0x0a00_0001), 10);
}

#[test]
fn inet_netof_class_b() {
    assert_eq!(inet_netof(0x8000_0001), 0x8000);
}

#[test]
fn inet_netof_class_c() {
    assert_eq!(inet_netof(0xc000_0001), 0xc00000);
}

#[test]
fn pton_ntop_round_trip_v4() {
    for text in [
        &b"0.0.0.0"[..],
        b"127.0.0.1",
        b"192.0.2.1",
        b"255.255.255.255",
    ] {
        let mut bytes = [0u8; 4];
        assert_eq!(inet_pton(AF_INET, text, &mut bytes), 1);
        let mut out = [0u8; 64];
        let len = inet_ntop(AF_INET, &bytes, &mut out).expect("ntop");
        assert_eq!(&out[..len], text);
    }
}

#[test]
fn pton_ntop_round_trip_v6() {
    for text in [
        &b"::"[..],
        b"::1",
        b"2001:db8::1",
        b"2001:db8:0:1:1:1:1:1",
        b"::ffff:192.0.2.1",
    ] {
        let mut bytes = [0u8; 16];
        assert_eq!(inet_pton(AF_INET6, text, &mut bytes), 1);
        let mut out = [0u8; 64];
        let len = inet_ntop(AF_INET6, &bytes, &mut out).expect("ntop");
        assert_eq!(&out[..len], text);
    }
}

#[test]
fn pton_accepts_every_ntop_output() {
    let mut seen: Vec<alloc::string::String> = Vec::new();
    for last in 0u8..=255 {
        let mut bytes = [0u8; 16];
        bytes[15] = last;
        let text = ntop6(bytes);
        assert!(seen.iter().all(|entry| entry != &text));
        let mut parsed = [0u8; 16];
        assert_eq!(inet_pton(AF_INET6, text.as_bytes(), &mut parsed), 1);
        assert_eq!(parsed, bytes);
        seen.push(text);
    }
    assert_eq!(seen.len(), 256);
}
