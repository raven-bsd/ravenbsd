// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Binary subjects and patterns (`t_regex_binary.c` cases, rewritten).
//!
//! NUL is legal in a pattern with `REG_PEND` (the slice form) and in a subject with
//! `REG_STARTEND`; bytes `0x80..=0xFF` match themselves in the byte locale. The two donor
//! cases are re-implemented from their behaviour (the donor file is BSD-4, STUDY).

use raven_libc_regex::consts::{REG_EXTENDED, REG_NOMATCH, REG_PEND, REG_STARTEND};
use raven_libc_regex::{ByteLocale, RegError, RegMatch, Utf8Locale, regcomp, regexec};

/// Run one match with `nmatch` slots.
fn run(
    pattern: &[u8],
    subject: &[u8],
    cflags: u32,
    eflags: u32,
    window: Option<(i64, i64)>,
) -> Result<[RegMatch; 10], i32> {
    let re = match regcomp(pattern, cflags, &ByteLocale) {
        Ok(re) => re,
        Err(error) => return Err(error.0),
    };
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 10];
    if let Some((start, end)) = window {
        slots[0].rm_so = start;
        slots[0].rm_eo = end;
        let effective = eflags | REG_STARTEND;
        return regexec(&re, subject, &mut slots, effective, &ByteLocale)
            .map(|()| slots)
            .map_err(|error| error.0);
    }
    regexec(&re, subject, &mut slots, eflags, &ByteLocale)
        .map(|()| slots)
        .map_err(|error| error.0)
}

#[test]
fn embedded_nul_pattern_with_pend() {
    let slots = run(b"a\0b", b"xa\0by", REG_EXTENDED | REG_PEND, 0, None).expect("match");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (1, 4));
}

#[test]
fn embedded_nul_subject_with_startend() {
    let slots = run(b"a.b", b"a\0b", REG_EXTENDED, 0, Some((0, 3))).expect("match");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (0, 3));
}

#[test]
fn a_window_ending_at_a_nul_is_a_real_end() {
    let slots = run(b"b", b"a\0b", REG_EXTENDED, 0, Some((0, 2)));
    assert!(matches!(slots, Err(code) if code == REG_NOMATCH));
}

#[test]
fn negative_ranges_compile_in_byte_mode() {
    // t_regex_binary.c negative_ranges.
    let re = regcomp(b"[\xe0-\xf1][\xa0-\xd1].*", REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("negative range regcomp {}", error.0));
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 1];
    regexec(&re, b"\xe1\xa1x", &mut slots, 0, &ByteLocale)
        .unwrap_or_else(|e| panic!("exec {}", e.0));
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (0, 3));
}

#[test]
fn negative_char_compiles_in_byte_mode() {
    // t_regex_binary.c negative_char (PR/58910).
    regcomp(b": j:[]j:[]j:[\xd9j:[]", REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("negative char regcomp {}", error.0));
}

#[test]
fn high_bytes_match_themselves_in_byte_mode() {
    for byte in [0x80u8, 0xa5, 0xe9, 0xff] {
        let slots = run(&[byte], &[b'x', byte, b'y'], REG_EXTENDED, 0, None).expect("match");
        assert_eq!((slots[0].rm_so, slots[0].rm_eo), (1, 2));
    }
}

#[test]
fn dot_is_one_byte_in_byte_mode() {
    let slots = run(b".", &[0xe9, b'x'], REG_EXTENDED, 0, None).expect("match");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (0, 1));
}

#[test]
fn dot_is_one_code_point_in_utf8_mode() {
    let re = regcomp(b".", REG_EXTENDED, &Utf8Locale).unwrap_or_else(|e| panic!("regcomp {}", e.0));
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 1];
    regexec(&re, "\u{e9}x".as_bytes(), &mut slots, 0, &Utf8Locale)
        .unwrap_or_else(|e| panic!("exec {}", e.0));
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (0, 2));
}

#[test]
fn startend_windows_at_every_offset_of_a_32_byte_subject() {
    let mut subject = vec![b'a'; 31];
    subject.push(b'b');
    let re = regcomp(b"b", REG_EXTENDED, &ByteLocale).unwrap_or_else(|e| panic!("regcomp {}", e.0));
    for start in 0..=subject.len() {
        for end in start..=subject.len() {
            let mut slots = [RegMatch {
                rm_so: start as i64,
                rm_eo: end as i64,
            }; 2];
            let result = regexec(&re, &subject, &mut slots, REG_STARTEND, &ByteLocale);
            let expected = subject
                .get(start..end)
                .and_then(|window| window.iter().position(|&byte| byte == b'b'))
                .map(|offset| start + offset);
            match expected {
                Some(position) => {
                    assert!(result.is_ok(), "window {start}..{end}");
                    assert_eq!(slots[0].rm_so, position as i64);
                    assert_eq!(slots[0].rm_eo, position as i64 + 1);
                }
                None => assert!(matches!(result, Err(RegError(REG_NOMATCH)))),
            }
        }
    }
}

#[test]
fn startend_does_not_imply_notbol() {
    let slots = run(b"^b", b"ab", REG_EXTENDED, 0, Some((1, 2))).expect("window start is BOL");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (1, 2));
}

#[test]
fn an_empty_window_matches_an_empty_expression() {
    let slots = run(b"^$", b"abc", REG_EXTENDED, 0, Some((2, 2))).expect("empty window");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (2, 2));
    let slots = run(b"a", b"abc", REG_EXTENDED, 0, Some((2, 2)));
    assert!(matches!(slots, Err(code) if code == REG_NOMATCH));
}
