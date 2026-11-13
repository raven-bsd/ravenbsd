// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for the musl printf engine (second opinion).
//!
//! Musl and FreeBSD agree on the integer, character and default-locale float
//! cells; the known differences (`%p` of NULL, `'`, `%b`, `%m` detail,
//! `%D %O %U`, the C23 `%w` lengths, `%5%`) are excluded here and listed in the
//! report.

#![cfg(all(feature = "oracle-fmt-musl", not(feature = "oracle-fmt-freebsd")))]
#![allow(
    unsafe_code,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks
)]

mod common;

use std::vec::Vec;

use common::{CStrings, CWide, diff_musl};
use raven_libc_fmt::FmtOptions;
use raven_libc_oracle::support_fmt::{Arg, OracleVa, call_swprintf_words, format_u32};

fn cmp(d: &common::PrintDiff, what: &str) {
    match &d.rust {
        Ok(r) => {
            if d.c_ret < 0 {
                panic!("{what}: C failed ({}) but Rust returned {r:?}", d.c_ret);
            }
            if d.c_ret as usize != r.len() || d.c_out != *r {
                panic!(
                    "{what}: C ret {} {:?} != Rust ret {} {:?}",
                    d.c_ret,
                    String::from_utf8_lossy(&d.c_out),
                    r.len(),
                    String::from_utf8_lossy(r)
                );
            }
        }
        Err(_) => {
            if d.c_ret >= 0 {
                panic!("{what}: C returned {} but Rust failed", d.c_ret);
            }
        }
    }
}

#[test]
fn integers_agree() {
    let convs = ["d", "i", "u", "o", "x", "X"];
    let lengths = ["", "h", "hh", "l", "ll", "j", "z", "t"];
    let flags = ["", "-", "+", " ", "#", "0", "-+", "+0", "# ", "-0"];
    let widths = ["", "1", "5", "40"];
    let precs = ["", ".0", ".5"];
    let values: [i64; 7] = [0, 1, -1, i32::MIN as i64, i32::MAX as i64, i64::MIN, -2];
    let mut cases = 0u32;
    for conv in convs {
        for length in lengths {
            for flag in flags {
                for width in widths {
                    for prec in precs {
                        for value in values {
                            let fmt = format!("%{flag}{width}{prec}{length}{conv}");
                            let mut va = OracleVa::new();
                            va.word(value as u64);
                            let d = diff_musl(
                                fmt.as_bytes(),
                                &[Arg::word(value)],
                                &mut va,
                                None,
                                &FmtOptions::default(),
                            );
                            cmp(&d, &fmt);
                            cases = cases.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    println!("fmt-musl integers: {cases} cases");
}

#[test]
fn floats_agree() {
    let convs = ["e", "E", "f", "F", "g", "G"];
    let flags = ["", "-", "+", " ", "#", "0"];
    let precs = ["", ".0", ".1", ".6"];
    let values = [
        0.0,
        -0.0,
        1.5,
        -2.25,
        0.1,
        123_456_789.0,
        f64::INFINITY,
        f64::NAN,
    ];
    let mut cases = 0u32;
    for conv in convs {
        for flag in flags {
            for prec in precs {
                for value in values {
                    if value.is_nan() && (flag.contains('+') || flag.contains(' ')) {
                        // musl prints the sign of a NaN; FreeBSD (and the Rust engine)
                        // prints the bare text (donor-map/04 3.8 rule 5).
                        continue;
                    }
                    let fmt = format!("%{flag}{prec}{conv}");
                    let mut va = OracleVa::new();
                    va.dbl(value);
                    let d = diff_musl(
                        fmt.as_bytes(),
                        &[Arg::dbl(value)],
                        &mut va,
                        Some(0),
                        &FmtOptions::default(),
                    );
                    cmp(&d, &fmt);
                    cases = cases.saturating_add(1);
                }
            }
        }
    }
    println!("fmt-musl floats: {cases} cases");
}

#[test]
fn strings_agree() {
    let mut cases = 0u32;
    for s in ["", "a", "abcdef", "é€😀"] {
        let mut cs = CStrings::new();
        let cp = cs.add(s);
        for fmt in ["%s", "%10s", "%-10s|", "%.3s", "%10.3s"] {
            let mut va = OracleVa::new();
            let off = va.cstr(s.as_bytes());
            va.word(off as u64);
            let d = diff_musl(
                fmt.as_bytes(),
                &[Arg::ptr(cp)],
                &mut va,
                None,
                &FmtOptions::default(),
            );
            cmp(&d, fmt);
            cases = cases.saturating_add(1);
        }
    }
    for fmt in ["%s", "%10s", "%.0s", "%.3s"] {
        let mut va = OracleVa::new();
        va.word(0);
        let d = diff_musl(
            fmt.as_bytes(),
            &[Arg::ptr(core::ptr::null_mut())],
            &mut va,
            None,
            &FmtOptions::default(),
        );
        cmp(&d, fmt);
        cases = cases.saturating_add(1);
    }
    for fmt in ["%c", "%5c", "%-5c"] {
        let mut va = OracleVa::new();
        va.word(b'A' as u64);
        let d = diff_musl(
            fmt.as_bytes(),
            &[Arg::word(b'A' as i64)],
            &mut va,
            None,
            &FmtOptions::default(),
        );
        cmp(&d, fmt);
        cases = cases.saturating_add(1);
    }
    println!("fmt-musl strings: {cases} cases");
}

#[test]
fn wide_agrees() {
    let _guard = common::c_guard();
    for (fmt, arg) in [
        ("%d", Some(42i32)),
        ("%5d", Some(42)),
        ("%ls", None),
        ("%lc", Some(0x20ac)),
        ("%s", None),
        ("%%", None),
    ] {
        let fmtz: Vec<i32> = fmt.chars().map(|c| c as i32).chain([0]).collect();
        let mut cw = CWide::new();
        let mut cs = CStrings::new();
        let mut args: Vec<Arg> = Vec::new();
        let mut va = OracleVa::new();
        if fmt.contains("ls") {
            let p = cw.add(&[0x20ac, 0x41]);
            let off = va.wstr(&[0x20ac, 0x41]);
            va.word(off as u64);
            args.push(Arg::ptr(p));
        } else if fmt.contains("lc") {
            va.word(arg.unwrap_or(0) as u64);
            args.push(Arg::word(arg.unwrap_or(0) as i64));
        } else if fmt.contains('s') {
            let p = cs.add("hé");
            let off = va.cstr("hé".as_bytes());
            va.word(off as u64);
            args.push(Arg::ptr(p));
        } else if fmt.contains('d') {
            va.word(arg.unwrap_or(0) as u64);
            args.push(Arg::word(arg.unwrap_or(0) as i64));
        }
        let mut buf = [0u32; 64];
        let cret = unsafe {
            call_swprintf_words(
                buf.as_mut_ptr() as *mut i32,
                buf.len(),
                fmtz.as_ptr(),
                &args,
                raven_libc_oracle::decls::fmt_musl::oracle_swprintf,
            )
        };
        let units: Vec<u32> = fmt.chars().map(|c| c as u32).collect();
        let mut opts = FmtOptions::default();
        opts.mode = raven_libc_ctype::MbMode::Utf8;
        let rust =
            format_u32(&units, &mut va, &opts).unwrap_or_else(|_| panic!("{fmt}: Rust error"));
        if cret < 0 {
            continue;
        }
        assert_eq!(cret as usize, rust.len(), "{fmt}");
        assert_eq!(&buf[..cret as usize], &rust[..], "{fmt}");
    }
}
