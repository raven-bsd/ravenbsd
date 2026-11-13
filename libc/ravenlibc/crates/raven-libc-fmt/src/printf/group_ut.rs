// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::VecSink;
use crate::types::C_NUMERIC_FORMAT;

const GROUPED: NumericFormat<'static> = NumericFormat {
    decimal_point: b".",
    thousands_sep: b",",
    grouping: b"\x03\x03",
};

fn grouped(digits: &[u8], wide: bool) -> String {
    let (g, extra) = Grouping::new(digits.len() as i32, &GROUPED, wide);
    assert!(extra > 0 || digits.len() <= 3);
    let mut sink = VecSink::<u8>::new();
    let used = g
        .print(&mut sink, digits, digits.len(), MbMode::Byte)
        .unwrap();
    assert_eq!(used, digits.len());
    String::from_utf8(sink.data).unwrap()
}

#[test]
fn c_locale_has_no_grouping() {
    let (g, extra) = Grouping::new(7, &C_NUMERIC_FORMAT, false);
    assert_eq!(extra, 0);
    assert!(!g.active());
    let mut sink = VecSink::<u8>::new();
    g.print(&mut sink, b"1234567", 7, MbMode::Byte).unwrap();
    assert_eq!(sink.data, b"1234567");
}

#[test]
fn grouping_three_three() {
    assert_eq!(grouped(b"1", false), "1");
    assert_eq!(grouped(b"12", false), "12");
    assert_eq!(grouped(b"123", false), "123");
    assert_eq!(grouped(b"1234", false), "1,234");
    assert_eq!(grouped(b"1234567", false), "1,234,567");
    assert_eq!(grouped(b"12345678", false), "12,345,678");
}

#[test]
fn grouping_pads_a_short_integer_part() {
    // The engine may hand grouping_print fewer digits than the integer part of %f.
    let (g, _) = Grouping::new(4, &GROUPED, false);
    let mut sink = VecSink::<u8>::new();
    g.print(&mut sink, b"12", 2, MbMode::Byte).unwrap();
    assert_eq!(sink.data, b"1,200");
}

#[test]
fn grouping_wide_output_uses_one_separator_unit() {
    let (g, extra) = Grouping::new(4, &GROUPED, true);
    assert_eq!(extra, 1);
    let mut sink = VecSink::<u32>::new();
    g.print(&mut sink, b"1234", 4, MbMode::Utf8).unwrap();
    let text: String = sink
        .data
        .iter()
        .map(|&u| char::from_u32(u).unwrap_or('?'))
        .collect();
    assert_eq!(text, "1,234");
}

#[test]
fn a_hostile_grouping_string_terminates() {
    let nf = NumericFormat {
        decimal_point: b".",
        thousands_sep: b",",
        grouping: &[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    };
    let (g, extra) = Grouping::new(1000, &nf, false);
    assert!(extra > 0);
    assert!(g.active());
}
