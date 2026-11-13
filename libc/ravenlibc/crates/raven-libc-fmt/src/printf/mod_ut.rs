// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::sinks::CountingSink;
use crate::test_support::{FakeFloat, SliceVa, VecSink};
use crate::traits::{NoFloat, StubFloat};
use crate::types::NumericFormat;
use raven_libc_ctype::MbMode;

fn format(fmt: &str, build: impl FnOnce(&mut SliceVa)) -> String {
    let mut va = SliceVa::new();
    build(&mut va);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    let r = vformat(&mut sink, fmt.as_bytes(), &mut va, &opts, &FakeFloat);
    match r {
        Ok(_) => String::from_utf8(sink.data).unwrap(),
        Err(e) => panic!("format {fmt:?} failed: {e:?}"),
    }
}

fn format_result(
    fmt: &str,
    build: impl FnOnce(&mut SliceVa),
    opts: &FmtOptions<'_>,
) -> Result<String, FmtErr> {
    let mut va = SliceVa::new();
    build(&mut va);
    let mut sink = VecSink::<u8>::new();
    vformat(&mut sink, fmt.as_bytes(), &mut va, opts, &FakeFloat)
        .map(|_| String::from_utf8(sink.data).unwrap())
}

fn format_wide(fmt: &str, build: impl FnOnce(&mut SliceVa)) -> String {
    let mut va = SliceVa::new();
    build(&mut va);
    let mut sink = VecSink::<u32>::new();
    let units: Vec<u32> = fmt.chars().map(|c| c as u32).collect();
    let mut opts = FmtOptions::default();
    opts.mode = MbMode::Utf8;
    vformat(&mut sink, &units, &mut va, &opts, &FakeFloat).unwrap();
    sink.data
        .iter()
        .map(|&u| char::from_u32(u).unwrap_or('?'))
        .collect()
}

#[test]
fn k03_strings_chars_and_percent() {
    assert_eq!(format("hello", |_| {}), "hello");
    assert_eq!(format("%%", |_| {}), "%");
    assert_eq!(
        format("%c", |v| {
            v.int(b'Z' as u64);
        }),
        "Z"
    );
    assert_eq!(
        format("%5c", |v| {
            v.int(b'x' as u64);
        }),
        "    x"
    );
    assert_eq!(
        format("%-5c|", |v| {
            v.int(b'y' as u64);
        }),
        "y    |"
    );
    assert_eq!(
        format("%s", |v| {
            let p = v.cstr("world");
            v.ptr(p);
        }),
        "world"
    );
    assert_eq!(
        format("%10s", |v| {
            let p = v.cstr("world");
            v.ptr(p);
        }),
        "     world"
    );
    assert_eq!(
        format("%-10s|", |v| {
            let p = v.cstr("world");
            v.ptr(p);
        }),
        "world     |"
    );
    assert_eq!(
        format("%.3s", |v| {
            let p = v.cstr("truncate");
            v.ptr(p);
        }),
        "tru"
    );
    assert_eq!(
        format("%10.3s", |v| {
            let p = v.cstr("truncate");
            v.ptr(p);
        }),
        "       tru"
    );
    assert_eq!(
        format("%s", |v| {
            v.ptr(0);
        }),
        "(null)"
    );
    assert_eq!(
        format("%.0s", |v| {
            v.ptr(0);
        }),
        ""
    );
    assert_eq!(
        format("%.3s", |v| {
            v.ptr(0);
        }),
        "(nu"
    );
    assert_eq!(
        format("%p", |v| {
            v.ptr(0);
        }),
        "0x0"
    );
}

#[test]
fn k03_integers_and_flags() {
    assert_eq!(
        format("%d", |v| {
            v.int(0);
        }),
        "0"
    );
    assert_eq!(
        format("%d", |v| {
            v.int((-42i64) as u64);
        }),
        "-42"
    );
    assert_eq!(
        format("%i", |v| {
            v.int(1234);
        }),
        "1234"
    );
    assert_eq!(
        format("%u", |v| {
            v.int(4_000_000_000);
        }),
        "4000000000"
    );
    assert_eq!(
        format("%x", |v| {
            v.int(0xdead_beef);
        }),
        "deadbeef"
    );
    assert_eq!(
        format("%X", |v| {
            v.int(0xdead_beef);
        }),
        "DEADBEEF"
    );
    assert_eq!(
        format("%o", |v| {
            v.int(0o755);
        }),
        "755"
    );
    assert_eq!(
        format("%8d", |v| {
            v.int(42);
        }),
        "      42"
    );
    assert_eq!(
        format("%-8d|", |v| {
            v.int(42);
        }),
        "42      |"
    );
    assert_eq!(
        format("%08d", |v| {
            v.int((-42i64) as u64);
        }),
        "-0000042"
    );
    assert_eq!(
        format("%+d", |v| {
            v.int(42);
        }),
        "+42"
    );
    assert_eq!(
        format("% d", |v| {
            v.int(42);
        }),
        " 42"
    );
    assert_eq!(
        format("%.6d", |v| {
            v.int(42);
        }),
        "000042"
    );
    assert_eq!(
        format("%8.6d", |v| {
            v.int(42);
        }),
        "  000042"
    );
    assert_eq!(
        format("%08.6d", |v| {
            v.int(42);
        }),
        "  000042"
    );
    assert_eq!(
        format("%u", |v| {
            v.int(0);
        }),
        "0"
    );
    assert_eq!(
        format("%.0u", |v| {
            v.int(0);
        }),
        ""
    );
    assert_eq!(
        format("%ld", |v| {
            v.int((-9_000_000_000i64) as u64);
        }),
        "-9000000000"
    );
    assert_eq!(
        format("%lu", |v| {
            v.int(9_000_000_000);
        }),
        "9000000000"
    );
    assert_eq!(
        format("%lld", |v| {
            v.int(i64::MIN as u64);
        }),
        "-9223372036854775808"
    );
    assert_eq!(
        format("%llu", |v| {
            v.int(u64::MAX);
        }),
        "18446744073709551615"
    );
    assert_eq!(
        format("%zu", |v| {
            v.int(usize::MAX as u64);
        }),
        "18446744073709551615"
    );
}

#[test]
fn k03_floats() {
    assert_eq!(
        format("%f", |v| {
            v.dbl(0.0);
        }),
        "0.000000"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(1.5);
        }),
        "1.500000"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(-2.25);
        }),
        "-2.250000"
    );
    assert_eq!(
        format("%.0f", |v| {
            v.dbl(1.5);
        }),
        "2"
    );
    assert_eq!(
        format("%.2f", |v| {
            v.dbl(1234.5678);
        }),
        "1234.57"
    );
    assert_eq!(
        format("%8.2f", |v| {
            v.dbl(3.5);
        }),
        "    3.50"
    );
    assert_eq!(
        format("%-8.2f|", |v| {
            v.dbl(3.5);
        }),
        "3.50    |"
    );
    assert_eq!(
        format("%08.2f", |v| {
            v.dbl(3.5);
        }),
        "00003.50"
    );
    assert_eq!(
        format("%+.2f", |v| {
            v.dbl(3.5);
        }),
        "+3.50"
    );
    assert_eq!(
        format("% .2f", |v| {
            v.dbl(3.5);
        }),
        " 3.50"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(-0.0);
        }),
        "-0.000000"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(f64::INFINITY);
        }),
        "inf"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(f64::NEG_INFINITY);
        }),
        "-inf"
    );
    assert_eq!(
        format("%f", |v| {
            v.dbl(f64::NAN);
        }),
        "nan"
    );
    assert_eq!(
        format("%F", |v| {
            v.dbl(f64::NAN);
        }),
        "NAN"
    );
    assert_eq!(
        format("%.10f", |v| {
            v.dbl(1.0 / 3.0);
        }),
        "0.3333333333"
    );
}

#[test]
fn k03_pointers_and_strings() {
    assert_eq!(
        format("%p", |v| {
            v.ptr(1);
        }),
        "0x1"
    );
    assert_eq!(
        format("%20p", |v| {
            v.ptr(1);
        }),
        "                 0x1"
    );
    assert_eq!(
        format("%s %s", |v| {
            let a = v.cstr("a");
            let b = v.cstr("b");
            v.ptr(a).ptr(b);
        }),
        "a b"
    );
    assert_eq!(
        format("%.0s", |v| {
            let p = v.cstr("abc");
            v.ptr(p);
        }),
        ""
    );
    assert_eq!(
        format("%5.2s", |v| {
            let p = v.cstr("abc");
            v.ptr(p);
        }),
        "   ab"
    );
}

#[test]
fn k03_argument_order_and_overflow_area_shapes() {
    assert_eq!(
        format("%d:%d:%d:%d:%d:%d:%d:%d:%d:%d", |v| {
            for i in 0..10 {
                v.int(i);
            }
        }),
        "0:1:2:3:4:5:6:7:8:9"
    );
    assert_eq!(
        format("%d:%f:%d:%f:%d:%f", |v| {
            v.int(0).dbl(1.0).int(2).dbl(3.0).int(4).dbl(5.0);
        }),
        "0:1.000000:2:3.000000:4:5.000000"
    );
    assert_eq!(
        format("%d%d%d%d%d%d%d%d%f%f", |v| {
            for i in 0..8 {
                v.int(i);
            }
            v.dbl(1.5).dbl(2.5);
        }),
        "012345671.5000002.500000"
    );
}

#[test]
fn upper_half_garbage_is_truncated() {
    assert_eq!(
        format("%d", |v| {
            v.int_garbage(0xffff_ffff);
        }),
        "-1"
    );
    assert_eq!(
        format("%hhd", |v| {
            v.int_garbage(0x0000_00ff);
        }),
        "-1"
    );
    assert_eq!(
        format("%hd", |v| {
            v.int_garbage(0x0000_ffff);
        }),
        "-1"
    );
    assert_eq!(
        format("%u", |v| {
            v.int_garbage(0x0000_002a);
        }),
        "42"
    );
    assert_eq!(
        format("%x", |v| {
            v.int_garbage(0x0000_00ff);
        }),
        "ff"
    );
    assert_eq!(
        format("%c", |v| {
            v.int_garbage(0x0000_0041);
        }),
        "A"
    );
    assert_eq!(
        format("%c", |v| {
            v.int(0x141);
        }),
        "A"
    );
    assert_eq!(
        format("%u", |v| {
            v.int(0xffff_ffff_0000_002a);
        }),
        "42"
    );
}

#[test]
fn flags_and_prefixes() {
    assert_eq!(
        format("%#x", |v| {
            v.int(255);
        }),
        "0xff"
    );
    assert_eq!(
        format("%#X", |v| {
            v.int(255);
        }),
        "0XFF"
    );
    assert_eq!(
        format("%#x", |v| {
            v.int(0);
        }),
        "0"
    );
    assert_eq!(
        format("%#o", |v| {
            v.int(8);
        }),
        "010"
    );
    assert_eq!(
        format("%#.0o", |v| {
            v.int(0);
        }),
        "0"
    );
    assert_eq!(
        format("%#b", |v| {
            v.int(5);
        }),
        "0b101"
    );
    assert_eq!(
        format("%#B", |v| {
            v.int(5);
        }),
        "0B101"
    );
    assert_eq!(
        format("%#b", |v| {
            v.int(0);
        }),
        "0"
    );
    assert_eq!(
        format("%+5d", |v| {
            v.int(42);
        }),
        "  +42"
    );
    assert_eq!(
        format("%-05d|", |v| {
            v.int(42);
        }),
        "42   |"
    );
    assert_eq!(
        format("%.0d", |v| {
            v.int(0);
        }),
        ""
    );
    assert_eq!(
        format("%.0d", |v| {
            v.int(7);
        }),
        "7"
    );
    assert_eq!(format("%5%", |_| {}), "    %");
    assert_eq!(format("%05%", |_| {}), "0000%");
    assert_eq!(format("%?", |_| {}), "?");
    assert_eq!(format("%5?", |_| {}), "    ?");
    assert_eq!(format("abc%", |_| {}), "abc");
}

#[test]
fn grouping_flag() {
    let nf = NumericFormat {
        decimal_point: b".",
        thousands_sep: b",",
        grouping: b"\x03\x03",
    };
    let opts = FmtOptions {
        numeric: nf,
        ..FmtOptions::default()
    };
    assert_eq!(
        format_result(
            "%'d",
            |v| {
                v.int(1_234_567);
            },
            &opts
        )
        .unwrap(),
        "1,234,567"
    );
    assert_eq!(
        format_result(
            "%'f",
            |v| {
                v.dbl(1_234_567.5);
            },
            &opts
        )
        .unwrap(),
        "1,234,567.500000"
    );
    assert_eq!(
        format_result(
            "%'e",
            |v| {
                v.dbl(1_234_567.5);
            },
            &opts
        )
        .unwrap(),
        "1.234568e+06"
    );
    assert_eq!(
        format_result(
            "%'d",
            |v| {
                v.int(123);
            },
            &FmtOptions::default()
        )
        .unwrap(),
        "123"
    );
}

#[test]
fn percent_n_sizes_and_policies() {
    let mut va = SliceVa::new();
    let b = va.cell(8);
    let h = va.cell(8);
    let i = va.cell(8);
    let l = va.cell(8);
    let ll = va.cell(8);
    let j = va.cell(8);
    let z = va.cell(8);
    let t = va.cell(8);
    va.ptr(b).ptr(h).ptr(i).ptr(l).ptr(ll).ptr(j).ptr(z).ptr(t);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    let fmt = b"abc%hhn%hn%n%ln%lln%jn%zn%tn";
    let r = vformat(&mut sink, fmt, &mut va, &opts, &FakeFloat).unwrap();
    assert_eq!(r, 3);
    assert_eq!(va.read_cell(b, 1), 3);
    assert_eq!(va.read_cell(h, 2), 3);
    assert_eq!(va.read_cell(i, 4), 3);
    assert_eq!(va.read_cell(l, 8), 3);
    assert_eq!(va.read_cell(ll, 8), 3);
    assert_eq!(va.read_cell(j, 8), 3);
    assert_eq!(va.read_cell(z, 8), 3);
    assert_eq!(va.read_cell(t, 8), 3);
}

#[test]
fn percent_n_policies() {
    let mut opts = FmtOptions::default();
    opts.pct_n = PctN::Abort;
    assert_eq!(
        format_result(
            "abc%n",
            |v| {
                let c = v.cell(4);
                v.ptr(c);
            },
            &opts
        ),
        Err(FmtErr::PctNAbort)
    );
    opts.pct_n = PctN::Reject;
    assert_eq!(
        format_result(
            "abc%n",
            |v| {
                let c = v.cell(4);
                v.ptr(c);
            },
            &opts
        ),
        Err(FmtErr::Invalid)
    );
}

#[test]
fn percent_m_uses_the_captured_errno() {
    let mut opts = FmtOptions::default();
    opts.serrno = 2;
    assert_eq!(format_result("%m", |_| {}, &opts).unwrap(), "errno 2");
    assert_eq!(format_result("%.4m", |_| {}, &opts).unwrap(), "errn");
    assert_eq!(format_result("%10m", |_| {}, &opts).unwrap(), "   errno 2");
}

#[test]
fn obsolete_synonyms() {
    assert_eq!(
        format("%D", |v| {
            v.int((-42i64) as u64);
        }),
        "-42"
    );
    assert_eq!(
        format("%O", |v| {
            v.int(8);
        }),
        "10"
    );
    assert_eq!(
        format("%U", |v| {
            v.int(42);
        }),
        "42"
    );
}

#[test]
fn wide_characters() {
    let mut opts = FmtOptions::default();
    opts.mode = MbMode::Utf8;
    assert_eq!(
        format_result(
            "%lc",
            |v| {
                v.int(0x20ac);
            },
            &opts
        )
        .unwrap(),
        "€"
    );
    assert_eq!(
        format_wide("%lc", |v| {
            v.int(0x20ac);
        }),
        "€"
    );
    assert_eq!(
        format_wide("%c", |v| {
            v.int(0x41);
        }),
        "A"
    );
    assert_eq!(
        format_wide("%s", |v| {
            let p = v.cstr("hé");
            v.ptr(p);
        }),
        "hé"
    );
}

#[test]
fn wide_strings_and_precision() {
    let mut opts = FmtOptions::default();
    opts.mode = MbMode::Utf8;
    let mut va = SliceVa::new();
    let p = va.wstr(&[0x00e9, 0x20ac, 0x1f600]);
    va.ptr(p);
    let mut sink = VecSink::<u8>::new();
    let r = vformat(&mut sink, b"%.3ls", &mut va, &opts, &FakeFloat).unwrap();
    assert_eq!(r, 2);
    assert_eq!(sink.data, "é".as_bytes());
    assert_eq!(
        format_wide("%ls", |v| {
            let p = v.wstr(&[0x20ac, 0x41]);
            v.ptr(p);
        }),
        "€A"
    );
}

#[test]
fn null_wide_string() {
    let mut opts = FmtOptions::default();
    opts.mode = MbMode::Utf8;
    assert_eq!(
        format_result(
            "%ls",
            |v| {
                v.ptr(0);
            },
            &opts
        )
        .unwrap(),
        "(null)"
    );
    assert_eq!(
        format_wide("%ls", |v| {
            v.ptr(0);
        }),
        "(null)"
    );
}

#[test]
fn positional_arguments() {
    assert_eq!(
        format("%2$*1$d", |v| {
            v.int(5).int(7);
        }),
        "    7"
    );
    assert_eq!(
        format("%1$d %1$d", |v| {
            v.int(9);
        }),
        "9 9"
    );
    assert_eq!(
        format("%2$d %1$d", |v| {
            v.int(1).int(2);
        }),
        "2 1"
    );
    assert_eq!(
        format("%1$.*2$f", |v| {
            v.dbl(1.23456).int(3);
        }),
        "1.235"
    );
    assert_eq!(
        format("%1$s %2$d", |v| {
            let p = v.cstr("x");
            v.ptr(p).int(7);
        }),
        "x 7"
    );
    assert_eq!(
        format_result(
            "%2$d",
            |v| {
                v.int(1).int(2);
            },
            &FmtOptions::default()
        ),
        Err(FmtErr::Invalid)
    );
    assert_eq!(
        format_result(
            "%1$d %d",
            |v| {
                v.int(1).int(2);
            },
            &FmtOptions::default()
        ),
        Err(FmtErr::Invalid)
    );
}

#[test]
fn vformat_dry_validates_without_writing() {
    let mut va = SliceVa::new();
    assert!(vformat_dry::<u8, _>(b"plain %d", &mut va).is_ok());
    assert!(vformat_dry::<u8, _>(b"%1$d %2$d", &mut va).is_ok());
    assert_eq!(vformat_dry::<u8, _>(b"%2$d", &mut va), Err(FmtErr::Invalid));
    assert_eq!(
        vformat_dry::<u8, _>(b"%1$d %d", &mut va),
        Err(FmtErr::Invalid)
    );
}

#[test]
fn overflow_is_detected_before_writing() {
    let mut va = SliceVa::new();
    va.int(b'x' as u64);
    let mut sink = CountingSink::<u8>::new();
    let opts = FmtOptions::default();
    let r = vformat(&mut sink, b"%2147483647cX", &mut va, &opts, &FakeFloat);
    assert_eq!(r, Err(FmtErr::Overflow));
    assert_eq!(sink.count(), 2_147_483_647);
}

#[test]
fn sink_failures_are_reported() {
    let mut va = SliceVa::new();
    let mut sink = VecSink::<u8>::failing(0);
    let opts = FmtOptions::default();
    let r = vformat(&mut sink, b"abc", &mut va, &opts, &FakeFloat);
    assert_eq!(r, Err(FmtErr::Sink));
}

#[test]
fn stub_float_prints_the_literal_and_continues() {
    let mut va = SliceVa::new();
    va.dbl(1.5).int(42);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    let r = vformat(&mut sink, b"%8f|%d", &mut va, &opts, &StubFloat).unwrap();
    assert_eq!(r, 11);
    assert_eq!(sink.data, b" *float*|42");
}

#[test]
fn no_float_is_an_error() {
    let mut va = SliceVa::new();
    va.dbl(1.5);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    assert_eq!(
        vformat(&mut sink, b"%f", &mut va, &opts, &NoFloat),
        Err(FmtErr::Invalid)
    );
}

#[test]
fn precision_does_not_read_past_the_limit() {
    let mut va = SliceVa::new();
    let p = va.cstr("abcdef");
    va.ptr(p);
    let mut sink = VecSink::<u8>::new();
    let opts = FmtOptions::default();
    vformat(&mut sink, b"%.0s", &mut va, &opts, &FakeFloat).unwrap();
    assert_eq!(va.max_read, 0);
    let mut va = SliceVa::new();
    let p = va.cstr("abcdef");
    va.ptr(p);
    let mut sink = VecSink::<u8>::new();
    vformat(&mut sink, b"%.3s", &mut va, &opts, &FakeFloat).unwrap();
    assert_eq!(sink.data, b"abc");
    assert_eq!(va.max_read, p + 2);
}
