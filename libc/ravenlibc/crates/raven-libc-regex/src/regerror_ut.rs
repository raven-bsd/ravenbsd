// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the error messages and the ATOI/ITOA conversions.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

/// The 17 donor messages, in donor order.
const MESSAGES: [&str; 17] = [
    "regexec() failed to match",
    "invalid regular expression",
    "invalid collating element",
    "invalid character class",
    "trailing backslash (\\)",
    "invalid backreference number",
    "brackets ([ ]) not balanced",
    "parentheses not balanced",
    "braces not balanced",
    "invalid repetition count(s)",
    "invalid character range",
    "out of memory",
    "repetition-operator operand invalid",
    "empty (sub)expression",
    "\"can't happen\" -- you found a bug",
    "invalid argument to regex routine",
    "illegal byte sequence",
];

#[test]
fn every_message_and_length_is_exact() {
    for (index, message) in MESSAGES.iter().enumerate() {
        let code = (index as i32) + 1;
        let mut buffer = [0u8; 128];
        let needed = regerror(code, None, &mut buffer);
        assert_eq!(needed, message.len() + 1);
        assert_eq!(&buffer[..message.len()], message.as_bytes());
        assert_eq!(buffer[message.len()], 0);
    }
}

#[test]
fn itoa_gives_the_donor_name() {
    let mut buffer = [0u8; 64];
    let needed = regerror(REG_ITOA | REG_BADPAT, None, &mut buffer);
    assert_eq!(needed, b"REG_BADPAT".len() + 1);
    assert_eq!(&buffer[..needed - 1], b"REG_BADPAT");
}

#[test]
fn itoa_unknown_code_uses_hex() {
    let mut buffer = [0u8; 64];
    let needed = regerror(REG_ITOA | 0x2a, None, &mut buffer);
    assert_eq!(&buffer[..needed - 1], b"REG_0x2a");
}

#[test]
fn unknown_code_explains_itself() {
    let mut buffer = [0u8; 64];
    let needed = regerror(99, None, &mut buffer);
    assert_eq!(&buffer[..needed - 1], b"*** unknown regexp error code ***");
}

#[test]
fn atoi_converts_names_and_unknown_names() {
    assert_eq!(regatoi(b"REG_BADPAT"), REG_BADPAT);
    assert_eq!(regatoi(b"REG_NOMATCH"), REG_NOMATCH);
    assert_eq!(regatoi(b"REG_ILLSEQ"), REG_ILLSEQ);
    assert_eq!(regatoi(b"REG_NOPE"), 0);
    let mut buffer = [0u8; 8];
    let written = regatoi_str(b"REG_BADBR", &mut buffer);
    assert_eq!(&buffer[..written], b"10");
}

#[test]
fn buffer_sizes_zero_one_exact_and_short() {
    let expected = b"invalid regular expression";
    let needed = expected.len() + 1;

    let mut none = [0u8; 0];
    assert_eq!(regerror(REG_BADPAT, None, &mut none), needed);

    let mut one = [0xffu8; 1];
    assert_eq!(regerror(REG_BADPAT, None, &mut one), needed);
    assert_eq!(one, [0]);

    let mut exact = [0xffu8; 32];
    assert_eq!(regerror(REG_BADPAT, None, &mut exact[..needed]), needed);
    assert_eq!(&exact[..needed - 1], expected);
    assert_eq!(exact[needed - 1], 0);

    let mut short = [0xffu8; 10];
    assert_eq!(regerror(REG_BADPAT, None, &mut short), needed);
    assert_eq!(&short[..9], &expected[..9]);
    assert_eq!(short[9], 0);
}

#[test]
fn atoi_through_regerror_reports_the_unknown_name_result() {
    let mut buffer = [0u8; 8];
    let needed = regerror(REG_ATOI, None, &mut buffer);
    assert_eq!(needed, 2);
    assert_eq!(&buffer[..1], b"0");
}

macro_rules! message_cases {
    ($($name:ident => ($code:expr, $text:expr, $symbol:expr);)*) => {
        $(
            #[test]
            fn $name() {
                let mut buffer = [0u8; 96];
                let needed = regerror($code, None, &mut buffer);
                assert_eq!(needed, $text.len() + 1);
                assert_eq!(&buffer[..$text.len()], $text.as_bytes());
                assert_eq!(buffer[$text.len()], 0);

                let mut named = [0u8; 96];
                let needed = regerror(REG_ITOA | $code, None, &mut named);
                assert_eq!(needed, $symbol.len() + 1);
                assert_eq!(&named[..$symbol.len()], $symbol.as_bytes());

                assert_eq!(regatoi($symbol.as_bytes()), $code);
            }
        )*
    };
}

message_cases! {
    msg_nomatch => (REG_NOMATCH, "regexec() failed to match", "REG_NOMATCH");
    msg_badpat => (REG_BADPAT, "invalid regular expression", "REG_BADPAT");
    msg_ecollate => (REG_ECOLLATE, "invalid collating element", "REG_ECOLLATE");
    msg_ectype => (REG_ECTYPE, "invalid character class", "REG_ECTYPE");
    msg_eescape => (REG_EESCAPE, "trailing backslash (\\)", "REG_EESCAPE");
    msg_esubreg => (REG_ESUBREG, "invalid backreference number", "REG_ESUBREG");
    msg_ebrack => (REG_EBRACK, "brackets ([ ]) not balanced", "REG_EBRACK");
    msg_eparen => (REG_EPAREN, "parentheses not balanced", "REG_EPAREN");
    msg_ebrace => (REG_EBRACE, "braces not balanced", "REG_EBRACE");
    msg_badbr => (REG_BADBR, "invalid repetition count(s)", "REG_BADBR");
    msg_erange => (REG_ERANGE, "invalid character range", "REG_ERANGE");
    msg_espace => (REG_ESPACE, "out of memory", "REG_ESPACE");
    msg_badrpt => (REG_BADRPT, "repetition-operator operand invalid", "REG_BADRPT");
    msg_empty => (REG_EMPTY, "empty (sub)expression", "REG_EMPTY");
    msg_assert => (REG_ASSERT, "\"can't happen\" -- you found a bug", "REG_ASSERT");
    msg_invarg => (REG_INVARG, "invalid argument to regex routine", "REG_INVARG");
    msg_illseq => (REG_ILLSEQ, "illegal byte sequence", "REG_ILLSEQ");
}
