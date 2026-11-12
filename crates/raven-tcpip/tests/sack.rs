// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SACK scoreboard vectors and receiver block selection.

#![cfg(feature = "testutil")]

use raven_tcpip::{NextKind, SackScoreboard, receiver_blocks, sack_option_bytes};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Array(Vec<Value>),
    Table(Vec<(String, Value)>),
}

impl Default for Value {
    fn default() -> Self {
        Self::Int(0)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }
    fn skip(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }
    fn value(&mut self) -> Value {
        self.skip();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'[') => self.array(),
            Some(b'"') => Value::Str(self.string()),
            Some(b't') => {
                self.pos += 4;
                Value::Bool(true)
            }
            Some(b'f') => {
                self.pos += 5;
                Value::Bool(false)
            }
            _ => Value::Int(self.integer()),
        }
    }
    fn string(&mut self) -> String {
        assert_eq!(self.bump(), Some(b'"'));
        let mut text = String::new();
        while let Some(byte) = self.bump() {
            if byte == b'"' {
                return text;
            }
            text.push(char::from(byte));
        }
        panic!("unterminated string");
    }
    fn key(&mut self) -> String {
        self.skip();
        let mut text = String::new();
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                text.push(char::from(byte));
                self.pos += 1;
            } else {
                break;
            }
        }
        text
    }
    fn table(&mut self) -> Value {
        assert_eq!(self.bump(), Some(b'{'));
        let mut entries = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }
            let key = self.key();
            self.skip();
            assert_eq!(self.bump(), Some(b'='));
            let value = self.value();
            entries.push((key, value));
            self.skip();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => break,
            }
        }
        Value::Table(entries)
    }
    fn array(&mut self) -> Value {
        assert_eq!(self.bump(), Some(b'['));
        let mut entries = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(b']') {
                self.pos += 1;
                break;
            }
            entries.push(self.value());
            self.skip();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => break,
            }
        }
        Value::Array(entries)
    }
    fn integer(&mut self) -> i64 {
        self.skip();
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
        }
        let mut value = 0i64;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                value = value * 10 + i64::from(byte - b'0');
                self.pos += 1;
            } else {
                break;
            }
        }
        if negative { -value } else { value }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Table(entries) => entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

fn as_int(value: &Value) -> i64 {
    match value {
        Value::Int(number) => *number,
        other => panic!("integer expected: {other:?}"),
    }
}

fn as_bool(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        other => panic!("bool expected: {other:?}"),
    }
}

fn as_array(value: &Value) -> &[Value] {
    match value {
        Value::Array(values) => values,
        other => panic!("array expected: {other:?}"),
    }
}

fn as_pairs(value: &Value) -> Vec<(i64, i64)> {
    as_array(value)
        .iter()
        .map(|pair| {
            let pair = as_array(pair);
            (as_int(&pair[0]), as_int(&pair[1]))
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
struct Case {
    name: String,
    mss: u32,
    dup: u32,
    high_data: u32,
    high_rxt: u32,
    rescue_rxt: Option<u32>,
    snd_una: u32,
    unsent: bool,
    receiver_window: u32,
    range_cap: usize,
    sacked: Vec<(i64, i64)>,
    dsack: Option<(i64, i64)>,
    previously_sacked: Vec<(i64, i64)>,
    expect: Value,
}

fn parse_cases() -> Vec<Case> {
    let text = include_str!("../../../spec/tcpip/vectors/sack.toml");
    let mut cases = Vec::new();
    let mut current: Option<Case> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[case]]" {
            if let Some(case) = current.take() {
                cases.push(case);
            }
            current = Some(Case::default());
            continue;
        }
        let Some(case) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let mut parser = Parser::new(value.trim());
        let value = parser.value();
        match key.trim() {
            "name" => {
                case.name = match value {
                    Value::Str(text) => text,
                    _ => String::new(),
                }
            }
            "mss" => case.mss = as_int(&value) as u32,
            "dup_ack_threshold" => case.dup = as_int(&value) as u32,
            "snd_una" => case.snd_una = as_int(&value) as u32,
            "high_data" => case.high_data = as_int(&value) as u32,
            "high_rxt" => case.high_rxt = as_int(&value) as u32,
            "rescue_rxt" => case.rescue_rxt = Some(as_int(&value) as u32),
            "range_cap" => case.range_cap = as_int(&value) as usize,
            "unsent_available" => case.unsent = as_bool(&value),
            "receiver_window" => case.receiver_window = as_int(&value) as u32,
            "sacked" => case.sacked = as_pairs(&value),
            "previously_sacked" => case.previously_sacked = as_pairs(&value),
            "dsack" => {
                let pairs = as_pairs(&value);
                case.dsack = pairs.first().copied();
            }
            "expect" => case.expect = value,
            _ => {}
        }
    }
    if let Some(case) = current {
        cases.push(case);
    }
    cases
}

fn check_case(name: &str) {
    let cases = parse_cases();
    let case = cases
        .iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("missing SACK case {name}"));
    let mut scoreboard = SackScoreboard::new(
        case.high_data,
        case.high_rxt,
        case.snd_una,
        case.mss,
        case.dup,
        case.range_cap,
    );
    scoreboard.rescue_rxt = case.rescue_rxt;
    for (start, end) in &case.sacked {
        scoreboard.insert_block(*start, *end);
    }
    if let Some((start, end)) = case.dsack {
        scoreboard.note_dsack(start, end);
    }
    if !case.previously_sacked.is_empty() && case.sacked.is_empty() {
        scoreboard.note_reneging();
    }
    let result = scoreboard.next_seg(case.unsent, case.receiver_window);
    let expect = &case.expect;
    let expected_sacked: Vec<(u32, u32)> =
        as_pairs(field(expect, "sacked_after").expect("sacked_after"))
            .into_iter()
            .map(|(start, end)| (start as u32, end as u32))
            .collect();
    assert_eq!(result.sacked, expected_sacked, "{name} sacked");
    assert_eq!(
        result.dsack_detected,
        as_bool(field(expect, "dsack_detected").expect("dsack")),
        "{name} dsack"
    );
    assert_eq!(
        result.reneging,
        as_bool(field(expect, "reneging").expect("reneging")),
        "{name} reneging"
    );
    assert_eq!(
        result.dropped,
        as_int(field(expect, "dropped").expect("dropped")) as usize,
        "{name} dropped"
    );
    assert_eq!(
        result.pipe,
        as_int(field(expect, "pipe").expect("pipe")) as u32,
        "{name} pipe"
    );
    let expected_lost: Vec<(u32, u32)> = as_pairs(field(expect, "lost").expect("lost"))
        .into_iter()
        .map(|(start, end)| (start as u32, end as u32))
        .collect();
    assert_eq!(result.lost, expected_lost, "{name} lost");
    let next = field(expect, "next_seg").expect("next_seg");
    assert_eq!(
        result.next_seg.start,
        as_int(field(next, "start").expect("start")),
        "{name} next start"
    );
    assert_eq!(
        result.next_seg.len,
        as_int(field(next, "len").expect("len")) as u32,
        "{name} next len"
    );
    assert_eq!(
        result.next_seg.rule,
        as_int(field(next, "rule").expect("rule")) as u8,
        "{name} rule"
    );
    let kind = match field(next, "kind").expect("kind") {
        Value::Str(text) => text.as_str(),
        _ => "",
    };
    let expected_kind = match kind {
        "retransmit" => NextKind::Retransmit,
        "new" => NextKind::New,
        "rescue" => NextKind::Rescue,
        "none" => NextKind::None,
        other => panic!("{name}: unknown kind {other}"),
    };
    assert_eq!(result.next_seg.kind, expected_kind, "{name} kind");
    assert_eq!(
        result.high_rxt_after,
        as_int(field(expect, "high_rxt_after").expect("high_rxt_after")) as u32,
        "{name} high_rxt_after"
    );
    assert_eq!(
        result.rescue_rxt_after,
        as_int(field(expect, "rescue_rxt_after").expect("rescue_rxt_after")),
        "{name} rescue_rxt_after"
    );
}

macro_rules! sack_test {
    ($function:ident, $name:literal) => {
        #[test]
        fn $function() {
            check_case($name);
        }
    };
}

sack_test!(case_empty, "empty-scoreboard");
sack_test!(case_one_range, "one-sacked-range");
sack_test!(case_three_ranges, "three-sacked-ranges");
sack_test!(case_bytes_threshold, "sack-bytes-threshold");
sack_test!(case_below_threshold, "below-threshold");
sack_test!(case_high_rxt_pipe, "high-rxt-pipe");
sack_test!(case_rule2_after_no_rule1, "rule2-after-no-rule1");
sack_test!(case_rule3_no_unsent, "rule3-no-unsent");
sack_test!(case_rule4_rescue, "rule4-rescue");
sack_test!(case_rule5_all_sacked, "rule5-all-sacked");
sack_test!(case_dsack_duplicate, "dsack-duplicate");
sack_test!(case_reneging, "reneging");
sack_test!(case_cap_two, "cap-two");
sack_test!(case_mixed_pipe, "mixed-pipe");
sack_test!(case_lost_gap, "lost-gap");
sack_test!(case_rule4_existing_rescue, "rule4-existing-rescue");
sack_test!(case_high_rxt_in_gap, "high-rxt-in-gap");
sack_test!(case_no_sacked_unsent, "no-sacked-unsent");
sack_test!(case_sacked_tail, "sacked-tail");

#[test]
fn every_sack_case_is_present() {
    assert_eq!(parse_cases().len(), 19);
}

#[test]
fn is_lost_uses_range_count_and_byte_threshold() {
    let mut scoreboard = SackScoreboard::new(6000, 0, 0, 1000, 3, 16);
    for block in [(1000, 2000), (3000, 4000), (5000, 6000)] {
        scoreboard.insert_block(block.0, block.1);
    }
    assert!(scoreboard.is_lost(0));
    assert!(!scoreboard.is_lost(2000));
}

#[test]
fn receiver_blocks_are_most_recent_first() {
    let blocks = receiver_blocks(&[(100, 200), (300, 400), (500, 600)], false, None);
    assert_eq!(blocks, vec![(500, 600), (300, 400), (100, 200)]);
}

#[test]
fn timestamps_reduce_the_receiver_block_count() {
    let holes = [(100, 200), (300, 400), (500, 600), (700, 800)];
    assert_eq!(receiver_blocks(&holes, true, None).len(), 3);
    assert_eq!(receiver_blocks(&holes, false, None).len(), 4);
}

#[test]
fn dsack_block_is_placed_first() {
    let blocks = receiver_blocks(&[(100, 200)], false, Some((0, 100)));
    assert_eq!(blocks[0], (0, 100));
}

#[test]
fn sack_option_bytes_encode_length_and_ranges() {
    let bytes = sack_option_bytes(&[(100, 200), (300, 400)]);
    assert_eq!(bytes[0], 5);
    assert_eq!(bytes[1], 18);
    assert_eq!(bytes.len(), 18);
}

#[test]
fn range_cap_is_conservative() {
    let mut scoreboard = SackScoreboard::new(10_000, 0, 0, 1000, 3, 2);
    scoreboard.insert_block(1000, 2000);
    scoreboard.insert_block(3000, 4000);
    scoreboard.insert_block(5000, 6000);
    assert_eq!(scoreboard.sacked.len(), 2);
    assert_eq!(scoreboard.dropped, 1);
}

#[test]
fn block_outside_the_window_is_dropped() {
    let mut scoreboard = SackScoreboard::new(1000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(2000, 3000);
    assert!(scoreboard.sacked.is_empty());
    assert_eq!(scoreboard.dropped, 1);
}

#[test]
fn adjacent_blocks_merge() {
    let mut scoreboard = SackScoreboard::new(5000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(1000, 2000);
    scoreboard.insert_block(2000, 3000);
    assert_eq!(scoreboard.sacked, vec![(1000, 3000)]);
}

#[test]
fn next_seg_none_when_everything_is_sacked() {
    let mut scoreboard = SackScoreboard::new(3000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(0, 3000);
    let result = scoreboard.next_seg(false, 0);
    assert_eq!(result.next_seg.kind, NextKind::None);
    assert_eq!(result.next_seg.rule, 5);
}

#[test]
fn pipe_counts_retransmitted_bytes_twice() {
    let scoreboard = SackScoreboard::new(5000, 5000, 0, 1000, 3, 8);
    assert_eq!(scoreboard.pipe(), 10_000);
}

#[test]
fn dsack_flag_is_set_for_a_duplicate() {
    let mut scoreboard = SackScoreboard::new(5000, 0, 0, 1000, 3, 8);
    scoreboard.note_dsack(-1000, 0);
    let result = scoreboard.next_seg(true, 10_000);
    assert!(result.dsack_detected);
}

#[test]
fn reneging_clears_the_scoreboard() {
    let mut scoreboard = SackScoreboard::new(4000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(1000, 3000);
    scoreboard.note_reneging();
    assert!(scoreboard.sacked.is_empty());
    let result = scoreboard.next_seg(false, 0);
    assert!(result.reneging);
}

#[test]
fn rule2_uses_the_receiver_window() {
    let mut scoreboard = SackScoreboard::new(1000, 0, 0, 1000, 3, 8);
    let result = scoreboard.next_seg(true, 1_500);
    assert_eq!(result.next_seg.kind, NextKind::New);
    assert_eq!(result.next_seg.len, 500);
}

#[test]
fn rule3_caps_the_length_at_the_gap() {
    let mut scoreboard = SackScoreboard::new(3000, 2000, 0, 1000, 3, 8);
    scoreboard.insert_block(1000, 2000);
    scoreboard.insert_block(2500, 3000);
    let result = scoreboard.next_seg(false, 0);
    assert_eq!(result.next_seg.kind, NextKind::Retransmit);
    assert_eq!(result.next_seg.len, 500);
}

#[test]
fn no_receiver_blocks_for_no_holes() {
    assert!(receiver_blocks(&[], true, None).is_empty());
}

#[test]
fn no_sack_option_bytes_when_empty() {
    assert!(sack_option_bytes(&[]).is_empty());
}

#[test]
fn next_seg_rule4_sets_rescue_point() {
    let mut scoreboard = SackScoreboard::new(3000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(0, 1000);
    let result = scoreboard.next_seg(false, 0);
    assert_eq!(result.next_seg.kind, NextKind::Rescue);
    assert_eq!(result.rescue_rxt_after, 3000);
}

#[test]
fn next_seg_rule1_records_high_rxt() {
    let mut scoreboard = SackScoreboard::new(4000, 0, 0, 1000, 3, 8);
    scoreboard.insert_block(1500, 4000);
    let result = scoreboard.next_seg(false, 0);
    assert_eq!(result.next_seg.rule, 1);
    assert_eq!(result.high_rxt_after, 1000);
}

#[test]
fn dsack_block_header_length_is_correct() {
    let bytes = sack_option_bytes(&[(0, 1000)]);
    assert_eq!(bytes[1], 10);
}
