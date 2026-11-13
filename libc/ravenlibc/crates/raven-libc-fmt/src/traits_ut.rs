// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::VecSink;

#[test]
fn sink_pad_writes_one_call_of_70_units() {
    let mut sink = VecSink::<u8>::new();
    sink.pad(b'x', 70).unwrap();
    assert_eq!(sink.data.len(), 70);
    assert_eq!(sink.writes, 1);
    assert!(sink.data.iter().all(|&b| b == b'x'));
}

#[test]
fn sink_pad_zero_is_a_no_op() {
    let mut sink = VecSink::<u8>::new();
    sink.pad(b'x', 0).unwrap();
    assert_eq!(sink.data.len(), 0);
    assert_eq!(sink.writes, 0);
}

#[test]
fn no_float_refuses_dec_and_hex() {
    let mut buf = [0u8; 8];
    let v = FloatBits {
        kind: FloatKind::F64,
        bits: 0,
    };
    assert_eq!(NoFloat.dec_digits(v, 2, 6, &mut buf), Err(FmtErr::Invalid));
    assert_eq!(
        NoFloat.hex_digits(v, false, -1, &mut buf),
        Err(FmtErr::Invalid)
    );
}

#[test]
fn stub_float_reports_unavailable() {
    let mut buf = [0u8; 8];
    let v = FloatBits {
        kind: FloatKind::F64,
        bits: 0,
    };
    assert_eq!(
        StubFloat.dec_digits(v, 2, 6, &mut buf),
        Err(FmtErr::FloatUnavailable)
    );
    assert_eq!(
        StubFloat.hex_digits(v, true, 0, &mut buf),
        Err(FmtErr::FloatUnavailable)
    );
}

#[test]
fn no_scan_float_refuses_every_kind() {
    assert_eq!(
        NoScanFloat.parse(b"1.5", FloatKind::F64),
        Err(FmtErr::Invalid)
    );
    assert_eq!(
        NoScanFloat.parse(b"1.5", FloatKind::LongDouble),
        Err(FmtErr::Invalid)
    );
}
