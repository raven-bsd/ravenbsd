// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exhaustive `<ctype.h>` truth table for `-128..=255`, `EOF` and the `i32` extremes.

use raven_libc_ctype::*;

fn letter(c: i32) -> bool {
    (65..=90).contains(&c) || (97..=122).contains(&c)
}

fn digit(c: i32) -> bool {
    (48..=57).contains(&c)
}

fn space(c: i32) -> bool {
    c == 32 || (9..=13).contains(&c)
}

fn expected_isalpha(c: i32) -> bool {
    letter(c)
}

fn expected_isalnum(c: i32) -> bool {
    letter(c) || digit(c)
}

fn expected_isblank(c: i32) -> bool {
    c == 32 || c == 9
}

fn expected_iscntrl(c: i32) -> bool {
    (0..=31).contains(&c) || c == 127
}

fn expected_isdigit(c: i32) -> bool {
    digit(c)
}

fn expected_isgraph(c: i32) -> bool {
    (33..=126).contains(&c)
}

fn expected_islower(c: i32) -> bool {
    (97..=122).contains(&c)
}

fn expected_isprint(c: i32) -> bool {
    (32..=126).contains(&c)
}

fn expected_ispunct(c: i32) -> bool {
    expected_isgraph(c) && !expected_isalnum(c)
}

fn expected_isspace(c: i32) -> bool {
    space(c)
}

fn expected_isupper(c: i32) -> bool {
    (65..=90).contains(&c)
}

fn expected_isxdigit(c: i32) -> bool {
    digit(c) || (97..=102).contains(&c) || (65..=70).contains(&c)
}

#[test]
fn truth_table_minus_128_to_255() {
    for c in -128..=255 {
        assert_eq!(isalpha(c), expected_isalpha(c), "isalpha({c})");
        assert_eq!(isalnum(c), expected_isalnum(c), "isalnum({c})");
        assert_eq!(isblank(c), expected_isblank(c), "isblank({c})");
        assert_eq!(iscntrl(c), expected_iscntrl(c), "iscntrl({c})");
        assert_eq!(isdigit(c), expected_isdigit(c), "isdigit({c})");
        assert_eq!(isgraph(c), expected_isgraph(c), "isgraph({c})");
        assert_eq!(islower(c), expected_islower(c), "islower({c})");
        assert_eq!(isprint(c), expected_isprint(c), "isprint({c})");
        assert_eq!(ispunct(c), expected_ispunct(c), "ispunct({c})");
        assert_eq!(isspace(c), expected_isspace(c), "isspace({c})");
        assert_eq!(isupper(c), expected_isupper(c), "isupper({c})");
        assert_eq!(isxdigit(c), expected_isxdigit(c), "isxdigit({c})");
        assert_eq!(isascii(c), (0..=127).contains(&c), "isascii({c})");
        assert_eq!(toascii(c), c & 0x7f, "toascii({c})");
    }
}

#[test]
fn eof_and_i32_extremes_do_not_panic_or_classify() {
    for c in [
        EOF,
        i32::MIN,
        i32::MIN + 1,
        i32::MAX,
        -300,
        300,
        0xE9,
        0x1_0000,
    ] {
        assert!(!isalpha(c), "isalpha({c})");
        assert!(!isalnum(c), "isalnum({c})");
        assert!(!isblank(c), "isblank({c})");
        assert!(!iscntrl(c), "iscntrl({c})");
        assert!(!isdigit(c), "isdigit({c})");
        assert!(!isgraph(c), "isgraph({c})");
        assert!(!islower(c), "islower({c})");
        assert!(!isprint(c), "isprint({c})");
        assert!(!ispunct(c), "ispunct({c})");
        assert!(!isspace(c), "isspace({c})");
        assert!(!isupper(c), "isupper({c})");
        assert!(!isxdigit(c), "isxdigit({c})");
        assert_eq!(tolower(c), c, "tolower({c})");
        assert_eq!(toupper(c), c, "toupper({c})");
    }
    assert_eq!(toupper(EOF), EOF);
}

#[test]
fn case_mapping_is_ascii_only() {
    for c in 0..=255 {
        let lower = tolower(c);
        let upper = toupper(c);
        if (65..=90).contains(&c) {
            assert_eq!(lower, c + 32);
            assert_eq!(upper, c);
        } else if (97..=122).contains(&c) {
            assert_eq!(upper, c - 32);
            assert_eq!(lower, c);
        } else {
            assert_eq!(lower, c);
            assert_eq!(upper, c);
        }
    }
    assert_eq!(toupper(0xE9), 0xE9);
    assert_eq!(tolower(0xC9), 0xC9);
}
