// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn input_for(cflags: u8, eflags: u8, pattern: &[u8], subject: &[u8]) -> Vec<u8> {
    let length = u16::try_from(pattern.len()).unwrap_or(u16::MAX);
    let mut input = vec![cflags, eflags];
    input.extend_from_slice(&length.to_le_bytes());
    input.extend_from_slice(pattern);
    input.extend_from_slice(subject);
    input
}

#[test]
fn regex_compile_exec_hostile_ok() {
    body(&input_for(0, 0, b"(a|b)*c", b"abababc"));
    body(&input_for(0x40, 0, b"\xc3\xa9+", b"\xc3\xa9\xc3\xa9"));
}

#[test]
fn regex_compile_exec_boundaries_ok() {
    for pattern in [
        b"(".as_slice(),
        b")",
        b"\\(",
        b"[",
        b"[[:",
        b"a{",
        b"a{1,0}",
        b"a{256}",
        b"\\1",
        b"(a)\\2",
        b"((((((((((a))))))))))\\10",
        b"a**",
        b"^*",
    ] {
        for cflags in [0u8, 1, 2, 4, 8, 16, 32, 64, 255] {
            body(&input_for(cflags, 7, pattern, b"aaab"));
        }
    }
    body(&input_for(1, 0, &[b'('; 300], b"aaaaaaaaaaaaaaaa"));
}

#[test]
#[should_panic(expected = "P-regex-3")]
fn regex_compile_exec_mutation_3_fires() {
    let entry = RegMatch { rm_so: 5, rm_eo: 4 };
    let subject_len = 10usize;
    assert!(
        entry.rm_so <= entry.rm_eo && (entry.rm_eo as usize) <= subject_len,
        "P-regex-3: a match offset left the subject"
    );
}

#[test]
fn startend_with_ten_matches_is_in_range() {
    body(&input_for(0, 4, b"(a)(b)", b"ab"));
}

#[test]
fn regerror_length_is_buffer_independent() {
    let mut large = [0u8; 128];
    let mut small = [0u8; 1];
    assert_eq!(
        regerror(REG_BADPAT, None, &mut large),
        regerror(REG_BADPAT, None, &mut small)
    );
}
