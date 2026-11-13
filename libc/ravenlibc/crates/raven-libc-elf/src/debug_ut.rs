// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// Word lists are fixed literals.

//! `LD_DEBUG` word parsing.

use super::*;

#[test]
fn every_word_selects_its_bit() {
    let words = [
        (b"files".as_slice(), DebugWords::FILES),
        (b"reloc", DebugWords::RELOC),
        (b"bind", DebugWords::BIND),
        (b"symbols", DebugWords::SYMBOLS),
        (b"versions", DebugWords::VERSIONS),
        (b"tls", DebugWords::TLS),
        (b"init", DebugWords::INIT),
        (b"libs", DebugWords::LIBS),
        (b"statistics", DebugWords::STATISTICS),
        (b"trace", DebugWords::TRACE),
    ];
    for (word, bit) in words {
        let parsed = DebugWords::parse(word);
        assert!(parsed.contains(bit), "{word:?}");
        assert_eq!(parsed.0.count_ones(), 1, "{word:?}");
    }
}

#[test]
fn a_comma_list_unions_the_words() {
    let parsed = DebugWords::parse(b"files,reloc");
    assert!(parsed.contains(DebugWords::FILES));
    assert!(parsed.contains(DebugWords::RELOC));
    assert!(!parsed.contains(DebugWords::BIND));
}

#[test]
fn all_sets_every_bit() {
    let parsed = DebugWords::parse(b"all");
    assert_eq!(parsed.0, DebugWords::ALL);
    assert!(parsed.contains(DebugWords::FILES));
    assert!(parsed.contains(DebugWords::TRACE));
}

#[test]
fn the_number_one_means_all() {
    assert_eq!(DebugWords::parse(b"1").0, DebugWords::ALL);
}

#[test]
fn an_unknown_word_is_ignored() {
    let parsed = DebugWords::parse(b"files,unknown,reloc");
    assert_eq!(parsed.0, DebugWords::FILES | DebugWords::RELOC);
    assert!(DebugWords::parse(b"unknown").is_empty());
}

#[test]
fn an_empty_value_selects_nothing() {
    assert!(DebugWords::parse(b"").is_empty());
    assert!(DebugWords::parse(b",,").is_empty());
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    let parsed = DebugWords::parse(b" files , reloc ");
    assert_eq!(parsed.0, DebugWords::FILES | DebugWords::RELOC);
}

#[test]
fn the_all_mask_contains_every_named_bit() {
    for bit in [
        DebugWords::FILES,
        DebugWords::RELOC,
        DebugWords::BIND,
        DebugWords::SYMBOLS,
        DebugWords::VERSIONS,
        DebugWords::TLS,
        DebugWords::INIT,
        DebugWords::LIBS,
        DebugWords::STATISTICS,
        DebugWords::TRACE,
    ] {
        assert!(DebugWords(DebugWords::ALL).contains(bit));
    }
}
