// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tokenizer tests.

use super::*;

fn words(input: &str) -> Vec<String> {
    tokenize(input)
        .expect("tokenize")
        .into_iter()
        .filter_map(|spanned| match spanned.token {
            Token::Word(word) => Some(word),
            _ => None,
        })
        .collect()
}

#[test]
fn newlines_are_significant_and_comments_are_dropped() {
    let tokens = tokenize("pass in all # comment\nblock out all\n").expect("tokenize");
    let kinds: Vec<Token> = tokens.into_iter().map(|spanned| spanned.token).collect();
    assert_eq!(
        kinds,
        vec![
            Token::Word(String::from("pass")),
            Token::Word(String::from("in")),
            Token::Word(String::from("all")),
            Token::Newline,
            Token::Word(String::from("block")),
            Token::Word(String::from("out")),
            Token::Word(String::from("all")),
            Token::Newline,
            Token::Eof,
        ]
    );
}

#[test]
fn strings_keep_spaces_and_escapes() {
    let tokens = tokenize(r#"set skip on "em 0" "#).expect("tokenize");
    assert_eq!(tokens[3].token, Token::Str(String::from("em 0")));
    let tokens = tokenize(r#""a\"b""#).expect("tokenize");
    assert_eq!(tokens[0].token, Token::Str(String::from("a\"b")));
}

#[test]
fn unterminated_string_reports_its_line() {
    let error = tokenize("pass in all\nset loginterface \"em0\n").expect_err("must fail");
    assert_eq!(error.line, 2);
    assert_eq!(error.message, "unterminated string");
}

#[test]
fn line_numbers_increment_across_statements() {
    let tokens = tokenize("pass in all\n\nblock out all\n").expect("tokenize");
    let lines: Vec<u32> = tokens.iter().map(|spanned| spanned.line).collect();
    assert_eq!(lines, vec![1, 1, 1, 1, 2, 3, 3, 3, 3, 4]);
}

#[test]
fn addresses_port_ranges_and_macros_arrive_whole() {
    assert_eq!(
        words("from 10.0.0.1/8 port 1000:2000 on $ext_if"),
        vec![
            String::from("from"),
            String::from("10.0.0.1/8"),
            String::from("port"),
            String::from("1000:2000"),
            String::from("on"),
            String::from("$ext_if"),
        ]
    );
}

#[test]
fn port_operators_are_typed() {
    let tokens = tokenize("port != 22 port < 1024 port >= 49152").expect("tokenize");
    let operators: Vec<Token> = tokens
        .into_iter()
        .filter_map(|spanned| match spanned.token {
            Token::Ne | Token::Lt | Token::Ge => Some(spanned.token),
            _ => None,
        })
        .collect();
    assert_eq!(operators, vec![Token::Ne, Token::Lt, Token::Ge]);
}
