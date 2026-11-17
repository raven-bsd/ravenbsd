// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the shared format-directive scanner.

use super::*;

#[test]
fn literals_and_a_plain_directive() {
    let mut items = FormatIter::new(b"a%Yb");
    assert_eq!(items.next(), Some(Item::Literal(b'a')));
    assert_eq!(
        items.next(),
        Some(Item::Directive(Directive {
            conversion: b'Y',
            pad: Pad::Default,
            upper: false,
            swap: false,
            alt_e: 0,
            alt_o: 0,
        }))
    );
    assert_eq!(items.next(), Some(Item::Literal(b'b')));
    assert_eq!(items.next(), None);
}

#[test]
fn padding_case_and_alternative_flags() {
    let mut items = FormatIter::new(b"%_0^#EOY");
    assert_eq!(
        items.next(),
        Some(Item::Directive(Directive {
            conversion: b'Y',
            pad: Pad::Zero,
            upper: true,
            swap: true,
            alt_e: 1,
            alt_o: 1,
        }))
    );
}

#[test]
fn trailing_percent_is_its_own_item() {
    let mut items = FormatIter::new(b"abc%");
    assert_eq!(items.next(), Some(Item::Literal(b'a')));
    assert_eq!(items.next(), Some(Item::Literal(b'b')));
    assert_eq!(items.next(), Some(Item::Literal(b'c')));
    assert_eq!(items.next(), Some(Item::TrailingPercent));
    assert_eq!(items.next(), None);
}

#[test]
fn whitespace_is_a_literal() {
    let mut items = FormatIter::new(b" \t%n");
    assert_eq!(items.next(), Some(Item::Literal(b' ')));
    assert_eq!(items.next(), Some(Item::Literal(b'\t')));
    assert_eq!(
        items.next(),
        Some(Item::Directive(Directive {
            conversion: b'n',
            pad: Pad::Default,
            upper: false,
            swap: false,
            alt_e: 0,
            alt_o: 0,
        }))
    );
}
