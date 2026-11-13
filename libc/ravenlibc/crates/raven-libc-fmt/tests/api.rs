// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end API tests through the public re-exports.

#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

#[path = "support/mod.rs"]
mod support;

use raven_libc_fmt::{
    FmtErr, FmtOptions, NumericFormat, PctN, ScanOptions, Sink, SinkError, StoreSize, glue,
    vformat, vformat_dry, vscan,
};
use support::{FakeFloat, FakeScanFloat, RecordingSource, SliceVa, VecSink};

fn ok<T>(r: Result<T, FmtErr>) -> T {
    match r {
        Ok(v) => v,
        Err(_) => panic!("engine error"),
    }
}

#[test]
fn public_printf_path() {
    let mut va = SliceVa::new();
    let p = va.cstr("world");
    va.int(42).ptr(p);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    let n = ok(vformat(&mut sink, b"%d %s", &mut va, &opts, &FakeFloat));
    assert_eq!(n, 8);
    assert_eq!(sink.data, b"42 world");
}

#[test]
fn public_scanf_path() {
    let mut va = SliceVa::new();
    let cell = va.cell(8);
    let text = va.cell(16);
    va.ptr(cell).ptr(text);
    let mut src = RecordingSource::new(b"42 abc".to_vec());
    let opts = ScanOptions::default();
    let res = ok(vscan(&mut src, b"%d %s", &mut va, &opts, &FakeScanFloat));
    assert_eq!(res.assigned, 2);
    assert_eq!(res.consumed, 6);
    assert!(!res.return_eof);
    assert_eq!(va.read_cell(cell, 4), 42);
    assert_eq!(va.arena.get(text..text + 3), Some(b"abc".as_slice()));
}

#[test]
fn public_glue_and_dry_run() {
    let mut va = SliceVa::new();
    let p = va.cstr("abc");
    va.ptr(p);
    let mut out = [0u8; 8];
    let opts = FmtOptions::default();
    let n = ok(glue::vsnprintf_core(
        &mut out, 8, b"[%s]", &mut va, &opts, &FakeFloat,
    ));
    assert_eq!(n, 5);
    assert_eq!(&out[..6], b"[abc]\0");
    let mut dry = SliceVa::new();
    dry.int(1).int(2);
    assert!(vformat_dry::<u8, _>(b"%1$d %2$d", &mut dry).is_ok());
    assert!(vformat_dry::<u8, _>(b"%2$d", &mut dry).is_err());
}

#[test]
fn public_sink_trait_and_options() {
    struct Failing;
    impl Sink<u8> for Failing {
        fn write(&mut self, _s: &[u8]) -> Result<(), SinkError> {
            Err(SinkError)
        }
    }
    let mut va = SliceVa::new();
    let opts = FmtOptions {
        pct_n: PctN::Store,
        serrno: 1,
        numeric: NumericFormat {
            decimal_point: b".",
            thousands_sep: b"",
            grouping: b"",
        },
        mode: raven_libc_ctype::MbMode::Byte,
    };
    assert!(matches!(
        vformat(&mut Failing, b"x", &mut va, &opts, &FakeFloat),
        Err(FmtErr::Sink)
    ));
    assert_eq!(StoreSize::Ll.bytes(), 8);
}
