// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the dispatcher.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::consts::{
    REG_EXTENDED, REG_ICASE, REG_INVARG, REG_LARGE, REG_NEWLINE, REG_NOMATCH, REG_STARTEND,
};
use crate::locale::ByteLocale;
use crate::regcomp;

fn compiled(pattern: &[u8]) -> Regex {
    regcomp(pattern, REG_EXTENDED, &ByteLocale).expect("compile")
}

fn run(re: &Regex, subject: &[u8]) -> Result<Vec<RegMatch>, RegError> {
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 4];
    regexec(re, subject, &mut slots, 0, &ByteLocale).map(|()| slots.to_vec())
}

#[test]
fn a_zeroed_regex_is_invarg() {
    let zeroed = Regex::zeroed_for_test();
    let mut slots = [RegMatch::default(); 1];
    assert_eq!(
        regexec(&zeroed, b"a", &mut slots, 0, &ByteLocale),
        Err(RegError(REG_INVARG))
    );
}

#[test]
fn simple_match_and_no_match() {
    let re = compiled(b"b+");
    let slots = run(&re, b"abbbc").expect("match");
    assert_eq!(slots[0].rm_so, 1);
    assert_eq!(slots[0].rm_eo, 4);
    assert_eq!(run(&re, b"ac"), Err(RegError(REG_NOMATCH)));
}

#[test]
fn startend_offsets_are_relative_to_the_string() {
    let re = compiled(b"b+");
    let mut slots = [RegMatch { rm_so: 1, rm_eo: 6 }; 2];
    regexec(&re, b"xabbbby", &mut slots, REG_STARTEND, &ByteLocale).expect("match");
    assert_eq!(slots[0].rm_so, 2);
    assert_eq!(slots[0].rm_eo, 6);
}

#[test]
fn startend_window_past_the_end_is_invarg() {
    let re = compiled(b"a");
    let mut slots = [RegMatch {
        rm_so: 0,
        rm_eo: 100,
    }; 1];
    assert_eq!(
        regexec(&re, b"abc", &mut slots, REG_STARTEND, &ByteLocale),
        Err(RegError(REG_INVARG))
    );
}

#[test]
fn large_selector_returns_the_same_match() {
    let re = compiled(b"a(b+)c");
    let small = run(&re, b"xabbc").expect("small");
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 4];
    regexec(&re, b"xabbc", &mut slots, REG_LARGE, &ByteLocale).expect("large");
    assert_eq!(&slots[..2], &small[..2]);
}

#[test]
fn backr_forces_the_backref_path_without_backrefs() {
    let re = compiled(b"(a)(b)?");
    let mut slots = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 3];
    regexec(&re, b"a", &mut slots, crate::consts::REG_BACKR, &ByteLocale).expect("match");
    assert_eq!((slots[0].rm_so, slots[0].rm_eo), (0, 1));
    assert_eq!((slots[1].rm_so, slots[1].rm_eo), (0, 1));
    assert_eq!(slots[2].rm_so, -1);
}

#[test]
fn trace_is_accepted_and_ignored() {
    let re = compiled(b"a+");
    let mut with = [RegMatch::default(); 1];
    let mut without = [RegMatch::default(); 1];
    regexec(
        &re,
        b"baa",
        &mut with,
        crate::consts::REG_TRACE,
        &ByteLocale,
    )
    .expect("trace");
    regexec(&re, b"baa", &mut without, 0, &ByteLocale).expect("plain");
    assert_eq!(
        (with[0].rm_so, with[0].rm_eo),
        (without[0].rm_so, without[0].rm_eo)
    );
}

#[test]
fn nosub_never_writes_pmatch() {
    let re = crate::regcomp(b"a+", REG_EXTENDED | crate::consts::REG_NOSUB, &ByteLocale)
        .expect("compile");
    let mut slots = [RegMatch {
        rm_so: 77,
        rm_eo: 88,
    }; 2];
    regexec(&re, b"aaa", &mut slots, 0, &ByteLocale).expect("match");
    assert_eq!(slots[0].rm_so, 77);
    assert_eq!(slots[0].rm_eo, 88);
}

#[test]
fn empty_pmatch_is_the_nosub_path() {
    let re = compiled(b"a+");
    let mut slots: [RegMatch; 0] = [];
    regexec(&re, b"aaa", &mut slots, 0, &ByteLocale).expect("match");
}

#[test]
fn unused_groups_are_minus_one() {
    let re = compiled(b"(a)(b)?");
    let slots = run(&re, b"a").expect("match");
    assert_eq!(slots[0].rm_so, 0);
    assert_eq!(slots[0].rm_eo, 1);
    assert_eq!(slots[1].rm_so, 0);
    assert_eq!(slots[1].rm_eo, 1);
    assert_eq!(slots[2].rm_so, -1);
    assert_eq!(slots[2].rm_eo, -1);
}

#[test]
fn dispatch_boundary_is_sixty_four_states() {
    // 63 literal characters plus two OEND words: 64 states or fewer -> smatcher.
    let mut pattern = vec![b'a'; 62];
    let re = regcomp(&pattern, REG_EXTENDED, &ByteLocale).expect("compile");
    assert!(re.guts.nstates <= 64);
    let mut slots = [RegMatch::default(); 1];
    regexec(&re, &[b'a'; 62], &mut slots, 0, &ByteLocale).expect("small");
    pattern.push(b'a');
    let re = regcomp(&pattern, REG_EXTENDED, &ByteLocale).expect("compile");
    assert!(re.guts.nstates > 64);
    regexec(&re, &pattern, &mut slots, 0, &ByteLocale).expect("large");
}

macro_rules! match_cases {
    ($($name:ident => ($pattern:expr, $flags:expr, $subject:expr, $expected:expr);)*) => {
        $(
            #[test]
            fn $name() {
                let re = regcomp($pattern, $flags, &ByteLocale)
                    .unwrap_or_else(|error| panic!("regcomp {}", error.0));
                let mut slots = [RegMatch { rm_so: -1, rm_eo: -1 }; 1];
                match $expected {
                    Some((start, end)) => {
                        regexec(&re, $subject, &mut slots, 0, &ByteLocale)
                            .unwrap_or_else(|error| panic!("exec {}", error.0));
                        assert_eq!((slots[0].rm_so, slots[0].rm_eo), (start, end));
                    }
                    None => {
                        let result = regexec(&re, $subject, &mut slots, 0, &ByteLocale);
                        let code = match result {
                            Ok(()) => 0,
                            Err(error) => error.0,
                        };
                        assert!(code == REG_NOMATCH, "expected no match, got {code}");
                    }
                }
            }
        )*
    };
}

match_cases! {
    mc_literal_a => (b"a", REG_EXTENDED, b"abc", Some((0, 1)));
    mc_literal_b => (b"b", REG_EXTENDED, b"abc", Some((1, 2)));
    mc_literal_c => (b"c", REG_EXTENDED, b"abc", Some((2, 3)));
    mc_literal_absent => (b"z", REG_EXTENDED, b"abc", None);
    mc_plus => (b"a+", REG_EXTENDED, b"baaa", Some((1, 4)));
    mc_star_longest => (b"a*", REG_EXTENDED, b"aaa", Some((0, 3)));
    mc_star_leftmost_empty => (b"a*", REG_EXTENDED, b"baaa", Some((0, 0)));
    mc_star_empty => (b"a*", REG_EXTENDED, b"bbb", Some((0, 0)));
    mc_caret_miss => (b"^a", REG_EXTENDED, b"ba", None);
    mc_caret_hit => (b"^a", REG_EXTENDED, b"ab", Some((0, 1)));
    mc_dollar_hit => (b"a$", REG_EXTENDED, b"ba", Some((1, 2)));
    mc_dollar_miss => (b"a$", REG_EXTENDED, b"ab", None);
    mc_both_anchors_empty => (b"^$", REG_EXTENDED, b"", Some((0, 0)));
    mc_both_anchors_miss => (b"^$", REG_EXTENDED, b"a", None);
    mc_dot_one => (b".", REG_EXTENDED, b"abc", Some((0, 1)));
    mc_dot_star => (b".*", REG_EXTENDED, b"abc", Some((0, 3)));
    mc_dot_plus => (b".+", REG_EXTENDED, b"abc", Some((0, 3)));
    mc_class_plus => (b"[abc]+", REG_EXTENDED, b"xabcx", Some((1, 4)));
    mc_inverted_class => (b"[^a]+", REG_EXTENDED, b"aaabbb", Some((3, 6)));
    mc_range_class => (b"[a-c]+", REG_EXTENDED, b"xxabc", Some((2, 5)));
    mc_group_concat => (b"(a)(b)", REG_EXTENDED, b"ab", Some((0, 2)));
    mc_choice_plus => (b"(a|b)+", REG_EXTENDED, b"abba", Some((0, 4)));
    mc_choice_leftmost => (b"a|b", REG_EXTENDED, b"cba", Some((1, 2)));
    mc_choice_longest => (b"ab|a", REG_EXTENDED, b"xabc", Some((1, 3)));
    mc_choice_short => (b"ab|a", REG_EXTENDED, b"xac", Some((1, 2)));
    mc_dup_two => (b"a{2}", REG_EXTENDED, b"aaa", Some((0, 2)));
    mc_dup_range => (b"a{2,3}", REG_EXTENDED, b"aaaa", Some((0, 3)));
    mc_dup_open => (b"a{2,}", REG_EXTENDED, b"aaaa", Some((0, 4)));
    mc_dup_zero => (b"a{0,2}", REG_EXTENDED, b"b", Some((0, 0)));
    mc_group_plus => (b"(ab)+", REG_EXTENDED, b"ababx", Some((0, 4)));
    mc_nested_star => (b"(a*)*", REG_EXTENDED, b"aaa", Some((0, 3)));
    mc_choice_repeat => (b"(a|aa)+", REG_EXTENDED, b"aaa", Some((0, 3)));
    mc_anchored_star => (b"^.*$", REG_EXTENDED, b"abc", Some((0, 3)));
    mc_anchored_star_empty => (b"^.*$", REG_EXTENDED, b"", Some((0, 0)));
    mc_star_b => (b"b*", REG_EXTENDED, b"abc", Some((0, 0)));
    mc_plus_b => (b"b+", REG_EXTENDED, b"abc", Some((1, 2)));
    mc_question => (b"x?", REG_EXTENDED, b"abc", Some((0, 0)));
    mc_escaped_dot => (b"\\.", REG_EXTENDED, b"a.b", Some((1, 2)));
    mc_escaped_dot_seq => (b"a\\.b", REG_EXTENDED, b"a.b", Some((0, 3)));
    mc_dot_in_class => (b"[.]", REG_EXTENDED, b"a.b", Some((1, 2)));
    mc_inverted_dot => (b"[^.]", REG_EXTENDED, b"a.b", Some((0, 1)));
    mc_digit_class => (b"[[:digit:]]+", REG_EXTENDED, b"ab123cd", Some((2, 5)));
    mc_alpha_class => (b"[[:alpha:]]+", REG_EXTENDED, b"12ab34", Some((2, 4)));
    mc_space_class => (b"[[:space:]]+", REG_EXTENDED, b"a \t b", Some((1, 4)));
    mc_upper_class => (b"[[:upper:]]+", REG_EXTENDED, b"aBCd", Some((1, 3)));
    mc_lower_class => (b"[[:lower:]]+", REG_EXTENDED, b"aBCd", Some((0, 1)));
    mc_angle_start_miss => (b"\\<a", REG_EXTENDED, b"ba", None);
    mc_angle_start_hit => (b"\\<a", REG_EXTENDED, b"a b", Some((0, 1)));
    mc_angle_end_miss => (b"a\\>", REG_EXTENDED, b"ab", None);
    mc_angle_end_hit => (b"a\\>", REG_EXTENDED, b"a b", Some((0, 1)));
    mc_bow_bracket => (b"[[:<:]]b", REG_EXTENDED, b"a b", Some((2, 3)));
    mc_eow_bracket => (b"b[[:>:]]", REG_EXTENDED, b"b c", Some((0, 1)));
    mc_newline_caret => (b"^b", REG_EXTENDED | REG_NEWLINE, b"a\nb\nc", Some((2, 3)));
    mc_no_newline_caret => (b"^b", REG_EXTENDED, b"a\nb", None);
    mc_newline_dollar => (b"a$", REG_EXTENDED | REG_NEWLINE, b"a\nb", Some((0, 1)));
    mc_newline_dot => (b"^.$", REG_EXTENDED | REG_NEWLINE, b"a\nb", Some((0, 1)));
    mc_newline_class => (b"[^a]", REG_EXTENDED | REG_NEWLINE, b"\n", None);
    mc_icase_lower => (b"a", REG_EXTENDED | REG_ICASE, b"Abc", Some((0, 1)));
    mc_icase_upper => (b"A", REG_EXTENDED | REG_ICASE, b"abc", Some((0, 1)));
    mc_icase_class => (b"[a-z]+", REG_EXTENDED | REG_ICASE, b"ABC", Some((0, 3)));
    mc_icase_high_byte => (b"\xe9", REG_EXTENDED | REG_ICASE, b"\xc9", Some((0, 1)));
    mc_greedy_star => (b"a.*b", REG_EXTENDED, b"axxbxxb", Some((0, 7)));
    mc_group_plus_plus => (b"(a+)(b+)", REG_EXTENDED, b"aaabbb", Some((0, 6)));
    mc_double_group => (b"((a)(b))", REG_EXTENDED, b"ab", Some((0, 2)));
    mc_triple_group => (b"(a(b)c)", REG_EXTENDED, b"abc", Some((0, 3)));
    mc_alt_group => (b"(a|ab)(c|bcd)", REG_EXTENDED, b"abcd", Some((0, 4)));
    mc_star_empty_subject => (b"x*", REG_EXTENDED, b"", Some((0, 0)));
    mc_caret_only => (b"^", REG_EXTENDED, b"abc", Some((0, 0)));
    mc_dollar_only => (b"$", REG_EXTENDED, b"abc", Some((3, 3)));
    mc_caret_middle => (b"a^b", REG_EXTENDED, b"a^b", None);
    mc_dollar_end => (b"a$", REG_EXTENDED, b"a", Some((0, 1)));
    mc_empty_group => (b"()", REG_EXTENDED, b"abc", Some((0, 0)));
    mc_optional_group_miss => (b"(a)?", REG_EXTENDED, b"b", Some((0, 0)));
    mc_optional_group_hit => (b"(a)?", REG_EXTENDED, b"a", Some((0, 1)));
    mc_group_star_alt => (b"a(b|c)*d", REG_EXTENDED, b"ad", Some((0, 2)));
    mc_group_star_alt_hit => (b"a(b|c)*d", REG_EXTENDED, b"abcd", Some((0, 4)));
    mc_inverted_b => (b"[^b]", REG_EXTENDED, b"abc", Some((0, 1)));
    mc_inverted_b_miss => (b"[^b]", REG_EXTENDED, b"b", None);
    mc_trailing_dash => (b"[a-]", REG_EXTENDED, b"-", Some((0, 1)));
    mc_leading_dash => (b"[-a]", REG_EXTENDED, b"-", Some((0, 1)));
    mc_bracket_first_close => (b"[]a]", REG_EXTENDED, b"]", Some((0, 1)));
    mc_inverted_first_close => (b"[^]a]", REG_EXTENDED, b"b", Some((0, 1)));
    mc_inverted_first_close_miss => (b"[^]a]", REG_EXTENDED, b"a", None);
    mc_two_repeat_group => (b"(a){2}", REG_EXTENDED, b"aa", Some((0, 2)));
    mc_open_repeat_group => (b"(a){1,}", REG_EXTENDED, b"aaa", Some((0, 3)));
    mc_bounded_group => (b"(ab){1,2}", REG_EXTENDED, b"abab", Some((0, 4)));
    mc_bounded_group_max => (b"(ab){1,2}", REG_EXTENDED, b"ababab", Some((0, 4)));
    mc_choice_three => (b"(a|b|c)+", REG_EXTENDED, b"cba", Some((0, 3)));
    mc_choice_three_empty => (b"(a|b|c)*", REG_EXTENDED, b"xyz", Some((0, 0)));
    mc_two_stars => (b"(a*)(b*)", REG_EXTENDED, b"aabb", Some((0, 4)));
    mc_two_stars_b => (b"(a*)(b*)", REG_EXTENDED, b"bb", Some((0, 2)));
    mc_two_stars_a => (b"(a*)(a*)", REG_EXTENDED, b"aa", Some((0, 2)));
    mc_bracket_seq => (b"a[bc]d", REG_EXTENDED, b"abd", Some((0, 3)));
    mc_bracket_seq_miss => (b"a[bc]d", REG_EXTENDED, b"aXd", None);
    mc_dot_newline_default => (b"a.b", REG_EXTENDED, b"a\nb", Some((0, 3)));
}
