// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the pattern compiler.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::consts::{
    REG_BADBR, REG_BADRPT, REG_EBRACK, REG_ECOLLATE, REG_ECTYPE, REG_EESCAPE, REG_EMPTY,
    REG_EPAREN, REG_ESUBREG, REG_EXTENDED, REG_GNU, REG_ICASE, REG_INVARG, REG_NEWLINE, REG_NOSPEC,
};
use crate::locale::{ByteLocale, Utf8Locale};
use crate::{Regex, regexec};

fn compile(pattern: &[u8], cflags: u32) -> Result<Regex, RegError> {
    regcomp(pattern, cflags, &ByteLocale)
}

fn code(pattern: &[u8], cflags: u32) -> i32 {
    match compile(pattern, cflags) {
        Ok(_) => 0,
        Err(RegError(code)) => code,
    }
}

#[test]
fn group_count_is_recorded() {
    let re = compile(b"(a)(b)(c)", REG_EXTENDED).expect("compile");
    assert_eq!(re.guts.nsub, 3);
    let re = compile(b"a\\(b\\)\\(c\\)", 0).expect("compile");
    assert_eq!(re.guts.nsub, 2);
}

#[test]
fn basic_syntax_errors() {
    assert_eq!(code(b"(", REG_EXTENDED), REG_EPAREN);
    assert_eq!(code(b"a)", REG_EXTENDED), 0);
    assert_eq!(code(b"a\\)", 0), REG_EPAREN);
    assert_eq!(code(b"*", REG_EXTENDED), REG_BADRPT);
    assert_eq!(code(b"a**", REG_EXTENDED), REG_BADRPT);
    assert_eq!(code(b"a\\", REG_EXTENDED), REG_EESCAPE);
    assert_eq!(code(b"a[b", REG_EXTENDED), REG_EBRACK);
    assert_eq!(code(b"a||b", REG_EXTENDED), REG_EMPTY);
    assert_eq!(code(b"\\1", 0), REG_ESUBREG);
}

#[test]
fn bracket_class_and_collation_errors() {
    assert_eq!(code(b"[[:notdef:]]", REG_EXTENDED), REG_ECTYPE);
    assert_eq!(code(b"[[.notdef.]]", REG_EXTENDED), REG_ECOLLATE);
    assert_eq!(code(b"[[:alpha:]]", REG_EXTENDED), 0);
    assert_eq!(code(b"[[.NUL.]]", REG_EXTENDED), 0);
    assert_eq!(code(b"[[=a=]]", REG_EXTENDED), 0);
}

#[test]
fn dup_max_is_255() {
    assert_eq!(code(b"a{255}", REG_EXTENDED), 0);
    assert_eq!(code(b"a{256}", REG_EXTENDED), REG_BADBR);
    assert_eq!(code(b"a{2,1}", REG_EXTENDED), REG_BADBR);
    assert_eq!(code(b"a{2,3}", REG_EXTENDED), 0);
    assert_eq!(code(b"a{2,}", REG_EXTENDED), 0);
}

#[test]
fn nospac_and_extended_conflict() {
    assert_eq!(code(b"a", REG_EXTENDED | REG_NOSPEC), REG_INVARG);
    let re = compile(b"a*b", REG_NOSPEC).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&re, b"xa*b", &mut slots, 0, &ByteLocale).expect("literal");
    assert_eq!(slots[0].rm_so, 1);
    assert_eq!(slots[0].rm_eo, 4);
}

#[test]
fn nspan_empty_pattern_is_an_error() {
    assert_eq!(code(b"", REG_EXTENDED), REG_EMPTY);
    assert_eq!(code(b"", 0), REG_EMPTY);
    assert_eq!(code(b"", REG_NOSPEC), REG_EMPTY);
}

#[test]
fn newline_anchors_and_dot() {
    let re = compile(b"^b$", REG_EXTENDED | REG_NEWLINE).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&re, b"a\nb\nc", &mut slots, 0, &ByteLocale).expect("match");
    assert_eq!(slots[0].rm_so, 2);
    assert_eq!(slots[0].rm_eo, 3);

    let dot = compile(b"a.b", REG_EXTENDED | REG_NEWLINE).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    assert_eq!(
        regexec(&dot, b"a\nb", &mut slots, 0, &ByteLocale),
        Err(RegError(crate::consts::REG_NOMATCH))
    );
}

#[test]
fn gnu_escapes_are_off_by_default() {
    assert_eq!(code(b"\\w", REG_EXTENDED), REG_EESCAPE);
    assert_eq!(code(b"\\b", 0), REG_EESCAPE);

    let gnu = compile(b"\\w", REG_EXTENDED | REG_GNU).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&gnu, b"a", &mut slots, 0, &ByteLocale).expect("word class");
    assert_eq!(
        regexec(&gnu, b"-", &mut slots, 0, &ByteLocale),
        Err(RegError(crate::consts::REG_NOMATCH))
    );

    let bre_gnu = compile(b"\\b", REG_GNU).expect("compile");
    regexec(&bre_gnu, b"x", &mut slots, 0, &ByteLocale).expect("boundary");
}

#[test]
fn angle_anchors_work_without_gnu() {
    for pattern in [&b"\\<a"[..], &b"[[:<:]]a"[..]] {
        let re = compile(pattern, REG_EXTENDED).expect("compile");
        let mut slots = [crate::RegMatch::default(); 1];
        regexec(&re, b"ba", &mut slots, 0, &ByteLocale).expect_err("not at a word start");
        regexec(&re, b"a b", &mut slots, 0, &ByteLocale).expect("word start");
    }
}

#[test]
fn ere_backreference_needs_gnu() {
    let plain = compile(b"(a)\\1", REG_EXTENDED).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&plain, b"a1", &mut slots, 0, &ByteLocale).expect("escaped 1");
    let gnu = compile(b"(a)\\1", REG_EXTENDED | REG_GNU).expect("compile");
    regexec(&gnu, b"aa", &mut slots, 0, &ByteLocale).expect("backref");
}

#[test]
fn nsub_limit_ten_groups() {
    let groups = b"(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)";
    let re = compile(groups, REG_EXTENDED).expect("twelve groups compile");
    assert_eq!(re.guts.nsub, 12);

    let nine =
        compile(b"(a)(b)(c)(d)(e)(f)(g)(h)(i)\\9", REG_EXTENDED | REG_GNU).expect("group nine");
    let mut slots = [crate::RegMatch::default(); 10];
    regexec(&nine, b"abcdefghii", &mut slots, 0, &ByteLocale).expect("\\9 repeats i");

    let ten = compile(
        b"(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)\\10",
        REG_EXTENDED | REG_GNU,
    )
    .expect("\\10 is \\1 and a zero");
    let mut slots = [crate::RegMatch::default(); 11];
    regexec(&ten, b"abcdefghija0", &mut slots, 0, &ByteLocale).expect("\\10 literal zero");
}

#[test]
fn nested_group_depth_boundary() {
    let mut pattern = vec![b'('; 256];
    pattern.push(b'a');
    pattern.extend(std::iter::repeat_n(b')', 256));
    let re = compile(&pattern, REG_EXTENDED).expect("256 deep compiles");
    assert_eq!(re.guts.nsub, 256);
    pattern.insert(0, b'(');
    pattern.push(b')');
    assert_eq!(code(&pattern, REG_EXTENDED), crate::consts::REG_ESPACE);
}

#[test]
fn repeat_duplicates_the_strip() {
    let re = compile(b"(a{2}){3}", REG_EXTENDED).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&re, b"aaaaaa", &mut slots, 0, &ByteLocale).expect("six");
    assert_eq!(
        regexec(&re, b"aaaaa", &mut slots, 0, &ByteLocale),
        Err(RegError(crate::consts::REG_NOMATCH))
    );
}

#[test]
fn must_string_is_the_longest_literal() {
    let re = compile(b"abcdef", REG_EXTENDED).expect("compile");
    assert_eq!(re.guts.must.as_deref(), Some(&b"abcdef"[..]));
    assert_eq!(re.guts.mlen, 6);
    let re = compile(b"abc.*def", REG_EXTENDED).expect("compile");
    assert_eq!(re.guts.mlen, 3);
    assert_eq!(re.guts.must.as_deref(), Some(&b"abc"[..]));
}

#[test]
fn icase_matches_non_ascii_in_utf8() {
    let re = regcomp("\u{c9}".as_bytes(), REG_EXTENDED | REG_ICASE, &Utf8Locale).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&re, "\u{e9}".as_bytes(), &mut slots, 0, &Utf8Locale).expect("case pair");
}

#[test]
fn invalid_utf8_pattern_is_illseq() {
    let code = match regcomp(b"\xff", REG_EXTENDED, &Utf8Locale) {
        Ok(_) => 0,
        Err(RegError(code)) => code,
    };
    assert_eq!(code, crate::consts::REG_ILLSEQ);
}

#[test]
fn embedded_nul_without_pend_is_the_prefix() {
    // The shell passes the bytes before the NUL when REG_PEND is absent; the slice API makes
    // that explicit, so a slice containing NUL is the REG_PEND form.
    let re = compile(b"a\0b", REG_EXTENDED).expect("compile");
    let mut slots = [crate::RegMatch::default(); 1];
    regexec(&re, b"a\0b", &mut slots, 0, &ByteLocale).expect("NUL matches");
    assert_eq!(slots[0].rm_eo, 3);
}

#[test]
fn singleton_brackets_become_characters() {
    let one = compile(b"[a]", REG_EXTENDED).expect("compile");
    let plain = compile(b"a", REG_EXTENDED).expect("compile");
    assert_eq!(one.guts.nstates, plain.guts.nstates);
}

macro_rules! pattern_cases {
    ($($name:ident => ($pattern:expr, $flags:expr, $expected:expr);)*) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(code($pattern, $flags), $expected);
            }
        )*
    };
}

pattern_cases! {
    pc_ere_literal => (b"a", REG_EXTENDED, 0);
    pc_bre_literal => (b"a", 0, 0);
    pc_ere_group => (b"(a)", REG_EXTENDED, 0);
    pc_bre_group => (b"\\(a\\)", 0, 0);
    pc_ere_unmatched_open => (b"(", REG_EXTENDED, REG_EPAREN);
    pc_bre_unmatched_open => (b"\\(", 0, REG_EPAREN);
    pc_ere_unmatched_close => (b"a)", REG_EXTENDED, 0);
    pc_bre_unmatched_close => (b"a\\)", 0, REG_EPAREN);
    pc_ere_star_first => (b"*", REG_EXTENDED, REG_BADRPT);
    pc_bre_star_first => (b"*", 0, 0);
    pc_ere_plus_first => (b"+", REG_EXTENDED, REG_BADRPT);
    pc_ere_question_first => (b"?", REG_EXTENDED, REG_BADRPT);
    pc_ere_brace_first => (b"{1}", REG_EXTENDED, REG_BADRPT);
    pc_ere_double_star => (b"a**", REG_EXTENDED, REG_BADRPT);
    pc_ere_double_plus => (b"a++", REG_EXTENDED, REG_BADRPT);
    pc_ere_double_question => (b"a??", REG_EXTENDED, REG_BADRPT);
    pc_ere_star_plus => (b"a*+", REG_EXTENDED, REG_BADRPT);
    pc_ere_star_question => (b"a*?", REG_EXTENDED, REG_BADRPT);
    pc_ere_brace_star => (b"a{1}*", REG_EXTENDED, REG_BADRPT);
    pc_ere_caret_star => (b"^*", REG_EXTENDED, REG_BADRPT);
    pc_ere_empty_choice => (b"|", REG_EXTENDED, REG_EMPTY);
    pc_ere_double_choice => (b"a||b", REG_EXTENDED, REG_EMPTY);
    pc_ere_trailing_choice => (b"ab|", REG_EXTENDED, REG_EMPTY);
    pc_ere_leading_choice => (b"|ab", REG_EXTENDED, REG_EMPTY);
    pc_ere_trailing_backslash => (b"a\\", REG_EXTENDED, REG_EESCAPE);
    pc_ere_bracket_unclosed => (b"a[b", REG_EXTENDED, REG_EBRACK);
    pc_ere_class_unclosed => (b"a[[:", REG_EXTENDED, REG_EBRACK);
    pc_ere_class_unknown => (b"[[:notdef:]]", REG_EXTENDED, REG_ECTYPE);
    pc_ere_collation_unknown => (b"[[.notdef.]]", REG_EXTENDED, REG_ECOLLATE);
    pc_ere_range_reverse => (b"[3-1]", REG_EXTENDED, REG_ERANGE);
    pc_ere_range_alpha => (b"[a-z]", REG_EXTENDED, 0);
    pc_ere_dup_255 => (b"a{255}", REG_EXTENDED, 0);
    pc_ere_dup_256 => (b"a{256}", REG_EXTENDED, REG_BADBR);
    pc_ere_dup_reverse => (b"a{2,1}", REG_EXTENDED, REG_BADBR);
    pc_ere_dup_open => (b"a{2,", REG_EXTENDED, REG_EBRACE);
    pc_ere_dup_bad => (b"a{2,x}", REG_EXTENDED, REG_BADBR);
    pc_ere_collation_one => (b"[[.one.]]", REG_EXTENDED, 0);
    pc_ere_equivalence => (b"[[=a=]]", REG_EXTENDED, 0);
    pc_ere_collation_nul => (b"[[.NUL.]]", REG_EXTENDED, 0);
    pc_ere_embedded_nul => (b"a\0b", REG_EXTENDED, 0);
    pc_ere_gnu_word => (b"\\w", REG_EXTENDED | REG_GNU, 0);
    pc_ere_gnu_word_off => (b"\\w", REG_EXTENDED, REG_EESCAPE);
    pc_bre_gnu_word_off => (b"\\w", 0, REG_EESCAPE);
    pc_ere_angle_start => (b"\\<a", REG_EXTENDED, 0);
    pc_ere_angle_end => (b"a\\>", REG_EXTENDED, 0);
    pc_ere_angle_brackets => (b"[[:<:]]a[[:>:]]", REG_EXTENDED, 0);
    pc_ere_gnu_backref => (b"(a)\\1", REG_EXTENDED | REG_GNU, 0);
    pc_ere_backref_off => (b"(a)\\1", REG_EXTENDED, 0);
    pc_bre_backref_missing => (b"\\1", 0, REG_ESUBREG);
    pc_ere_backref_nine => (b"(a)(b)(c)(d)(e)(f)(g)(h)(i)\\9", REG_EXTENDED | REG_GNU, 0);
    pc_ere_backref_ten => (b"(a)\\10", REG_EXTENDED | REG_GNU, 0);
    pc_ere_empty => (b"", REG_EXTENDED, REG_EMPTY);
    pc_nospec_empty => (b"", REG_NOSPEC, REG_EMPTY);
    pc_nospec_meta => (b"a*b", REG_NOSPEC, 0);
    pc_nospec_and_extended => (b"a", REG_EXTENDED | REG_NOSPEC, REG_INVARG);
    pc_ere_newline => (b".", REG_EXTENDED | REG_NEWLINE, 0);
    pc_ere_icase => (b"a", REG_EXTENDED | REG_ICASE, 0);
    pc_byte_icase_high => (b"\xe9", REG_EXTENDED | REG_ICASE, 0);
    pc_class_alpha => (b"[[:alpha:]]", REG_EXTENDED, 0);
    pc_class_alnum => (b"[[:alnum:]]", REG_EXTENDED, 0);
    pc_class_blank => (b"[[:blank:]]", REG_EXTENDED, 0);
    pc_class_cntrl => (b"[[:cntrl:]]", REG_EXTENDED, 0);
    pc_class_digit => (b"[[:digit:]]", REG_EXTENDED, 0);
    pc_class_graph => (b"[[:graph:]]", REG_EXTENDED, 0);
    pc_class_lower => (b"[[:lower:]]", REG_EXTENDED, 0);
    pc_class_print => (b"[[:print:]]", REG_EXTENDED, 0);
    pc_class_punct => (b"[[:punct:]]", REG_EXTENDED, 0);
    pc_class_space => (b"[[:space:]]", REG_EXTENDED, 0);
    pc_class_upper => (b"[[:upper:]]", REG_EXTENDED, 0);
    pc_class_xdigit => (b"[[:xdigit:]]", REG_EXTENDED, 0);
    pc_bracket_empty => (b"a[]", REG_EXTENDED, REG_EBRACK);
    pc_bracket_dash => (b"a[-b]c", REG_EXTENDED, 0);
    pc_bracket_inverted => (b"a[^]b]c", REG_EXTENDED, 0);
    pc_bracket_collation_bracket => (b"a[[.].]]b", REG_EXTENDED, 0);
    pc_bre_brace => (b"a\\{2\\}", 0, 0);
    pc_bre_brace_unbalanced => (b"a\\{2", 0, REG_EBRACE);
    pc_bre_brace_bad => (b"a\\{2,x\\}", 0, REG_BADBR);
    pc_bre_empty_group => (b"a\\(\\)b", 0, 0);
    pc_bre_dollar => (b"a$", 0, 0);
    pc_bre_caret => (b"^a", 0, 0);
    pc_bre_choice_literal => (b"a|b", 0, 0);
    pc_bre_plus_literal => (b"a+b", 0, 0);
    pc_bre_question_literal => (b"a?b", 0, 0);
    pc_bre_dot => (b"a.b", 0, 0);
    pc_bre_bracket => (b"a[bc]d", 0, 0);
    pc_bre_escaped_star => (b"a\\*c", 0, 0);
    pc_bre_escaped_backslash => (b"a\\\\b", 0, 0);
    pc_gnu_space_class => (b"\\s", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_nonword => (b"\\W", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_boundary => (b"\\b", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_nonboundary => (b"\\B", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_buffer_start => (b"\\`a", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_buffer_end => (b"a\\'", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_escape_n => (b"\\n", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_escape_t => (b"\\t", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_escape_a => (b"\\a", REG_EXTENDED | REG_GNU, 0);
    pc_gnu_escape_e => (b"\\e", REG_EXTENDED | REG_GNU, 0);
    pc_repeat_multi => (b"((a{1,10}){1,10}){1,10}", REG_EXTENDED, 0);
    pc_repeat_zero => (b"ab{0,0}c", REG_EXTENDED, 0);
    pc_ere_zero_repeat => (b"a{0}", REG_EXTENDED, REG_EMPTY);
    pc_ere_zero_repeat_group => (b"(a){0}", REG_EXTENDED, REG_EMPTY);
}
