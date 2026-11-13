// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The required API, donor-vector and boundary tests that do not need the C oracle library.

use raven_libc_dtoa::{
    C_NUMERIC, DigitMode, DtoaCtx, DtoaError, Kind, LongDouble, MAX_DIGITS_F64, MAX_DIGITS_LD,
    NumericLocale, RangeFlags, Rounding, dtoa, ldtoa, strtod, strtof, strtold,
};

fn parse(text: &[u8]) -> (f64, usize, RangeFlags) {
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).expect("strtod");
    (r.value, r.consumed, r.range)
}

#[test]
fn arena_peak_800_digits() {
    let mut text = b"1.".to_vec();
    text.extend(std::iter::repeat_n(b'9', 799));
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, &text, &C_NUMERIC, Rounding::Nearest).expect("800-digit strtod");
    assert_eq!(r.consumed, text.len());
    assert!(ctx.peak_words() <= 2048, "strtod peak {}", ctx.peak_words());

    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 2000];
    let d = dtoa(
        &mut ctx,
        f64::from_bits(1),
        DigitMode::Decimals,
        767,
        Rounding::Nearest,
        &mut out,
    )
    .expect("dtoa true min mode 3");
    assert!(d.len > 400, "dtoa true-min digits {}", d.len);
    assert!(ctx.peak_words() <= 2048, "dtoa peak {}", ctx.peak_words());
}

#[test]
fn dtoa_known_values_all_modes() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; MAX_DIGITS_F64];
    for mode in [
        DigitMode::Shortest,
        DigitMode::ShortestSteele,
        DigitMode::SigDigits,
        DigitMode::Decimals,
        DigitMode::SigDigitsShort,
        DigitMode::DecimalsShort,
    ] {
        let d = dtoa(&mut ctx, 1.5, mode, 2, Rounding::Nearest, &mut out).expect("dtoa");
        assert_eq!(d.kind, Kind::Finite);
        assert_eq!(d.decpt, 1);
        assert_eq!(&out[..d.len], b"15", "mode {mode:?}");
    }
    let d = dtoa(
        &mut ctx,
        123456789.0,
        DigitMode::SigDigits,
        4,
        Rounding::Nearest,
        &mut out,
    )
    .expect("dtoa sig digits");
    assert_eq!(&out[..d.len], b"1235");
    assert_eq!(d.decpt, 9);
}

#[test]
fn dtoa_zero_inf_nan_and_buffer() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; MAX_DIGITS_F64];
    let zero = dtoa(
        &mut ctx,
        -0.0,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .expect("zero");
    assert_eq!(zero.kind, Kind::Zero);
    assert!(zero.negative);
    assert_eq!(&out[..zero.len], b"0");
    assert_eq!(zero.decpt, 1);
    let inf = dtoa(
        &mut ctx,
        f64::INFINITY,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .expect("inf");
    assert_eq!((inf.kind, inf.len), (Kind::Inf, 0));
    let nan = dtoa(
        &mut ctx,
        f64::NAN,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .expect("nan");
    assert_eq!((nan.kind, nan.len), (Kind::Nan, 0));

    let mut small = [0u8; 1];
    let err = dtoa(
        &mut ctx,
        1.5,
        DigitMode::SigDigits,
        10,
        Rounding::Nearest,
        &mut small,
    )
    .unwrap_err();
    assert_eq!(err, DtoaError::BufferTooSmall { needed: 10 });
}

#[test]
fn dtoa_mode3_empty_digits() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; MAX_DIGITS_F64];
    let d = dtoa(
        &mut ctx,
        0.0001,
        DigitMode::Decimals,
        2,
        Rounding::Nearest,
        &mut out,
    )
    .expect("mode 3");
    assert_eq!(d.kind, Kind::Finite);
    assert!(d.len <= 1, "value rounds to {} digits", d.len);
    if d.len == 1 {
        assert_eq!(out[0], b'0');
    }
    assert_eq!(d.decpt, 1);
}

#[test]
fn strtod_donor_cases() {
    // t_strtod.c: strtod_basic (a sample of the 1..1024*1000 step-1024 loop).
    for i in [1usize, 1025, 1024 * 100, 1024 * 999] {
        let text = format!("{i}.{}", i + 1);
        let (v, n, flags) = parse(text.as_bytes());
        assert!(v > 0.0, "{text}");
        assert_eq!(n, text.len());
        assert!(flags.is_empty(), "{text}");
    }
    // strtod_hex: -0x0 and -0x keep the sign and consume 4/2 bytes.
    let (v, n, _) = parse(b"-0x0");
    assert_eq!(n, 4);
    assert_eq!(v.to_bits(), (-0.0f64).to_bits());
    let (v, n, _) = parse(b"-0x");
    assert_eq!(n, 2);
    assert_eq!(v.to_bits(), (-0.0f64).to_bits());

    // strtod_inf / strtof_inf / strtold_inf.
    for text in [
        b"Inf".as_slice(),
        b"INF",
        b"-Inf",
        b"-INF",
        b"Infinity",
        b"+Infinity",
        b"INFINITY",
        b"-INFINITY",
        b"InFiNiTy",
        b"+InFiNiTy",
    ] {
        let mut ctx = DtoaCtx::new();
        assert!(
            strtod(&mut ctx, text, &C_NUMERIC, Rounding::Nearest)
                .unwrap()
                .value
                .is_infinite()
        );
        let mut ctx = DtoaCtx::new();
        assert!(
            strtof(&mut ctx, text, &C_NUMERIC, Rounding::Nearest)
                .unwrap()
                .value
                .is_infinite()
        );
        let mut ctx = DtoaCtx::new();
        let ld = strtold(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).unwrap();
        assert_eq!(ld.value.0[9] & 0x7f, 0x7f);
    }

    // strtod_nan / strtof_nan / strtold_nan: "NaN(x)y" consumes 8 bytes.
    let (v, n, _) = parse(b"NaN(x)y");
    assert!(v.is_nan());
    assert_eq!(n, 6);
    let mut ctx = DtoaCtx::new();
    let f = strtof(&mut ctx, b"NaN(x)y", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert!(f.value.is_nan());
    assert_eq!(f.consumed, 6);
    let mut ctx = DtoaCtx::new();
    let ld = strtold(&mut ctx, b"NaN(x)y", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(ld.consumed, 6);

    // strtod_round: upward > downward for the near-1+eps value.
    let text = b"1.00000011920928977282585492503130808472633361816406";
    let mut ctx = DtoaCtx::new();
    let up = strtod(&mut ctx, text, &C_NUMERIC, Rounding::Upward)
        .unwrap()
        .value;
    let mut ctx = DtoaCtx::new();
    let down = strtod(&mut ctx, text, &C_NUMERIC, Rounding::Downward)
        .unwrap()
        .value;
    assert!(up > down);

    // strtod_underflow: 2 at the end of ~430 zeros underflows to zero with ERANGE.
    let mut underflow = b"0.".to_vec();
    underflow.extend(std::iter::repeat_n(b'0', 430));
    underflow.push(b'2');
    let (v, _, flags) = parse(&underflow);
    assert_eq!(v, 0.0);
    assert!(flags.contains(RangeFlags::UNDERFLOW));
}

#[test]
fn strtod_gay_hard_cases() {
    let cases: &[(&[u8], u64)] = &[
        (b"2.2250738585072011e-308", 0x000f_ffff_ffff_ffff),
        (b"4.9406564584124654e-324", 0x1),
        (b"1e23", 0x44b5_2d02_c7e1_4af6),
        (b"9007199254740993", 0x4340_0000_0000_0000),
        (
            b"0.010000000000000000057612911342378542997169",
            0x3f84_7ae1_47ae_147b,
        ),
        (
            b"1.8254370818746402660437411213933955878019332885742187",
            0x3ffd_34fd_8378_ea83,
        ),
    ];
    for (text, bits) in cases {
        let (v, n, flags) = parse(text);
        assert_eq!(v.to_bits(), *bits, "{:?}", String::from_utf8_lossy(text));
        assert_eq!(n, text.len());
        assert!(!flags.contains(RangeFlags::OVERFLOW));
    }
}

#[test]
fn strtod_locale_separator() {
    let comma = NumericLocale {
        decimal_point: b",",
    };
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, b"1,5", &comma, Rounding::Nearest).unwrap();
    assert_eq!(r.value, 1.5);
    assert_eq!(r.consumed, 3);
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, b"1.5", &comma, Rounding::Nearest).unwrap();
    assert_eq!(r.value, 1.0);
    assert_eq!(r.consumed, 1);
    let two = NumericLocale {
        decimal_point: "\u{066b}".as_bytes(),
    };
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, b"2\xd9\xab5", &two, Rounding::Nearest).unwrap();
    assert_eq!(r.value, 2.5);
    assert_eq!(r.consumed, 4);
}

#[test]
fn strtod_directed_overflow() {
    let max = f64::MAX.to_bits();
    let neg_max = f64::MAX.to_bits() | 0x8000_0000_0000_0000;
    for (mode, bits) in [
        (Rounding::TowardZero, max),
        (Rounding::Downward, max),
        (Rounding::Nearest, f64::INFINITY.to_bits()),
        (Rounding::Upward, f64::INFINITY.to_bits()),
    ] {
        let mut ctx = DtoaCtx::new();
        let r = strtod(&mut ctx, b"1e999", &C_NUMERIC, mode).unwrap();
        assert_eq!(r.value.to_bits(), bits, "{mode:?}");
        assert!(r.range.contains(RangeFlags::OVERFLOW));
    }
    let mut ctx = DtoaCtx::new();
    let r = strtod(&mut ctx, b"-1e999", &C_NUMERIC, Rounding::Upward).unwrap();
    assert_eq!(r.value.to_bits(), neg_max);
}

#[test]
fn hexfloat_forms() {
    for (text, bits) in [
        (b"0x1p0".as_slice(), 1.0f64.to_bits()),
        (b"0x.8p1", 1.0f64.to_bits()),
        (b"0X1.FFFFFFFFFFFFFp1023", f64::MAX.to_bits()),
        (b"0x1p-1074", 1u64),
    ] {
        let (v, n, _) = parse(text);
        assert_eq!(v.to_bits(), bits, "{:?}", String::from_utf8_lossy(text));
        assert_eq!(n, text.len());
    }
    let (v, n, flags) = parse(b"0x1p-1075");
    assert_eq!(v, 0.0);
    assert_eq!(n, 9);
    assert!(flags.contains(RangeFlags::UNDERFLOW));
    let missing = b"0x1p";
    let (_, n, _) = parse(missing);
    assert!(n < missing.len());
}

#[cfg(target_arch = "x86_64")]
#[test]
fn ld80_known_values() {
    let mut ctx = DtoaCtx::new();
    let v = strtold(&mut ctx, b"1e800", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(v.consumed, 5);
    assert_ne!(v.value.0[9] & 0x7f, 0, "finite ld80 exponent");
    assert_ne!(v.value.0[9] & 0x7f, 0x7f, "not infinity");

    let mut ctx = DtoaCtx::new();
    let small = strtold(
        &mut ctx,
        b"0x0.0000000000000002p-16382",
        &C_NUMERIC,
        Rounding::Nearest,
    )
    .unwrap();
    assert_eq!(small.value.0[0..8], [1, 0, 0, 0, 0, 0, 0, 0]);

    // Pseudo-denormal: exponent != 0 but the explicit integer bit is clear -> NaN (A-num-12).
    let mut pseudo = [0u8; 16];
    pseudo[0] = 0x42;
    pseudo[8] = 0x01;
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; MAX_DIGITS_LD];
    let d = ldtoa(
        &mut ctx,
        LongDouble(pseudo),
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(d.kind, Kind::Nan);
    assert_eq!(d.len, 0);
}

#[cfg(all(not(feature = "alloc"), target_arch = "x86_64"))]
#[test]
fn no_alloc_path() {
    // The 800-digit double case never needs the spill.
    let mut text = b"1.".to_vec();
    text.extend(std::iter::repeat_n(b'9', 799));
    let mut ctx = DtoaCtx::new();
    assert!(strtod(&mut ctx, &text, &C_NUMERIC, Rounding::Nearest).is_ok());

    // A 5000-digit string and ldtoa of 1e4900 need the spill and must say NoMemory instead.
    let mut long = b"1.".to_vec();
    long.extend(std::iter::repeat_n(b'9', 4999));
    let mut ctx = DtoaCtx::new();
    assert_eq!(
        strtod(&mut ctx, &long, &C_NUMERIC, Rounding::Nearest).unwrap_err(),
        DtoaError::NoMemory
    );
    // 1e4900 as an ld80 image, generated independently (Python exact rounding).
    let huge = LongDouble([
        0xc1, 0xa4, 0x37, 0x59, 0x27, 0x04, 0x92, 0xae, 0x94, 0x7f, 0, 0, 0, 0, 0, 0,
    ]);
    let mut out = [0u8; MAX_DIGITS_LD];
    let mut ctx = DtoaCtx::new();
    assert_eq!(
        ldtoa(
            &mut ctx,
            huge,
            DigitMode::Decimals,
            0,
            Rounding::Nearest,
            &mut out
        )
        .unwrap_err(),
        DtoaError::NoMemory
    );
}
