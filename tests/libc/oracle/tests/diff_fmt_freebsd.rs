// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for the FreeBSD printf engine.
//!
//! Every case compares the C return value and every output byte. The cells are
//! the layers of `C43` section 7.1 (integers, floats, strings, positional,
//! truncation, K03) plus the wide entry points. `%m` wording, non-NULL `%p`
//! (addresses differ by construction), the `C23 %w` lengths and `%L` floats
//! (a `long double` cannot cross a stable-Rust variadic call) are excluded and
//! listed in the report.

#![cfg(feature = "oracle-fmt-freebsd")]
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

use core::ffi::{c_char, c_void};
use std::vec::Vec;

use common::{CStrings, CWide, PrintDiff, diff_printf};
use raven_libc_fmt::{FmtOptions, NumericFormat};
use raven_libc_oracle::support_fmt::{Arg, OracleVa, call_swprintf_words, format_u32};

fn grouping_opts() -> FmtOptions<'static> {
    FmtOptions {
        numeric: NumericFormat {
            decimal_point: b".",
            thousands_sep: b",",
            grouping: b"\x03\x03",
        },
        mode: raven_libc_ctype::MbMode::Utf8,
        ..FmtOptions::default()
    }
}

fn cmp(d: &PrintDiff, what: &str) {
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

fn one(fmt: &str, args: &[Arg], va: &mut OracleVa, opts: &FmtOptions<'_>) {
    let d = diff_printf(fmt.as_bytes(), args, va, None, opts);
    cmp(&d, fmt);
}

#[test]
fn integers_layer_a() {
    let convs = ["d", "i", "u", "o", "x", "X", "b", "B"];
    let lengths = ["", "h", "hh", "l", "ll", "j", "z", "t"];
    let flags = ["", "-", "+", " ", "#", "0", "'", "-+", "+0", "# ", "-0"];
    let widths = ["", "1", "5", "40"];
    let precs = ["", ".0", ".5"];
    let values: [i64; 8] = [
        0,
        1,
        -1,
        i32::MIN as i64,
        i32::MAX as i64,
        i64::MIN,
        -2,
        u64::MAX as i64,
    ];
    let mut cases = 0u32;
    for conv in convs {
        for length in lengths {
            for flag in flags {
                for width in widths {
                    for prec in precs {
                        for value in values {
                            let fmt = format!("%{flag}{width}{prec}{length}{conv}");
                            if flag.contains('\'') && (conv == "x" || conv == "X") {
                                // FreeBSD clears grouping for hex; our engine does too.
                            }
                            let mut va = OracleVa::new();
                            va.word(value as u64);
                            one(&fmt, &[Arg::word(value)], &mut va, &grouping_opts());
                            cases = cases.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    println!("fmt-freebsd integers: {cases} cases");
    assert!(cases >= 40_000);
}

#[test]
fn floats_layer_b() {
    let convs = ["e", "E", "f", "F", "g", "G", "a", "A"];
    let flags = ["", "-", "+", " ", "#", "0"];
    let widths = ["", "12"];
    let precs = ["", ".0", ".1", ".6", ".17"];
    let values = [
        0.0,
        -0.0,
        1.5,
        -2.25,
        0.1,
        0.5,
        2.5,
        9.995,
        123_456_789.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let mut cases = 0u32;
    for conv in convs {
        for flag in flags {
            for width in widths {
                for prec in precs {
                    for value in values {
                        let fmt = format!("%{flag}{width}{prec}{conv}");
                        let mut va = OracleVa::new();
                        va.dbl(value);
                        let d = diff_printf(
                            fmt.as_bytes(),
                            &[Arg::dbl(value)],
                            &mut va,
                            Some(0),
                            &grouping_opts(),
                        );
                        cmp(&d, &fmt);
                        cases = cases.saturating_add(1);
                    }
                }
            }
        }
    }
    println!("fmt-freebsd floats: {cases} cases");
    assert!(cases >= 7_000);
}

#[test]
fn strings_and_characters() {
    let strings = ["", "a", "abcdef", "é€😀"];
    let flags = ["", "-", "+", " ", "#", "0", "'", "-0"];
    let widths = ["", "1", "5", "40"];
    let precs = ["", ".0", ".1", ".5"];
    let mut cases = 0u32;
    for s in strings {
        let mut cs = CStrings::new();
        let cp = cs.add(s);
        for flag in flags {
            for width in widths {
                for prec in precs {
                    let fmt = format!("%{flag}{width}{prec}s");
                    let mut va = OracleVa::new();
                    let off = va.cstr(s.as_bytes());
                    va.word(off as u64);
                    let d = diff_printf(
                        fmt.as_bytes(),
                        &[Arg::ptr(cp)],
                        &mut va,
                        None,
                        &grouping_opts(),
                    );
                    cmp(&d, &fmt);
                    cases = cases.saturating_add(1);
                }
            }
        }
    }
    // NULL %s.
    for fmt in ["%s", "%10s", "%.0s", "%.3s", "%5.2s"] {
        let mut va = OracleVa::new();
        va.word(0);
        one(
            fmt,
            &[Arg::ptr(core::ptr::null_mut())],
            &mut va,
            &grouping_opts(),
        );
        cases = cases.saturating_add(1);
    }
    // NULL %p: FreeBSD prints 0x0 (A-stdio-11); this is not compared with musl.
    for fmt in ["%p", "%20p", "%-20p|"] {
        let mut va = OracleVa::new();
        va.word(0);
        one(
            fmt,
            &[Arg::ptr(core::ptr::null_mut())],
            &mut va,
            &grouping_opts(),
        );
        cases = cases.saturating_add(1);
    }
    // %c, %lc and %C.
    for fmt in ["%c", "%5c", "%-5c", "%05c", "%lc", "%5lc", "%C"] {
        let mut va = OracleVa::new();
        va.word(0x20ac);
        let d = diff_printf(
            fmt.as_bytes(),
            &[Arg::word(0x20ac)],
            &mut va,
            None,
            &grouping_opts(),
        );
        cmp(&d, fmt);
        cases = cases.saturating_add(1);
    }
    // %ls with ASCII, Latin-1, BMP and astral characters.
    let wides: [&[u32]; 4] = [
        &[b'a' as u32, b'b' as u32],
        &[0x00e9],
        &[0x20ac],
        &[0x1f600],
    ];
    for w in wides {
        for fmt in ["%ls", "%5ls", "%.1ls", "%10.2ls", "%S"] {
            let mut cw = CWide::new();
            let cp = cw.add(w);
            let mut va = OracleVa::new();
            let off = va.wstr(w);
            va.word(off as u64);
            let d = diff_printf(
                fmt.as_bytes(),
                &[Arg::ptr(cp)],
                &mut va,
                None,
                &grouping_opts(),
            );
            cmp(&d, fmt);
            cases = cases.saturating_add(1);
        }
    }
    println!("fmt-freebsd strings: {cases} cases");
}

#[test]
fn obsolete_synonyms_and_percent() {
    for fmt in ["%D", "%O", "%U", "%%", "%5%", "%05%", "%?", "%5?"] {
        let mut va = OracleVa::new();
        va.word((-42i64) as u64);
        let args: Vec<Arg> = if fmt.contains('D') || fmt.contains('O') || fmt.contains('U') {
            vec![Arg::word(-42)]
        } else {
            Vec::new()
        };
        let d = diff_printf(fmt.as_bytes(), &args, &mut va, None, &grouping_opts());
        cmp(&d, fmt);
    }
}

#[test]
fn positional_layer_d() {
    let cases: [(&str, [i64; 3]); 6] = [
        ("%1$d %2$d %3$d", [1, 2, 3]),
        ("%3$d %2$d %1$d", [1, 2, 3]),
        ("%1$d %1$d", [7, 0, 0]),
        ("%2$*1$d", [5, 7, 0]),
        ("%1$.*2$f", [0, 0, 0]),
        ("%1$s %2$d", [0, 9, 0]),
    ];
    for (fmt, vals) in cases {
        let mut va = OracleVa::new();
        let mut args = Vec::new();
        let mut cs = CStrings::new();
        if fmt.contains("$s") {
            let cp = cs.add("x");
            let off = va.cstr(b"x");
            va.word(off as u64);
            va.word(vals[1] as u64);
            args.push(Arg::ptr(cp));
            args.push(Arg::word(vals[1]));
        } else if fmt.contains(".2$f") || fmt.contains(".*2$f") {
            va.words.clear();
            va.dbls.clear();
            va.dbl(1.23456);
            va.word(3);
            args.push(Arg::dbl(1.23456));
            args.push(Arg::word(3));
        } else {
            for &v in vals.iter() {
                va.word(v as u64);
                args.push(Arg::word(v));
            }
        }
        // The C argument list must have exactly the words the format reads.
        let d = if fmt.contains("$f") {
            let c = [Arg::dbl(1.23456), Arg::word(3)];
            diff_printf(fmt.as_bytes(), &c, &mut va, Some(0), &grouping_opts())
        } else {
            diff_printf(fmt.as_bytes(), &args, &mut va, None, &grouping_opts())
        };
        cmp(&d, fmt);
    }
    // Invalid positional uses: the Rust engine rejects a gap, mixing and a type conflict
    // (T18). The FreeBSD donor accepts them (it fills T_UNUSED with an int and overwrites
    // a conflicting type), so these are documented Rust-only policies and excluded from the
    // C comparison.
    for fmt in ["%2$d", "%1$d %d", "%1$d %1$f"] {
        let mut va = OracleVa::new();
        va.word(1).word(2);
        let args = [Arg::word(1), Arg::word(2)];
        let d = diff_printf(fmt.as_bytes(), &args, &mut va, None, &grouping_opts());
        assert!(d.rust.is_err(), "{fmt}: expected a Rust failure");
    }
}

#[test]
fn truncation_layer_f() {
    let _guard = common::c_guard();
    let mut cases = 0u32;
    for fmt in ["[%s]", "%d", "%08.3f", "%'d", "%b"] {
        for cap in 0..=12usize {
            let is_s = fmt.contains("%s");
            let is_float = fmt.contains('f');
            let mut cs = CStrings::new();
            let cp = cs.add("hello");
            let mut va = OracleVa::new();
            let off = va.cstr(b"hello");
            va.words.clear();
            if is_s {
                va.word(off as u64);
            } else if is_float {
                va.dbl(1.5);
            } else {
                va.word(1_234_567);
            }
            let mut buf = [0u8; 32];
            let mut fmtz = fmt.as_bytes().to_vec();
            fmtz.push(0);
            let c_args = if is_s {
                [Arg::ptr(cp)]
            } else if is_float {
                [Arg::dbl(1.5)]
            } else {
                [Arg::word(1_234_567)]
            };
            let cret = unsafe {
                if is_float {
                    raven_libc_oracle::support_fmt::call_printf_dbl(
                        buf.as_mut_ptr() as *mut c_char,
                        cap,
                        fmtz.as_ptr() as *const c_char,
                        &c_args,
                        0,
                        raven_libc_oracle::decls::fmt_freebsd::oracle_snprintf,
                    )
                } else {
                    raven_libc_oracle::support_fmt::call_printf_words(
                        buf.as_mut_ptr() as *mut c_char,
                        cap,
                        fmtz.as_ptr() as *const c_char,
                        &c_args,
                        raven_libc_oracle::decls::fmt_freebsd::oracle_snprintf,
                    )
                }
            };
            let mut rout = [0u8; 32];
            let r = raven_libc_fmt::glue::vsnprintf_core(
                &mut rout,
                cap,
                fmt.as_bytes(),
                &mut va,
                &grouping_opts(),
                &raven_libc_oracle::support_fmt::RealFloat,
            )
            .unwrap_or_else(|_| panic!("{fmt} cap {cap}: Rust error"));
            assert_eq!(cret, r, "{fmt} cap {cap}");
            let n = if cap == 0 {
                0
            } else {
                std::cmp::min(cret as usize, cap - 1)
            };
            assert_eq!(&buf[..n], &rout[..n], "{fmt} cap {cap} bytes");
            if cap > 0 && cret >= 0 {
                assert_eq!(buf[n], 0, "{fmt} cap {cap} NUL");
            }
            cases = cases.saturating_add(1);
        }
    }
    println!("fmt-freebsd truncation: {cases} cases");
}

#[test]
fn percent_n_stores_the_count() {
    let mut cell = [0u8; 8];
    let ptr = cell.as_mut_ptr() as *mut c_void;
    let mut va = OracleVa::new();
    let off = va.cell(8);
    va.word(off as u64);
    let d = diff_printf(b"abc%n", &[Arg::ptr(ptr)], &mut va, None, &grouping_opts());
    cmp(&d, "%n");
    assert_eq!(d.c_ret, 3);
    let c_val = unsafe { common::read_i32(ptr) };
    assert_eq!(c_val, 3);
    assert_eq!(va.read(off, 4), 3);
}

#[test]
fn wide_entry() {
    let _guard = common::c_guard();
    let mut cases = 0u32;
    for fmt in ["%d", "%5d", "%ls", "%lc", "%s", "%%"] {
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
            va.word(0x20ac);
            args.push(Arg::word(0x20ac));
        } else if fmt.contains("s") {
            let p = cs.add("hé");
            let off = va.cstr("hé".as_bytes());
            va.word(off as u64);
            args.push(Arg::ptr(p));
        } else if fmt.contains('d') {
            va.word(42);
            args.push(Arg::word(42));
        }
        let mut buf = [0u32; 64];
        let cret = unsafe {
            call_swprintf_words(
                buf.as_mut_ptr() as *mut i32,
                buf.len(),
                fmtz.as_ptr(),
                &args,
                raven_libc_oracle::decls::fmt_freebsd::oracle_swprintf,
            )
        };
        let units: Vec<u32> = fmt.chars().map(|c| c as u32).collect();
        let mut opts = grouping_opts();
        opts.mode = raven_libc_ctype::MbMode::Utf8;
        let rust =
            format_u32(&units, &mut va, &opts).unwrap_or_else(|_| panic!("{fmt}: Rust error"));
        if cret < 0 {
            continue;
        }
        assert_eq!(cret as usize, rust.len(), "{fmt}");
        assert_eq!(&buf[..cret as usize], &rust[..], "{fmt}");
        cases = cases.saturating_add(1);
    }
    println!("fmt-freebsd wide: {cases} cases");
}
