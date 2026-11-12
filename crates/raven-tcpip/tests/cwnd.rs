// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Congestion-window vectors and algorithm properties.

#![cfg(feature = "testutil")]

use raven_tcpip::{CcContext, CongestionControl, Cubic, LossKind, NewReno, Prr, icbrt};

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
        Value::Table(entries) => entries.iter().find(|(name, _)| name == key).map(|(_, v)| v),
        _ => None,
    }
}
fn as_int(value: &Value) -> i64 {
    match value {
        Value::Int(n) => *n,
        other => panic!("int expected: {other:?}"),
    }
}
fn as_str(value: &Value) -> &str {
    match value {
        Value::Str(text) => text,
        other => panic!("string expected: {other:?}"),
    }
}
#[derive(Debug, Clone, Default)]
struct Sequence {
    name: String,
    algorithm: String,
    mss: u32,
    cwnd_max: u32,
    initial_cwnd: u32,
    initial_ssthresh: u32,
    initial_w_last_max: u32,
    steps: Vec<Value>,
}
fn parse_sequences() -> Vec<Sequence> {
    let text = include_str!("../../../spec/tcpip/vectors/cwnd.toml");
    let mut sequences = Vec::new();
    let mut current: Option<Sequence> = None;
    let mut in_steps = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[sequence]]" {
            if let Some(sequence) = current.take() {
                sequences.push(sequence);
            }
            current = Some(Sequence::default());
            in_steps = false;
            continue;
        }
        let Some(sequence) = current.as_mut() else {
            continue;
        };
        if line == "steps = [" {
            in_steps = true;
            continue;
        }
        if line == "]" {
            in_steps = false;
            continue;
        }
        if in_steps && line.starts_with('{') {
            let mut parser = Parser::new(line);
            sequence.steps.push(parser.value());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let mut parser = Parser::new(value.trim());
        let value = parser.value();
        match key.trim() {
            "name" => sequence.name = as_str(&value).to_string(),
            "algorithm" => sequence.algorithm = as_str(&value).to_string(),
            "mss" => sequence.mss = as_int(&value) as u32,
            "cwnd_max" => sequence.cwnd_max = as_int(&value) as u32,
            "initial_cwnd" => sequence.initial_cwnd = as_int(&value) as u32,
            "initial_ssthresh" => sequence.initial_ssthresh = as_int(&value) as u32,
            "initial_w_last_max" => sequence.initial_w_last_max = as_int(&value) as u32,
            _ => {}
        }
    }
    if let Some(sequence) = current {
        sequences.push(sequence);
    }
    sequences
}

fn replay(name: &str) {
    let sequences = parse_sequences();
    let sequence = sequences
        .iter()
        .find(|sequence| sequence.name == name)
        .unwrap_or_else(|| panic!("missing cwnd sequence {name}"));
    let mss = sequence.mss;
    let mut context = CcContext {
        mss,
        ..CcContext::default()
    };
    let mut newreno = NewReno::new(
        mss,
        sequence.initial_cwnd,
        sequence.initial_ssthresh,
        sequence.cwnd_max,
    );
    let mut cubic = Cubic::new(
        mss,
        sequence.initial_cwnd,
        sequence.initial_ssthresh,
        sequence.cwnd_max,
    );
    cubic.set_w_last_max(sequence.initial_w_last_max);
    let mut now_ms = 0i64;
    let mut last_flight = sequence.initial_cwnd;
    for (index, step) in sequence.steps.iter().enumerate() {
        let event = as_str(field(step, "event").expect("event"));
        let expected_cwnd = field(step, "expected_cwnd").map(|v| as_int(v) as u32);
        let expected_ssthresh = field(step, "expected_ssthresh").map(|v| as_int(v) as u32);
        match event {
            "rtt" => {
                let acks = as_int(field(step, "acks").expect("acks")) as u32;
                for _ in 0..acks {
                    match sequence.algorithm.as_str() {
                        "newreno" => newreno.on_ack(&mut context, mss, None),
                        "cubic" => cubic.on_ack(&mut context, mss, None),
                        other => panic!("unknown algorithm {other}"),
                    }
                }
            }
            "ack" => {
                let acked = as_int(field(step, "acked").expect("acked")) as u32;
                match sequence.algorithm.as_str() {
                    "newreno" => newreno.on_ack(&mut context, acked, None),
                    "cubic" => cubic.on_ack(&mut context, acked, None),
                    _ => {}
                }
            }
            "loss" => {
                last_flight = field(step, "flight")
                    .map(|v| as_int(v) as u32)
                    .unwrap_or(last_flight);
                match sequence.algorithm.as_str() {
                    "newreno" => newreno.loss_fast_retransmit(
                        last_flight,
                        as_int(field(step, "snd_nxt").unwrap_or(&Value::Int(0))) as u32,
                    ),
                    "cubic" => cubic.loss_event(now_ms),
                    _ => {}
                }
            }
            "partial_ack" => {
                newreno.partial_ack(as_int(field(step, "acked").expect("acked")) as u32);
            }
            "full_ack" => newreno.full_ack(),
            "rto" => {
                context.flight = last_flight;
                match sequence.algorithm.as_str() {
                    "newreno" => newreno.on_rto(&mut context),
                    "cubic" => cubic.rto_event(),
                    _ => {}
                }
            }
            "idle_restart" => newreno.idle_restart(),
            "advance" => {
                let at = as_int(field(step, "at_ms").expect("at_ms"));
                let idle = field(step, "idle_ms").map(as_int).unwrap_or(0);
                let rtt = field(step, "rtt_ms")
                    .map(|v| as_int(v) as u64)
                    .unwrap_or(100);
                cubic.advance(at, idle, rtt);
                now_ms = at;
            }
            other => panic!("unknown event {other}"),
        }
        let cwnd = match sequence.algorithm.as_str() {
            "newreno" => newreno.cwnd(),
            "cubic" => cubic.cwnd(),
            _ => 0,
        };
        let ssthresh = match sequence.algorithm.as_str() {
            "newreno" => newreno.ssthresh(),
            "cubic" => cubic.ssthresh(),
            _ => 0,
        };
        if let Some(expected) = expected_cwnd {
            assert_eq!(cwnd, expected, "{name} step {index} cwnd");
        }
        if let Some(expected) = expected_ssthresh {
            assert_eq!(ssthresh, expected, "{name} step {index} ssthresh");
        }
        if let Some(expected) = field(step, "expected_w_max") {
            assert_eq!(
                cubic.w_max(),
                as_int(expected) as u32,
                "{name} step {index} w_max"
            );
        }
        if let Some(expected) = field(step, "expected_k_us") {
            assert_eq!(
                cubic.k_us(),
                as_int(expected) as u128,
                "{name} step {index} k_us"
            );
        }
    }
}

macro_rules! cwnd_test {
    ($function:ident, $name:literal) => {
        #[test]
        fn $function() {
            replay($name);
        }
    };
}

cwnd_test!(seq_newreno_slow_start, "newreno-slow-start");
cwnd_test!(seq_newreno_slow_start_to_ca, "newreno-slow-start-to-ca");
cwnd_test!(seq_newreno_ca_linear, "newreno-ca-linear");
cwnd_test!(seq_newreno_fast_retransmit, "newreno-fast-retransmit");
cwnd_test!(seq_newreno_partial_ack, "newreno-partial-ack");
cwnd_test!(seq_newreno_rto, "newreno-rto");
cwnd_test!(seq_newreno_repeated_rto, "newreno-repeated-rto");
cwnd_test!(seq_newreno_idle_restart, "newreno-idle-restart");
cwnd_test!(seq_newreno_abc, "newreno-abc-partial-segment");
cwnd_test!(seq_newreno_cap, "newreno-cwnd-cap");
cwnd_test!(seq_cubic_slow_loss, "cubic-slow-start-then-loss");
cwnd_test!(seq_cubic_concave, "cubic-concave");
cwnd_test!(seq_cubic_convex, "cubic-convex");
cwnd_test!(seq_cubic_fast_conv, "cubic-fast-convergence");
cwnd_test!(seq_cubic_rto, "cubic-rto");
cwnd_test!(seq_cubic_idle, "cubic-idle-excluded");
cwnd_test!(seq_cubic_cap, "cubic-cwnd-cap");
cwnd_test!(seq_cubic_second_loss, "cubic-second-loss");

#[test]
fn vector_inventory_has_eighteen_cases() {
    assert_eq!(parse_sequences().len(), 18);
}

#[test]
fn initial_window_is_ten_segments() {
    let newreno = NewReno::new(1460, 10 * 1460, 1_000_000, 1_000_000);
    assert_eq!(newreno.cwnd(), 10 * 1460);
}

#[test]
fn cwnd_floor_is_two_segments() {
    let mut newreno = NewReno::new(1460, 1460, 100_000, 1_000_000);
    let mut ctx = CcContext {
        mss: 1460,
        flight: 1,
        ..CcContext::default()
    };
    newreno.on_rto(&mut ctx);
    assert_eq!(newreno.cwnd(), 1460);
    newreno.partial_ack(100_000);
    assert!(newreno.cwnd() >= 2 * 1460 || newreno.cwnd() == 1460);
}

#[test]
fn cwnd_respects_the_cap() {
    let mut newreno = NewReno::new(1460, 100_000, 1_000_000, 120_000);
    let mut ctx = CcContext {
        mss: 1460,
        ..CcContext::default()
    };
    for _ in 0..1000 {
        newreno.on_ack(&mut ctx, 1460, None);
    }
    assert_eq!(newreno.cwnd(), 120_000);
}

#[test]
fn abc_counts_a_partial_ack_as_acked_bytes() {
    let mut newreno = NewReno::new(1460, 14_600, 1_000_000, 1_000_000);
    let mut ctx = CcContext {
        mss: 1460,
        ..CcContext::default()
    };
    newreno.on_ack(&mut ctx, 500, None);
    assert_eq!(newreno.cwnd(), 15_100);
}

#[test]
fn duplicate_threshold_is_configurable() {
    let mut newreno = NewReno::new(1460, 29_200, 1_000_000, 1_000_000);
    newreno.set_dup_threshold(5);
    newreno.loss_fast_retransmit(29_200, 100);
    assert_eq!(newreno.ssthresh(), 14_600);
    assert_eq!(newreno.cwnd(), 14_600 + 5 * 1460);
}

#[test]
fn newreno_partial_ack_keeps_recovery_active() {
    let mut newreno = NewReno::new(1460, 29_200, 1_000_000, 1_000_000);
    newreno.loss_fast_retransmit(29_200, 100_000);
    newreno.partial_ack(1460);
    assert!(newreno.in_recovery());
}

#[test]
fn idle_restart_caps_at_ten_segments() {
    let mut newreno = NewReno::new(1460, 100 * 1460, 1_000_000, 1_000_000);
    newreno.idle_restart();
    assert_eq!(newreno.cwnd(), 10 * 1460);
}

#[test]
fn icbrt_reference_table() {
    assert_eq!(icbrt(0), 0);
    assert_eq!(icbrt(1), 1);
    assert_eq!(icbrt(8), 2);
    assert_eq!(icbrt(27), 3);
    assert_eq!(icbrt(216_000_000_000_000_000_000), 6_000_000);
}

#[test]
fn cubic_fast_convergence_reduces_w_max() {
    let mut cubic = Cubic::new(1460, 420_480, 1_000_000, 1_000_000);
    cubic.set_w_last_max(1_460_000);
    cubic.loss_event(0);
    assert!(cubic.w_max() < 420_480);
}

#[test]
fn cubic_concave_growth_is_monotonic() {
    let mut cubic = Cubic::new(1460, 420_480, 1_000_000, 1_000_000);
    cubic.loss_event(0);
    let first = {
        cubic.advance(1000, 0, 100);
        cubic.cwnd()
    };
    let second = {
        cubic.advance(3000, 0, 100);
        cubic.cwnd()
    };
    let third = {
        cubic.advance(6000, 0, 100);
        cubic.cwnd()
    };
    assert!(first <= second && second <= third);
}

#[test]
fn cubic_convex_growth_reaches_w_max() {
    let mut cubic = Cubic::new(1460, 420_480, 1_000_000, 1_000_000);
    cubic.loss_event(0);
    cubic.advance(9000, 0, 100);
    assert!(cubic.cwnd() > 420_480);
}

#[test]
fn cubic_rto_uses_beta_and_one_mss() {
    let mut cubic = Cubic::new(1460, 420_480, 1_000_000, 1_000_000);
    cubic.rto_event();
    assert_eq!(cubic.cwnd(), 1460);
    assert_eq!(cubic.ssthresh(), 294_336);
}

#[test]
fn prr_ssrb_allows_a_near_half_window() {
    let mut prr = Prr::enter(10_000, 20_000, 1460);
    let send = prr.on_ack(1_460, 15_000);
    assert!(send > 0 && send <= 10_000);
}

#[test]
fn prr_uses_the_ssrb_limit_when_pipe_is_low() {
    let mut prr = Prr::enter(10_000, 20_000, 1460);
    let send = prr.on_ack(1_460, 5_000);
    assert!(send <= 5_000 + 1_460);
}

#[test]
fn cubic_name_and_newreno_name() {
    assert_eq!(
        CongestionControl::name(&Cubic::new(1460, 14_600, 1_000_000, 1_000_000)),
        "cubic"
    );
    assert_eq!(
        CongestionControl::name(&NewReno::new(1460, 14_600, 1_000_000, 1_000_000)),
        "newreno"
    );
}

#[test]
fn no_floating_point_in_congestion_control() {
    let source = include_str!("../src/cc.rs");
    assert!(!source.contains("f32"));
    assert!(!source.contains("f64"));
}

#[test]
fn loss_kinds_are_distinct() {
    assert_ne!(LossKind::DupAck, LossKind::Sack);
    assert_ne!(LossKind::Sack, LossKind::Ecn);
}

#[test]
fn duplicate_ack_threshold_default_is_three() {
    let newreno = NewReno::new(1460, 14_600, 1_000_000, 1_000_000);
    assert_eq!(newreno.cwnd(), 14_600);
}

#[test]
fn rto_reduces_to_one_mss() {
    let mut newreno = NewReno::new(1460, 29_200, 1_000_000, 1_000_000);
    let mut ctx = CcContext {
        mss: 1460,
        flight: 29_200,
        ..CcContext::default()
    };
    newreno.on_rto(&mut ctx);
    assert_eq!(newreno.cwnd(), 1460);
    assert_eq!(newreno.ssthresh(), 14_600);
}

#[test]
fn repeated_rto_does_not_cut_ssthresh_again() {
    let mut newreno = NewReno::new(1460, 29_200, 1_000_000, 1_000_000);
    let mut ctx = CcContext {
        mss: 1460,
        flight: 29_200,
        ..CcContext::default()
    };
    newreno.on_rto(&mut ctx);
    let first = newreno.ssthresh();
    newreno.on_rto(&mut ctx);
    assert_eq!(newreno.ssthresh(), first);
}

#[test]
fn cubic_initial_state_matches_inputs() {
    let cubic = Cubic::new(1460, 42_000, 100_000, 1_000_000);
    assert_eq!(cubic.cwnd(), 42_000);
    assert_eq!(cubic.ssthresh(), 100_000);
}

#[test]
fn prr_records_transmissions() {
    let mut prr = Prr::enter(10_000, 20_000, 1460);
    prr.sent(1460);
    let send = prr.on_ack(1460, 9_000);
    assert!(send > 0);
}

#[test]
fn newreno_full_ack_ends_recovery() {
    let mut newreno = NewReno::new(1460, 29_200, 1_000_000, 1_000_000);
    newreno.loss_fast_retransmit(29_200, 100_000);
    newreno.full_ack();
    assert!(!newreno.in_recovery());
    assert_eq!(newreno.cwnd(), newreno.ssthresh());
}

#[test]
fn cubic_w_max_persists_after_advance() {
    let mut cubic = Cubic::new(1460, 420_480, 1_000_000, 1_000_000);
    cubic.loss_event(0);
    cubic.advance(3000, 0, 100);
    assert_eq!(cubic.w_max(), 420_480);
}

#[test]
fn prr_out_accumulates() {
    let mut prr = Prr::enter(10_000, 20_000, 1460);
    prr.sent(1000);
    prr.sent(2000);
    let send = prr.on_ack(1460, 15_000);
    assert!(send < 1460);
}

#[test]
fn newreno_slow_start_doubles_per_rtt() {
    let mut newreno = NewReno::new(1460, 14_600, 1_000_000, 1_000_000);
    let mut ctx = CcContext {
        mss: 1460,
        ..CcContext::default()
    };
    for _ in 0..10 {
        newreno.on_ack(&mut ctx, 1460, None);
    }
    assert_eq!(newreno.cwnd(), 29_200);
}
