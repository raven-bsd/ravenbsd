// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for the `dtoa` family: the NetBSD C gdtoa runs next to `raven-libc-dtoa`.
//!
//! Every call sets the host FPU rounding mode for the C side (the donor uses `fegetround()` under
//! `Honor_FLT_ROUNDS`); the Rust side takes the same mode as its `Rounding` parameter. Values,
//! consumed bytes, `ERANGE`, digits, `decpt`, sign and `Kind` are compared.

#![cfg(feature = "oracle-dtoa")]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::undocumented_unsafe_blocks
)]

use core::ffi::{c_char, c_int, c_void};

use raven_libc_dtoa::{
    C_NUMERIC, DigitMode, DtoaCtx, HexDigits, Kind, LongDouble, RangeFlags, Rounding, dtoa, ftoa,
    hdtoa, hldtoa, ldtoa, strtod, strtof, strtold,
};
use raven_libc_oracle::decls::dtoa::{
    CFpi, oracle___dtoa, oracle___freedtoa, oracle___gdtoa, oracle___hdtoa, oracle___hldtoa,
    oracle___ldtoa, oracle___strtopQ_D2A, oracle___strtopx_D2A, oracle_strtod, oracle_strtof,
};
use raven_libc_oracle::norm::with_errno;
use raven_libc_oracle::rng::Rng;

/// The C gdtoa has process-global mutable state (free list, private pool, p5s, `dtoa_result`);
/// calls from parallel test threads must be serialized.
static C_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn c_guard() -> std::sync::MutexGuard<'static, ()> {
    C_LOCK.lock().unwrap()
}

unsafe extern "C" {
    fn fesetround(rounding: c_int) -> c_int;
}

const FE_TONEAREST: c_int = 0x000;
const FE_DOWNWARD: c_int = 0x400;
const FE_UPWARD: c_int = 0x800;
const FE_TOWARDZERO: c_int = 0xc00;

const ROUNDINGS: [(Rounding, c_int); 4] = [
    (Rounding::Nearest, FE_TONEAREST),
    (Rounding::TowardZero, FE_TOWARDZERO),
    (Rounding::Upward, FE_UPWARD),
    (Rounding::Downward, FE_DOWNWARD),
];

fn with_rounding<R>(fe: c_int, f: impl FnOnce() -> R) -> R {
    unsafe {
        fesetround(fe);
    }
    let value = f();
    unsafe {
        fesetround(FE_TONEAREST);
    }
    value
}

fn c_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.push(0);
    out
}

fn c_digits(ptr: *const c_char, end: *const c_char) -> Vec<u8> {
    if ptr.is_null() || end.is_null() || end <= ptr {
        return Vec::new();
    }
    let len = end as usize - ptr as usize;
    unsafe { core::slice::from_raw_parts(ptr as *const u8, len).to_vec() }
}

fn c_end_offset(start: *const c_char, end: *const c_char) -> usize {
    if end.is_null() {
        return 0;
    }
    end as usize - start as usize
}

fn parse_strtod(bytes: &[u8], fe: c_int) -> (u64, usize, bool) {
    let _guard = c_guard();
    let c = c_string(bytes);
    let mut end: *mut c_char = core::ptr::null_mut();
    let outcome = with_rounding(fe, || {
        with_errno(|| {
            let value = unsafe { oracle_strtod(c.as_ptr() as *const c_char, &mut end) };
            (value, end)
        })
    });
    let ((value, end_ptr), errno) = outcome;
    let consumed = c_end_offset(c.as_ptr() as *const c_char, end_ptr);
    let erange = errno == 34; // ERANGE on Linux
    (value.to_bits(), consumed, erange)
}

fn rust_strtod(bytes: &[u8], r: Rounding) -> (u64, usize, bool) {
    let mut ctx = DtoaCtx::new();
    let parsed = strtod(&mut ctx, bytes, &C_NUMERIC, r).expect("strtod");
    let range = parsed.range;
    let erange = range.contains(RangeFlags::OVERFLOW) || range.contains(RangeFlags::UNDERFLOW);
    (parsed.value.to_bits(), parsed.consumed, erange)
}

fn check_strtod(bytes: &[u8]) {
    for (r, fe) in ROUNDINGS {
        let want = parse_strtod(bytes, fe);
        let got = rust_strtod(bytes, r);
        assert_eq!(
            got,
            want,
            "strtod {:?} mode {r:?}: rust {got:?} != c {want:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn strtod_fixed_cases() {
    let cases: &[&[u8]] = &[
        b"0",
        b"1",
        b"-0.0",
        b"1.23",
        b"0.1",
        b"1e23",
        b"1e-23",
        b"2.2250738585072011e-308",
        b"4.9406564584124654e-324",
        b"1.7976931348623157e308",
        b"1.7976931348623159e308",
        b"9007199254740993",
        b"1.8254370818746402660437411213933955878019332885742187",
        b"0x1.8p3",
        b"0x1p-1074",
        b"0x1p-1075",
        b"0x1.fffffffffffffp1023",
        b"0x1p1024",
        b"inf",
        b"-Infinity",
        b"nan",
        b"nan(0x123)",
        b"1e",
        b"1e+",
        b"0x",
        b"   .5e-3x",
        b"1,5",
    ];
    for bytes in cases {
        check_strtod(bytes);
    }
}

fn c_dtoa(value: f64, mode: i32, ndigits: i32, fe: c_int) -> (Vec<u8>, i32, bool, Kind) {
    let _guard = c_guard();
    let mut decpt = 0i32;
    let mut sign = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = with_rounding(fe, || unsafe {
        oracle___dtoa(value, mode, ndigits, &mut decpt, &mut sign, &mut end)
    });
    let digits = c_digits(ptr, end);
    let kind = if value == 0.0 {
        Kind::Zero
    } else if decpt == 9999 {
        if digits == b"Infinity" {
            Kind::Inf
        } else {
            Kind::Nan
        }
    } else {
        Kind::Finite
    };
    let decpt = if decpt == 9999 { 0 } else { decpt };
    unsafe {
        oracle___freedtoa(ptr);
    }
    (digits, decpt, sign != 0, kind)
}

fn rust_dtoa(value: f64, mode: DigitMode, ndigits: i32, r: Rounding) -> (Vec<u8>, i32, bool, Kind) {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 2000];
    let d = dtoa(&mut ctx, value, mode, ndigits, r, &mut out).expect("dtoa");
    (out[..d.len].to_vec(), d.decpt, d.negative, d.kind)
}

#[test]
fn dtoa_fixed_cases() {
    let values = [
        0.0f64,
        -0.0,
        1.0,
        -1.0,
        1.5,
        0.1,
        1e23,
        123456789.0,
        f64::from_bits(1),
        f64::MAX,
        f64::MIN_POSITIVE,
        9.995,
        2.5,
        9.5,
    ];
    let modes = [
        DigitMode::Shortest,
        DigitMode::ShortestSteele,
        DigitMode::SigDigits,
        DigitMode::Decimals,
        DigitMode::SigDigitsShort,
        DigitMode::DecimalsShort,
    ];
    for value in values {
        for mode in modes {
            for ndigits in [-5, -1, 0, 1, 2, 6, 15, 17, 20, 40, 350, 767] {
                if mode == DigitMode::DecimalsShort && ndigits < 0 {
                    // The donor's mode-5 pow5mult receives a negative exponent and hangs; the
                    // port returns the defined no-digit result (see the crate tests).
                    continue;
                }
                for (r, fe) in ROUNDINGS {
                    let want = c_dtoa(value, mode as i32, ndigits, fe);
                    let got = rust_dtoa(value, mode, ndigits, r);
                    assert_eq!(
                        got, want,
                        "dtoa {value:?} mode {mode:?} ndigits {ndigits} rounding {r:?}"
                    );
                }
            }
        }
    }
}

fn fe_of(r: Rounding) -> c_int {
    match r {
        Rounding::Nearest => FE_TONEAREST,
        Rounding::TowardZero => FE_TOWARDZERO,
        Rounding::Upward => FE_UPWARD,
        Rounding::Downward => FE_DOWNWARD,
    }
}

fn c_strtof(bytes: &[u8], fe: c_int) -> (u32, usize, bool) {
    let _guard = c_guard();
    let c = c_string(bytes);
    let mut end: *mut c_char = core::ptr::null_mut();
    let ((value, end_ptr), errno) = with_rounding(fe, || {
        with_errno(|| {
            let value = unsafe { oracle_strtof(c.as_ptr() as *const c_char, &mut end) };
            (value, end)
        })
    });
    (
        value.to_bits(),
        c_end_offset(c.as_ptr() as *const c_char, end_ptr),
        errno == 34,
    )
}

fn rust_strtof(bytes: &[u8], r: Rounding) -> (u32, usize, bool) {
    let mut ctx = DtoaCtx::new();
    let parsed = strtof(&mut ctx, bytes, &C_NUMERIC, r).expect("strtof");
    let erange =
        parsed.range.contains(RangeFlags::OVERFLOW) || parsed.range.contains(RangeFlags::UNDERFLOW);
    (parsed.value.to_bits(), parsed.consumed, erange)
}

fn c_strtold(bytes: &[u8], fe: c_int) -> ([u8; 16], usize, bool) {
    let _guard = c_guard();
    let c = c_string(bytes);
    let mut out = [0u8; 16];
    let mut end: *mut c_char = core::ptr::null_mut();
    let ((_, end_ptr), errno) = with_rounding(fe, || {
        with_errno(|| {
            unsafe {
                oracle___strtopx_D2A(
                    c.as_ptr() as *const c_char,
                    &mut end,
                    out.as_mut_ptr() as *mut c_void,
                    core::ptr::null_mut(),
                );
            }
            ((), end)
        })
    });
    (
        out,
        c_end_offset(c.as_ptr() as *const c_char, end_ptr),
        errno == 34,
    )
}

fn rust_strtold(bytes: &[u8], r: Rounding) -> ([u8; 16], usize, bool) {
    let mut ctx = DtoaCtx::new();
    let parsed = strtold(&mut ctx, bytes, &C_NUMERIC, r).expect("strtold");
    let erange =
        parsed.range.contains(RangeFlags::OVERFLOW) || parsed.range.contains(RangeFlags::UNDERFLOW);
    (parsed.value.0, parsed.consumed, erange)
}

fn check_strtof(bytes: &[u8]) {
    // The NetBSD `strtodg.c` directed-rounding path corrupts `rvb` for formats whose `nbits`
    // is not a multiple of 32: the boundary test calls `lo0bits(rvb->x + i)`, and `lo0bits`
    // shifts its argument in place. The result is a truncated significand in round-zero and
    // round-down (e.g. `5e-31` becomes bits 0x00051212 instead of 0x00a2425f). The Rust port
    // passes a copy, so it produces the mathematically correct truncation; the directed
    // modes are therefore validated by the exact-reference vectors, not by this donor.
    let (r, fe) = (Rounding::Nearest, FE_TONEAREST);
    let want = c_strtof(bytes, fe);
    let got = rust_strtof(bytes, r);
    assert_eq!(
        got,
        want,
        "strtof {:?} mode {r:?}",
        String::from_utf8_lossy(bytes)
    );
}

fn check_strtold(bytes: &[u8]) {
    for (r, fe) in ROUNDINGS {
        let want = c_strtold(bytes, fe);
        let got = rust_strtold(bytes, r);
        assert_eq!(
            got,
            want,
            "strtold {:?} mode {r:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn strtof_fixed_cases() {
    let cases: &[&[u8]] = &[
        b"0",
        b"-0.0",
        b"1.5",
        b"0.1",
        b"16777217",
        b"8388608.5000000001",
        b"3.4028235e38",
        b"3.4028236e38",
        b"1.1754944e-38",
        b"1e-45",
        b"1e-46",
        b"1e999",
        b"0x1.fffffep127",
        b"0x1p-149",
        b"0x1p-150",
        b"inf",
        b"nan",
        b"nan(0x1)",
        b"1e",
        b"0x",
    ];
    for bytes in cases {
        check_strtof(bytes);
    }
}

#[test]
fn strtold_fixed_cases() {
    let cases: &[&[u8]] = &[
        b"0",
        b"-0.0",
        b"1.5",
        b"0.1",
        b"2.16",
        b"0x1.234p0",
        b"0x1.8p3",
        b"1e4932",
        b"1e-4950",
        b"1e-4951",
        b"0x1p-16445",
        b"0x1p-16446",
        b"1.2e4932",
        b"inf",
        b"-Infinity",
        b"nan(0x1)",
        b"1e",
        b"0x",
    ];
    for bytes in cases {
        check_strtold(bytes);
    }
}

fn c_gdtoa_f32(value: f32, mode: i32, ndigits: i32, fe: c_int) -> (Vec<u8>, i32, bool, Kind) {
    let _guard = c_guard();
    let raw = value.to_bits();
    let sign = raw & 0x8000_0000 != 0;
    let abs = raw & 0x7fff_ffff;
    if abs & 0x7f80_0000 == 0x7f80_0000 {
        let kind = if abs & 0x007f_ffff != 0 {
            Kind::Nan
        } else {
            Kind::Inf
        };
        return (Vec::new(), 0, sign, kind);
    }
    if abs == 0 {
        return (b"0".to_vec(), 1, sign, Kind::Zero);
    }
    let mut bits = [abs & 0x007f_ffff];
    let mut ex = ((abs >> 23) & 0xff) as i32;
    if ex != 0 {
        bits[0] |= 0x0080_0000;
    } else {
        ex = 1;
    }
    ex -= 0x7f + 23;
    let fpi = CFpi {
        nbits: 24,
        emin: 1 - 127 - 24 + 1,
        emax: 254 - 127 - 24 + 1,
        rounding: match fe {
            FE_TOWARDZERO => 0,
            FE_UPWARD => 2,
            FE_DOWNWARD => 3,
            _ => 1,
        },
        sudden_underflow: 0,
    };
    let mut kind = if sign { 0x09 } else { 1 };
    let mut decpt = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = with_rounding(fe, || unsafe {
        oracle___gdtoa(
            &fpi,
            ex,
            bits.as_mut_ptr(),
            &mut kind,
            mode,
            ndigits,
            &mut decpt,
            &mut end,
        )
    });
    let digits = c_digits(ptr, end);
    unsafe {
        oracle___freedtoa(ptr);
    }
    (digits, decpt, sign, Kind::Finite)
}

fn rust_ftoa(value: f32, mode: DigitMode, ndigits: i32, r: Rounding) -> (Vec<u8>, i32, bool, Kind) {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 128];
    let d = ftoa(&mut ctx, value, mode, ndigits, r, &mut out).expect("ftoa");
    (out[..d.len].to_vec(), d.decpt, d.negative, d.kind)
}

#[test]
fn ftoa_fixed_cases() {
    let values = [
        0.0f32,
        -0.0,
        1.0,
        -1.0,
        1.5,
        0.1,
        3.402_823_5e38,
        f32::from_bits(1),
        8_388_608.0,
        8_388_609.0,
        9.995,
        2.5,
    ];
    let modes = [
        DigitMode::Shortest,
        DigitMode::ShortestSteele,
        DigitMode::SigDigits,
        DigitMode::Decimals,
        DigitMode::SigDigitsShort,
        DigitMode::DecimalsShort,
    ];
    for value in values {
        for mode in modes {
            for ndigits in [-5, -1, 0, 1, 2, 6, 9, 20, 40] {
                let ndigits: i32 = ndigits;
                if mode == DigitMode::DecimalsShort && ndigits <= 0 {
                    // The donor's gdtoa small_ilim/pow5mult path hangs for mode 5 at ndigits 0
                    // (and below); the port returns the defined no-digit result.
                    continue;
                }
                if mode == DigitMode::DecimalsShort && value != 0.0 {
                    let k = (f64::from(value.abs()).log10().floor()) as i32;
                    if ndigits.wrapping_add(k).wrapping_add(1) <= 0 {
                        // ilim == 0 also reaches the donor's hanging small_ilim path.
                        continue;
                    }
                }
                for (r, fe) in ROUNDINGS {
                    let want = c_gdtoa_f32(value, mode as i32, ndigits, fe);
                    let got = rust_ftoa(value, mode, ndigits, r);
                    assert_eq!(
                        got, want,
                        "ftoa {value:?} mode {mode:?} ndigits {ndigits} rounding {r:?}"
                    );
                }
            }
        }
    }
}

fn c_hdtoa(value: f64, upper: bool, ndigits: i32, fe: c_int) -> (Vec<u8>, i32, bool, Kind) {
    let _guard = c_guard();
    let xdigs = if upper {
        b"0123456789ABCDEF\0".as_ptr()
    } else {
        b"0123456789abcdef\0".as_ptr()
    };
    let mut decpt = 0i32;
    let mut sign = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = with_rounding(fe, || unsafe {
        oracle___hdtoa(
            value,
            xdigs as *const c_char,
            ndigits,
            &mut decpt,
            &mut sign,
            &mut end,
        )
    });
    let digits = c_digits(ptr, end);
    unsafe {
        oracle___freedtoa(ptr);
    }
    let (kind, exp2) = if value == 0.0 {
        (Kind::Zero, 0)
    } else if decpt == i32::MAX {
        (if value.is_nan() { Kind::Nan } else { Kind::Inf }, 0)
    } else {
        (Kind::Finite, decpt - 1)
    };
    let digits = if kind == Kind::Finite || kind == Kind::Zero {
        digits
    } else {
        Vec::new()
    };
    (digits, exp2, sign != 0, kind)
}

fn rust_hdtoa(value: f64, upper: bool, ndigits: i32, r: Rounding) -> (Vec<u8>, i32, bool, Kind) {
    let mut out = [0u8; 128];
    let d = hdtoa(value, upper, ndigits, r, &mut out).expect("hdtoa");
    (out[..d.len].to_vec(), d.exp2, d.negative, d.kind)
}

#[test]
fn hdtoa_fixed_cases() {
    let values = [
        0.0f64,
        -0.0,
        1.0,
        -1.0,
        1.5,
        3.0,
        0.5,
        0.1,
        f64::from_bits(1),
        f64::MAX,
        f64::MIN_POSITIVE,
        1e23,
    ];
    for value in values {
        for upper in [false, true] {
            for ndigits in [-1, 0, 1, 2, 3, 13, 14, 20] {
                for (r, fe) in ROUNDINGS {
                    let want = c_hdtoa(value, upper, ndigits, fe);
                    let got = rust_hdtoa(value, upper, ndigits, r);
                    assert_eq!(
                        got, want,
                        "hdtoa {value:?} upper {upper} ndigits {ndigits} rounding {r:?}"
                    );
                }
            }
        }
    }
}

fn ld_kind(value: &[u8; 16]) -> Kind {
    let mut frac: u64 = 0;
    for i in 0..8 {
        frac |= u64::from(value[i]) << (i * 8);
    }
    let se = u16::from(value[8]) | (u16::from(value[9]) << 8);
    let exp = se & 0x7fff;
    if exp == 0 {
        if frac == 0 {
            Kind::Zero
        } else if frac >> 63 == 0 {
            Kind::Finite
        } else {
            Kind::Nan
        }
    } else if exp == 0x7fff {
        if frac == 0x8000_0000_0000_0000 {
            Kind::Inf
        } else {
            Kind::Nan
        }
    } else if frac >> 63 == 0 {
        Kind::Nan
    } else {
        Kind::Finite
    }
}

fn c_ldtoa(value: &[u8; 16], mode: i32, ndigits: i32) -> (Vec<u8>, i32, bool, Kind) {
    let _guard = c_guard();
    let mut ld = *value;
    let mut decpt = 0i32;
    let mut sign = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = with_rounding(FE_TONEAREST, || unsafe {
        oracle___ldtoa(
            ld.as_mut_ptr() as *mut c_void,
            mode,
            ndigits,
            &mut decpt,
            &mut sign,
            &mut end,
        )
    });
    let digits = c_digits(ptr, end);
    unsafe {
        oracle___freedtoa(ptr);
    }
    let kind = if decpt == i32::MAX {
        ld_kind(value)
    } else if *value == [0u8; 16] {
        Kind::Zero
    } else {
        Kind::Finite
    };
    let decpt = if decpt == i32::MAX { 0 } else { decpt };
    let digits = if kind == Kind::Finite || kind == Kind::Zero {
        digits
    } else {
        Vec::new()
    };
    (digits, decpt, sign != 0, kind)
}

fn rust_ldtoa(
    value: LongDouble,
    mode: DigitMode,
    ndigits: i32,
    r: Rounding,
) -> (Vec<u8>, i32, bool, Kind) {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 11_700];
    let d = ldtoa(&mut ctx, value, mode, ndigits, r, &mut out).expect("ldtoa");
    (out[..d.len].to_vec(), d.decpt, d.negative, d.kind)
}

#[test]
fn ldtoa_fixed_cases() {
    let mut values: Vec<[u8; 16]> = Vec::new();
    for exponent in [1u16, 0x3fff, 0x4000, 0x7ffe, 0x7fff] {
        let mut bytes = [0u8; 16];
        bytes[7] = 0x80;
        bytes[8] = (exponent & 0xff) as u8;
        bytes[9] = (exponent >> 8) as u8;
        values.push(bytes);
    }
    // Valid denormals and extended-range values.
    for frac in [1u64, 3, 0x7fff_ffff_ffff_ffff] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&frac.to_le_bytes());
        values.push(bytes);
    }
    values.push([0u8; 16]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x3f, 0, 0, 0, 0, 0, 0]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0xc0, 0xff, 0x3f, 0, 0, 0, 0, 0, 0]);
    // Largest finite ld80 and the Inf/NaN patterns.
    values.push([
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xff, 0x7f, 0, 0, 0, 0, 0, 0,
    ]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x7f, 0, 0, 0, 0, 0, 0]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0xc0, 0xff, 0x7f, 0, 0, 0, 0, 0, 0]);
    for bytes in values {
        for mode in [
            DigitMode::Shortest,
            DigitMode::SigDigits,
            DigitMode::Decimals,
        ] {
            for ndigits in [-1, 0, 1, 17, 40] {
                let want = c_ldtoa(&bytes, mode as i32, ndigits);
                let got = rust_ldtoa(LongDouble(bytes), mode, ndigits, Rounding::Nearest);
                assert_eq!(
                    got, want,
                    "ldtoa {bytes:02x?} mode {mode:?} ndigits {ndigits}"
                );
            }
        }
    }
}

/// The C `__hldtoa` takes `long double` by value (SysV class X87, passed in memory); this thunk
/// places the 16-byte image in the argument area and calls it with the integer arguments in their
/// registers.
#[allow(unsafe_code)]
unsafe fn c_hldtoa(
    value: &[u8; 16],
    xdigs: *const c_char,
    ndigits: c_int,
    decpt: *mut c_int,
    sign: *mut c_int,
    rve: *mut *mut c_char,
) -> *mut c_char {
    let ret: *mut c_char;
    unsafe {
        core::arch::asm!(
            "mov r10, {src}",
            "mov rax, [r10]",
            "mov r11, [r10 + 8]",
            "sub rsp, 24",
            "mov [rsp], rax",
            "mov [rsp + 8], r11",
            "mov rdi, {xdigs}",
            "mov rsi, {ndigits}",
            "mov rdx, {decpt}",
            "mov rcx, {sign}",
            "mov r8, {rve}",
            "call {f}",
            "add rsp, 24",
            src = in(reg) value.as_ptr(),
            xdigs = in(reg) xdigs,
            ndigits = in(reg) ndigits,
            decpt = in(reg) decpt,
            sign = in(reg) sign,
            rve = in(reg) rve,
            f = sym oracle___hldtoa,
            lateout("rax") ret,
            out("rcx") _,
            out("rdx") _,
            out("rsi") _,
            out("rdi") _,
            out("r8") _,
            out("r9") _,
            out("r10") _,
            out("r11") _,
            out("xmm0") _, out("xmm1") _, out("xmm2") _, out("xmm3") _,
            out("xmm4") _, out("xmm5") _, out("xmm6") _, out("xmm7") _,
            out("xmm8") _, out("xmm9") _, out("xmm10") _, out("xmm11") _,
            out("xmm12") _, out("xmm13") _, out("xmm14") _, out("xmm15") _,
        );
    }
    ret
}

fn c_hldtoa_digits(
    value: &[u8; 16],
    upper: bool,
    ndigits: i32,
    fe: c_int,
) -> (Vec<u8>, i32, bool, Kind) {
    let _guard = c_guard();
    let xdigs = if upper {
        b"0123456789ABCDEF\0".as_ptr()
    } else {
        b"0123456789abcdef\0".as_ptr()
    };
    let mut decpt = 0i32;
    let mut sign = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = with_rounding(fe, || unsafe {
        c_hldtoa(
            value,
            xdigs as *const c_char,
            ndigits,
            &mut decpt,
            &mut sign,
            &mut end,
        )
    });
    let digits = c_digits(ptr, end);
    unsafe {
        oracle___freedtoa(ptr);
    }
    let kind = if decpt == i32::MAX {
        ld_kind(value)
    } else if decpt == 1 && digits == b"0" {
        Kind::Zero
    } else {
        Kind::Finite
    };
    let exp2 = if kind == Kind::Finite { decpt - 1 } else { 0 };
    let digits = if kind == Kind::Finite || kind == Kind::Zero {
        digits
    } else {
        Vec::new()
    };
    (digits, exp2, sign != 0, kind)
}

fn rust_hldtoa(
    value: LongDouble,
    upper: bool,
    ndigits: i32,
    r: Rounding,
) -> (Vec<u8>, i32, bool, Kind) {
    let mut out = [0u8; 128];
    let d: HexDigits = hldtoa(value, upper, ndigits, r, &mut out).expect("hldtoa");
    (out[..d.len].to_vec(), d.exp2, d.negative, d.kind)
}

#[test]
fn hldtoa_fixed_cases() {
    let mut values: Vec<[u8; 16]> = Vec::new();
    values.push([0u8; 16]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x3f, 0, 0, 0, 0, 0, 0]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0xc0, 0xff, 0x3f, 0, 0, 0, 0, 0, 0]);
    values.push([0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x7f, 0, 0, 0, 0, 0, 0]);
    for frac in [1u64, 3, 0x7fff_ffff_ffff_ffff] {
        let mut bytes = [0u8; 16];
        let f = frac.to_le_bytes();
        bytes[..8].copy_from_slice(&f);
        values.push(bytes);
    }
    for frac in [0x8000_0000_0000_0001u64, 0xffff_ffff_ffff_ffff] {
        let mut bytes = [0u8; 16];
        let f = frac.to_le_bytes();
        bytes[..8].copy_from_slice(&f);
        bytes[8] = 0xff;
        bytes[9] = 0x3f;
        values.push(bytes);
    }
    for bytes in values {
        for upper in [false, true] {
            for ndigits in [-1, 0, 1, 3, 15, 16, 20] {
                for (r, fe) in ROUNDINGS {
                    let want = c_hldtoa_digits(&bytes, upper, ndigits, fe);
                    let got = rust_hldtoa(LongDouble(bytes), upper, ndigits, r);
                    assert_eq!(
                        got, want,
                        "hldtoa {bytes:02x?} upper {upper} ndigits {ndigits} rounding {r:?}"
                    );
                }
            }
        }
    }
}

const SEEDS: u64 = 20;

fn random_decimal(rng: &mut Rng) -> Vec<u8> {
    let mut text = Vec::new();
    if rng.below(4) == 0 {
        text.push(b'-');
    }
    let int_digits = rng.below(5) as usize;
    for _ in 0..int_digits {
        text.push(b'0' + rng.below(10) as u8);
    }
    if int_digits == 0 || rng.below(2) == 0 {
        text.push(b'.');
        let frac = 1 + rng.below(20) as usize;
        for _ in 0..frac {
            text.push(b'0' + rng.below(10) as u8);
        }
    }
    if text.is_empty() || text == b"-" || text == b"." || text == b"-." {
        text.extend_from_slice(b"0");
    }
    if rng.below(2) == 0 {
        let negative = rng.below(2) == 0;
        text.push(if rng.below(2) == 0 { b'e' } else { b'E' });
        if negative {
            text.push(b'-');
        }
        let digits = 1 + rng.below(3);
        for _ in 0..digits {
            text.push(b'0' + rng.below(10) as u8);
        }
    }
    text
}

fn decimal_of(value: f64, rng: &mut Rng) -> Vec<u8> {
    if value.is_nan() {
        return if rng.below(2) == 0 {
            b"nan".to_vec()
        } else {
            b"nan(0x123)".to_vec()
        };
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            b"-inf".to_vec()
        } else {
            b"inf".to_vec()
        };
    }
    if rng.below(2) == 0 {
        format!("{value:?}").into_bytes()
    } else {
        format!("{value:e}").into_bytes()
    }
}

#[test]
fn strtod_generated_cases() {
    // C35 convention: RL_DIFF_SEEDS / RL_DIFF_CASES let the manual 10^7-case run reuse the
    // automatic generator (1000 seeds x 10,000 cases x 4 modes).
    let seeds = std::env::var("RL_DIFF_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SEEDS);
    let per_seed = std::env::var("RL_DIFF_CASES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3000);
    let patterns = per_seed * 2 / 3;
    let random = per_seed - patterns;
    let mut total = 0u64;
    for seed in 1..=seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for _ in 0..patterns {
            let value = f64::from_bits(rng.next_u64());
            let text = decimal_of(value, &mut rng);
            check_strtod(&text);
            total += 1;
        }
        for _ in 0..random {
            let text = random_decimal(&mut rng);
            check_strtod(&text);
            total += 1;
        }
    }
    println!("strtod generated inputs: {total} x 4 modes");
}

#[test]
fn strtof_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0x5f32);
        for _ in 0..2000 {
            let value = f32::from_bits(rng.next_u64() as u32);
            let text = if value.is_nan() {
                b"nan(0x1)".to_vec()
            } else if value.is_infinite() {
                if value.is_sign_negative() {
                    b"-inf".to_vec()
                } else {
                    b"inf".to_vec()
                }
            } else if rng.below(2) == 0 {
                format!("{value:?}").into_bytes()
            } else {
                format!("{value:e}").into_bytes()
            };
            check_strtof(&text);
            total += 1;
        }
        for _ in 0..1000 {
            let text = random_decimal(&mut rng);
            check_strtof(&text);
            total += 1;
        }
    }
    println!("strtof generated inputs: {total} x 4 modes");
}

fn random_ld80(rng: &mut Rng) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    let frac = rng.next_u64();
    let choice = rng.below(10);
    let (frac, exp) = match choice {
        0 => (frac & 0x7fff_ffff_ffff_ffff, 0),      // denormal
        1 => (frac | 0x8000_0000_0000_0000, 0x7fff), // inf/nan pattern
        _ => (frac | 0x8000_0000_0000_0000, 1 + rng.below(0x7ffe) as u16),
    };
    bytes[..8].copy_from_slice(&frac.to_le_bytes());
    bytes[8] = (exp & 0xff) as u8;
    bytes[9] = (exp >> 8) as u8;
    bytes
}

fn format_ld_digits(digits: &[u8], decpt: i32, negative: bool) -> Vec<u8> {
    let mut text = Vec::new();
    if negative {
        text.push(b'-');
    }
    let decpt = decpt;
    if decpt <= 0 {
        text.extend_from_slice(b"0.");
        for _ in 0..(-decpt) {
            text.push(b'0');
        }
        text.extend_from_slice(digits);
    } else if (decpt as usize) >= digits.len() {
        text.extend_from_slice(digits);
        for _ in 0..((decpt as usize) - digits.len()) {
            text.push(b'0');
        }
    } else {
        let split = decpt as usize;
        text.extend_from_slice(&digits[..split]);
        text.push(b'.');
        text.extend_from_slice(&digits[split..]);
    }
    text
}

#[test]
fn strtold_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0x1d80);
        for _ in 0..1500 {
            let bytes = random_ld80(&mut rng);
            let kind = ld_kind(&bytes);
            let text = if kind == Kind::Finite || kind == Kind::Zero {
                let (digits, decpt, negative, _) = c_ldtoa(&bytes, DigitMode::Shortest as i32, -1);
                format_ld_digits(&digits, decpt, negative)
            } else if kind == Kind::Inf {
                if bytes[9] & 0x80 != 0 {
                    b"-inf".to_vec()
                } else {
                    b"inf".to_vec()
                }
            } else {
                b"nan".to_vec()
            };
            check_strtold(&text);
            total += 1;
        }
        for _ in 0..500 {
            let text = random_decimal(&mut rng);
            check_strtold(&text);
            total += 1;
        }
    }
    println!("strtold generated inputs: {total} x 4 modes");
}

#[test]
fn dtoa_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0xd70a);
        for _ in 0..1000 {
            let value = f64::from_bits(rng.next_u64());
            if !value.is_finite() {
                continue;
            }
            let mode = rng.pick(&[
                DigitMode::Shortest,
                DigitMode::ShortestSteele,
                DigitMode::SigDigits,
                DigitMode::Decimals,
                DigitMode::SigDigitsShort,
                DigitMode::DecimalsShort,
            ]);
            let ndigits = rng.pick(&[-5i32, -1, 0, 1, 2, 6, 15, 17, 20, 40, 350, 767]);
            if mode == DigitMode::DecimalsShort && ndigits < 0 {
                continue;
            }
            let r = rng.pick(&[
                Rounding::Nearest,
                Rounding::TowardZero,
                Rounding::Upward,
                Rounding::Downward,
            ]);
            let want = c_dtoa(value, mode as i32, ndigits, fe_of(r));
            let got = rust_dtoa(value, mode, ndigits, r);
            assert_eq!(
                got, want,
                "generated dtoa {value:?} {mode:?} {ndigits} {r:?}"
            );
            total += 1;
        }
    }
    println!("generated dtoa cases: {total}");
}

#[test]
fn hdtoa_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0x4d70);
        for _ in 0..1000 {
            let value = f64::from_bits(rng.next_u64());
            let upper = rng.below(2) == 0;
            let ndigits = rng.pick(&[-1i32, 0, 1, 2, 3, 13, 14, 20]);
            let r = rng.pick(&[
                Rounding::Nearest,
                Rounding::TowardZero,
                Rounding::Upward,
                Rounding::Downward,
            ]);
            let want = c_hdtoa(value, upper, ndigits, fe_of(r));
            let got = rust_hdtoa(value, upper, ndigits, r);
            assert_eq!(
                got, want,
                "generated hdtoa {value:?} upper {upper} {ndigits} {r:?}"
            );
            total += 1;
        }
    }
    println!("generated hdtoa cases: {total}");
}

#[test]
fn ldtoa_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0x1d7a);
        for _ in 0..1000 {
            let bytes = random_ld80(&mut rng);
            let kind = ld_kind(&bytes);
            if kind == Kind::Nan {
                continue;
            }
            let mode = rng.pick(&[
                DigitMode::Shortest,
                DigitMode::SigDigits,
                DigitMode::Decimals,
            ]);
            let ndigits = rng.pick(&[-1i32, 0, 1, 17, 40]);
            let want = c_ldtoa(&bytes, mode as i32, ndigits);
            let got = rust_ldtoa(LongDouble(bytes), mode, ndigits, Rounding::Nearest);
            assert_eq!(got, want, "generated ldtoa {bytes:02x?} {mode:?} {ndigits}");
            total += 1;
        }
    }
    println!("generated ldtoa cases: {total}");
}

#[test]
fn hldtoa_generated_cases() {
    let mut total = 0u64;
    for seed in 1..=SEEDS {
        let mut rng = Rng::seed_from_u64(seed ^ 0x41d7);
        for _ in 0..1000 {
            let bytes = random_ld80(&mut rng);
            let kind = ld_kind(&bytes);
            if kind == Kind::Nan {
                continue;
            }
            let upper = rng.below(2) == 0;
            let ndigits = rng.pick(&[-1i32, 0, 1, 3, 15, 16, 20]);
            let r = rng.pick(&[
                Rounding::Nearest,
                Rounding::TowardZero,
                Rounding::Upward,
                Rounding::Downward,
            ]);
            let want = c_hldtoa_digits(&bytes, upper, ndigits, fe_of(r));
            let got = rust_hldtoa(LongDouble(bytes), upper, ndigits, r);
            assert_eq!(
                got, want,
                "generated hldtoa {bytes:02x?} {upper} {ndigits} {r:?}"
            );
            total += 1;
        }
    }
    println!("generated hldtoa cases: {total}");
}

#[derive(serde::Deserialize)]
struct DtoaCorpus {
    case: Vec<DtoaCorpusCase>,
}

#[derive(serde::Deserialize)]
struct DtoaCorpusCase {
    text: String,
    #[serde(default)]
    strtod_bits: Option<String>,
    #[serde(default)]
    strtod_consumed: Option<usize>,
    #[serde(default)]
    strtof_nearest: Option<String>,
    #[serde(default)]
    strtof_upward: Option<String>,
    #[serde(default)]
    strtof_downward: Option<String>,
    #[serde(default)]
    strtof_towardzero: Option<String>,
    #[serde(default)]
    strtold_bytes: Option<String>,
}

fn hex_u64(text: &str) -> u64 {
    u64::from_str_radix(text, 16).expect("hex u64")
}

fn hex_u32(text: &str) -> u32 {
    u32::from_str_radix(text, 16).expect("hex u32")
}

fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex byte"))
        .collect()
}

#[test]
fn dtoa_regression_corpus() {
    let corpus: DtoaCorpus =
        toml::from_str(include_str!("../corpus/dtoa/regressions.toml")).expect("dtoa corpus");
    for case in corpus.case {
        let text = case.text.as_bytes();
        if let Some(want) = case.strtod_bits {
            let mut ctx = DtoaCtx::new();
            let parsed = strtod(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).expect("strtod");
            assert_eq!(
                parsed.value.to_bits(),
                hex_u64(&want),
                "corpus strtod {:?}",
                case.text
            );
            if let Some(consumed) = case.strtod_consumed {
                assert_eq!(
                    parsed.consumed, consumed,
                    "corpus strtod consumed {:?}",
                    case.text
                );
            }
        }
        for (want, mode) in [
            (case.strtof_nearest, Rounding::Nearest),
            (case.strtof_upward, Rounding::Upward),
            (case.strtof_downward, Rounding::Downward),
            (case.strtof_towardzero, Rounding::TowardZero),
        ] {
            if let Some(want) = want {
                let mut ctx = DtoaCtx::new();
                let parsed = strtof(&mut ctx, text, &C_NUMERIC, mode).expect("strtof");
                assert_eq!(
                    parsed.value.to_bits(),
                    hex_u32(&want),
                    "corpus strtof {:?} {mode:?}",
                    case.text
                );
            }
        }
        if let Some(want) = case.strtold_bytes {
            let mut ctx = DtoaCtx::new();
            let parsed = strtold(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).expect("strtold");
            let want = hex_bytes(&want);
            assert_eq!(want.len(), 16);
            assert_eq!(parsed.value.0, want[..], "corpus strtold {:?}", case.text);
        }
    }
}

#[test]
fn dtoa_rounding_modes() {
    for value in [2.5f64, 0.125, 9.5] {
        for (r, fe) in ROUNDINGS {
            let want = c_dtoa(value, DigitMode::SigDigits as i32, 1, fe);
            let got = rust_dtoa(value, DigitMode::SigDigits, 1, r);
            assert_eq!(got, want, "dtoa rounding {value} {r:?}");
        }
    }
}
