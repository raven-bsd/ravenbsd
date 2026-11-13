// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the RFC 1035 name codec and the hostname sanity check.

use super::*;

fn text(buffer: &[u8; 256], len: usize) -> alloc::string::String {
    alloc::string::String::from_utf8(buffer[..len].to_vec()).expect("utf8")
}

fn rfc1035_message() -> alloc::vec::Vec<u8> {
    let mut msg = alloc::vec::Vec::new();
    msg.extend_from_slice(&[1, b'F']);
    msg.extend_from_slice(&[3, b'I', b'S', b'I']);
    msg.extend_from_slice(&[4, b'A', b'R', b'P', b'A']);
    msg.push(0);
    msg.extend_from_slice(&[3, b'F', b'O', b'O', 0xc0, 0x00]);
    msg.extend_from_slice(&[0xc0, 0x06]);
    msg
}

#[test]
fn expand_f() {
    let msg = rfc1035_message();
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 0, &mut out), Some((12, 10)));
    assert_eq!(text(&out, 10), "F.ISI.ARPA");
}

#[test]
fn expand_isi() {
    let msg = rfc1035_message();
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 2, &mut out), Some((10, 8)));
    assert_eq!(text(&out, 8), "ISI.ARPA");
}

#[test]
fn expand_arpa() {
    let msg = rfc1035_message();
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 6, &mut out), Some((6, 4)));
    assert_eq!(text(&out, 4), "ARPA");
}

#[test]
fn expand_foo() {
    let msg = rfc1035_message();
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 12, &mut out), Some((6, 14)));
    assert_eq!(text(&out, 14), "FOO.F.ISI.ARPA");
}

#[test]
fn expand_a_bare_pointer() {
    let msg = rfc1035_message();
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 18, &mut out), Some((2, 4)));
    assert_eq!(text(&out, 4), "ARPA");
}

#[test]
fn expand_root() {
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&[0], 0, &mut out), Some((1, 0)));
    assert_eq!(out[0], 0);
}

#[test]
fn expand_rejects_a_position_past_the_end() {
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&[0], 1, &mut out), None);
}

#[test]
fn expand_rejects_a_truncated_label() {
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&[5, b'a', b'b'], 0, &mut out), None);
}

#[test]
fn expand_rejects_a_pointer_beyond_the_message() {
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&[0xc0, 0xff], 0, &mut out), None);
}

#[test]
fn expand_rejects_a_self_pointer() {
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&[0xc0, 0x00], 0, &mut out), None);
}

#[test]
fn expand_rejects_a_pointer_pair_loop() {
    let msg = [0xc0, 0x02, 0xc0, 0x00];
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 0, &mut out), None);
}

#[test]
fn expand_rejects_a_long_pointer_chain() {
    let mut msg = alloc::vec![0u8; 600];
    for index in 0..300usize {
        msg[index * 2] = 0xc0;
        msg[index * 2 + 1] = u8::try_from((index + 1) * 2).unwrap_or(0);
    }
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 0, &mut out), None);
}

#[test]
fn expand_rejects_text_over_254_bytes() {
    let mut msg = alloc::vec::Vec::new();
    for _ in 0..5 {
        msg.push(63);
        msg.extend_from_slice(&[b'a'; 63]);
    }
    msg.push(0);
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&msg, 0, &mut out), None);
}

#[test]
fn expand_a_long_valid_name() {
    let mut msg = alloc::vec::Vec::new();
    msg.push(63);
    msg.extend_from_slice(&[b'a'; 63]);
    msg.push(63);
    msg.extend_from_slice(&[b'b'; 63]);
    msg.push(63);
    msg.extend_from_slice(&[b'c'; 63]);
    msg.push(61);
    msg.extend_from_slice(&[b'd'; 61]);
    msg.push(0);
    let mut out = [0u8; 256];
    let (_, len) = dn_expand(&msg, 0, &mut out).expect("long name");
    assert_eq!(len, 253);
}

#[test]
fn skipname_counts_a_plain_name() {
    let msg = rfc1035_message();
    assert_eq!(dn_skipname(&msg, 0), Some(12));
    assert_eq!(dn_skipname(&msg, 2), Some(10));
    assert_eq!(dn_skipname(&msg, 6), Some(6));
}

#[test]
fn skipname_counts_a_pointer_as_two_bytes() {
    let msg = rfc1035_message();
    assert_eq!(dn_skipname(&msg, 18), Some(2));
}

#[test]
fn skipname_rejects_truncation() {
    assert_eq!(dn_skipname(&[3, b'a'], 0), None);
    assert_eq!(dn_skipname(&[0xc0], 0), None);
}

#[test]
fn comp_root_name() {
    let mut dst = [0u8; 32];
    assert_eq!(dn_comp(b".", &mut dst, &mut [], None), Some(1));
    assert_eq!(dst[0], 0);
}

#[test]
fn comp_a_plain_name() {
    let mut dst = [0u8; 64];
    let length = dn_comp(b"F.ISI.ARPA", &mut dst, &mut [], None).expect("comp");
    assert_eq!(length, 12);
    assert_eq!(
        &dst[..12],
        &[1, b'F', 3, b'I', b'S', b'I', 4, b'A', b'R', b'P', b'A', 0]
    );
    let mut out = [0u8; 256];
    assert_eq!(dn_expand(&dst[..12], 0, &mut out), Some((12, 10)));
    assert_eq!(text(&out, 10), "F.ISI.ARPA");
}

#[test]
fn comp_strips_one_trailing_dot() {
    let mut dst = [0u8; 64];
    let length = dn_comp(b"F.ISI.ARPA.", &mut dst, &mut [], None).expect("comp");
    assert_eq!(length, 12);
}

#[test]
fn comp_rejects_two_trailing_dots() {
    let mut dst = [0u8; 64];
    assert_eq!(dn_comp(b"F.ISI.ARPA..", &mut dst, &mut [], None), None);
}

#[test]
fn comp_rejects_an_empty_label() {
    let mut dst = [0u8; 64];
    assert_eq!(dn_comp(b"a..b", &mut dst, &mut [], None), None);
}

#[test]
fn comp_a_63_byte_label_is_accepted() {
    let mut name = alloc::vec::Vec::new();
    name.extend_from_slice(&[b'a'; 63]);
    name.extend_from_slice(b".com");
    let mut dst = [0u8; 128];
    assert!(dn_comp(&name, &mut dst, &mut [], None).is_some());
}

#[test]
fn comp_a_64_byte_label_is_rejected() {
    let mut name = alloc::vec::Vec::new();
    name.extend_from_slice(&[b'a'; 64]);
    name.extend_from_slice(b".com");
    let mut dst = [0u8; 128];
    assert_eq!(dn_comp(&name, &mut dst, &mut [], None), None);
}

fn long_name(last: usize) -> alloc::vec::Vec<u8> {
    let mut name = alloc::vec::Vec::new();
    name.extend_from_slice(&[b'a'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'b'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'c'; 63]);
    name.push(b'.');
    name.extend_from_slice(&alloc::vec![b'd'; last]);
    name
}

#[test]
fn comp_a_253_byte_name_is_accepted() {
    let name = long_name(61);
    assert_eq!(name.len(), 253);
    let mut dst = [0u8; 280];
    assert!(dn_comp(&name, &mut dst, &mut [], None).is_some());
}

#[test]
fn comp_a_254_byte_name_is_rejected() {
    let name = long_name(62);
    assert_eq!(name.len(), 254);
    let mut dst = [0u8; 280];
    assert_eq!(dn_comp(&name, &mut dst, &mut [], None), None);
}

#[test]
fn comp_rejects_a_buffer_that_is_too_small() {
    let mut dst = [0u8; 4];
    assert_eq!(dn_comp(b"example.org", &mut dst, &mut [], None), None);
}

#[test]
fn comp_uses_a_suffix_pointer() {
    let mut dst = [0u8; 128];
    let first = dn_comp(b"F.ISI.ARPA", &mut dst, &mut [], None).expect("first");
    assert_eq!(first, 12);
    let mut table = [12usize, 0, 0];
    let second = dn_comp(b"FOO.F.ISI.ARPA", &mut dst, &mut table, Some(2)).expect("second");
    assert_eq!(second, 6);
    assert_eq!(&dst[12..18], &[3, b'F', b'O', b'O', 0xc0, 0x00]);
    let mut out = [0u8; 256];
    let (_, len) = dn_expand(&dst[..18], 12, &mut out).expect("expand");
    assert_eq!(text(&out, len), "FOO.F.ISI.ARPA");
}

#[test]
fn comp_a_full_match_is_a_pointer_only() {
    let mut dst = [0u8; 128];
    let first = dn_comp(b"F.ISI.ARPA", &mut dst, &mut [], None).expect("first");
    let mut table = [first, 0, 0];
    let second = dn_comp(b"F.ISI.ARPA", &mut dst, &mut table, Some(2)).expect("second");
    assert_eq!(second, 2);
    assert_eq!(&dst[first..first + 2], &[0xc0, 0x00]);
}

#[test]
fn comp_appends_to_the_pointer_table() {
    let mut dst = [0u8; 128];
    let first = dn_comp(b"example.org", &mut dst, &mut [], None).expect("first");
    let mut table = [first, 0, 0];
    dn_comp(b"www.example.org", &mut dst, &mut table, Some(2)).expect("second");
    assert_eq!(table[2], first);
}

#[test]
fn comp_ignores_an_out_of_range_pointer() {
    let mut dst = [0u8; 64];
    dn_comp(b"example.org", &mut dst, &mut [], None).expect("first");
    let mut table = [14usize, 9999];
    let length = dn_comp(b"www.example.org", &mut dst, &mut table, Some(2)).expect("second");
    assert!(length >= 17);
}

#[test]
fn comp_rejects_a_write_position_past_the_buffer() {
    let mut dst = [0u8; 8];
    let mut table = [9usize];
    assert_eq!(dn_comp(b"a", &mut dst, &mut table, None), None);
}

#[test]
fn valid_hostname_accepts_letters_digits_dots_and_dashes() {
    assert!(is_valid_hostname("example.org"));
    assert!(is_valid_hostname("a-b.c9"));
    assert!(is_valid_hostname("localhost."));
}

#[test]
fn valid_hostname_rejects_empty_and_long() {
    assert!(!is_valid_hostname(""));
    assert!(is_valid_hostname(&"a".repeat(254)));
    assert!(!is_valid_hostname(&"a".repeat(255)));
}

#[test]
fn valid_hostname_rejects_controls_and_spaces() {
    assert!(!is_valid_hostname("bad name"));
    assert!(!is_valid_hostname("bad\nname"));
    assert!(!is_valid_hostname("bad\tname"));
}

#[test]
fn valid_hostname_rejects_underscores() {
    assert!(!is_valid_hostname("bad_name"));
    assert!(!is_valid_hostname("host_"));
}
