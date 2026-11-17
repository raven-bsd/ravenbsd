// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the bounded `localedef` parser.

use super::*;

#[test]
fn parses_a_numeric_category() {
    let source = "LC_NUMERIC\ndecimal_point \".\"\nthousands_sep \",\"\nEND LC_NUMERIC\n";
    let sets = parse_locale(source).expect("parse");
    let set = sets.get(&LC_NUMERIC).expect("numeric set");
    assert_eq!(set.get(&1).expect("decimal_point"), b".");
    assert_eq!(set.get(&2).expect("thousands_sep"), b",");
}

#[test]
fn parses_copy_and_semicolon_lists() {
    let source =
        "LC_COLLATE\ncopy \"C\"\nEND LC_COLLATE\nLC_TIME\nabday \"Sun\";\"Mon\"\nEND LC_TIME\n";
    let sets = parse_locale(source).expect("parse");
    assert_eq!(
        sets.get(&LC_COLLATE)
            .expect("collate")
            .get(&COPY_ID)
            .expect("copy"),
        b"C"
    );
    assert_eq!(
        sets.get(&LC_TIME).expect("time").get(&1).expect("abday"),
        b"Sun;Mon"
    );
}

#[test]
fn unknown_category_and_keyword_are_rejected() {
    assert_eq!(
        parse_locale("LC_FOO\nx \"y\"\nEND LC_FOO\n"),
        Err(LocaledefError::Unsupported("line 1: LC_FOO".to_string()))
    );
    assert_eq!(
        parse_locale("LC_NUMERIC\nnope \"y\"\nEND LC_NUMERIC\n"),
        Err(LocaledefError::Unsupported("line 2: nope".to_string()))
    );
}

#[test]
fn missing_end_and_nested_categories_are_rejected() {
    assert!(matches!(
        parse_locale("LC_NUMERIC\ndecimal_point \".\"\n"),
        Err(LocaledefError::Malformed(_))
    ));
    assert!(matches!(
        parse_locale("LC_NUMERIC\nLC_TIME\nEND LC_TIME\nEND LC_NUMERIC\n"),
        Err(LocaledefError::Malformed(_))
    ));
}

#[test]
fn malformed_values_are_rejected() {
    assert!(matches!(
        parse_locale("LC_NUMERIC\ndecimal_point \".\nEND LC_NUMERIC\n"),
        Err(LocaledefError::Malformed(_))
    ));
    assert!(matches!(
        parse_locale("LC_NUMERIC\ndecimal_point .\nEND LC_NUMERIC\n"),
        Err(LocaledefError::Malformed(_))
    ));
}

#[test]
fn charmap_subset_is_parsed() {
    let source = "<U002E> /x2e U+002E FULL STOP\n% comment\n<U002C> /x2c U+002C COMMA\n";
    assert_eq!(parse_charmap(source), Ok(2));
    assert!(matches!(
        parse_charmap("<U002E> zz U+002E FULL STOP\n"),
        Err(LocaledefError::Malformed(_))
    ));
    assert!(matches!(
        parse_charmap("not a charmap line\n"),
        Err(LocaledefError::Malformed(_))
    ));
    assert!(matches!(
        parse_charmap("# only a comment\n"),
        Err(LocaledefError::Malformed(_))
    ));
}

#[test]
fn compile_emits_the_runtime_magic() {
    let image = compile(
        "LC_NUMERIC\ndecimal_point \".\"\nEND LC_NUMERIC\n",
        "<U002E> /x2e U+002E FULL STOP\n",
        "UTF-8",
        Some("x"),
    )
    .expect("compile");
    assert_eq!(image.get(..4), Some([0xff, 0x88, 0xff, 0x89].as_slice()));
}

#[test]
fn arguments_are_parsed() {
    let options = parse_args(&[
        "-i".to_string(),
        "locale".to_string(),
        "-f".to_string(),
        "charmap".to_string(),
        "-o".to_string(),
        "out".to_string(),
        "name".to_string(),
    ])
    .expect("parse");
    assert_eq!(options.input, "locale");
    assert_eq!(options.charmap, "charmap");
    assert_eq!(options.output.as_deref(), Some("out"));
    assert_eq!(options.name.as_deref(), Some("name"));
    assert_eq!(parse_args(&["-i".to_string()]), Err(LocaledefError::Usage));
    assert_eq!(parse_args(&["-x".to_string()]), Err(LocaledefError::Usage));
}
