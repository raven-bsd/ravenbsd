// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exhaustive binary32 digit/parse check, sharded for a full 2^32 run.
//!
//! `RL_DTOA_SHARD=<i>/<n>` selects a contiguous shard (default `0/1`), `RL_DTOA_THREADS=<k>`
//! the worker count (default: available parallelism). Run in release mode:
//!
//! ```text
//! LIBRARY_PATH=<dir> LD_LIBRARY_PATH=<dir> RL_DTOA_SHARD=0/64 \
//!   cargo test -p raven-libc-oracle --release --features oracle-dtoa \
//!   --test exhaustive_dtoa_f32 -- --ignored --nocapture
//! ```
//!
//! The C gdtoa keeps process-global free/power-of-5 lists without working locks in this shim, so
//! every C call is serialized; the Rust work (two `ftoa`s, one `dtoa`, two `strtof`s and the
//! comparisons) runs on all workers.  The reference is the real C `__gdtoa`/`__dtoa`; even
//! binary32 is exercised through the generic core, never through the C `strtof` (whose directed
//! path is defective, see `diff_dtoa.rs`).

#![cfg(feature = "oracle-dtoa")]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::undocumented_unsafe_blocks,
    clippy::too_many_arguments
)]

use core::ffi::{c_char, c_int};

use raven_libc_dtoa::{
    C_NUMERIC, DigitMode, DtoaCtx, Kind, MAX_DIGITS_F64, Rounding, dtoa, ftoa, strtof,
};
use raven_libc_oracle::decls::dtoa::{CFpi, oracle___dtoa, oracle___freedtoa, oracle___gdtoa};

// glibc's FE_TONEAREST; calling fesetround also makes the test binary link libm, which the
// oracle shared object needs for its own fegetround import.

/// The NetBSD free lists and `p5s` are not thread-safe with this shim's no-op mutexes.
static C_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn c_guard() -> std::sync::MutexGuard<'static, ()> {
    C_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
const FE_TONEAREST: c_int = 0;
unsafe extern "C" {
    fn fesetround(mode: c_int) -> c_int;
}

struct CResult {
    len: usize,
    decpt: i32,
    negative: bool,
    kind: Kind,
}

fn c_gdtoa_f32(value: f32, mode: i32, ndigits: i32) -> ([u8; 32], CResult) {
    let _guard = c_guard();
    let mut out = [0u8; 32];
    let raw = value.to_bits();
    let sign = raw & 0x8000_0000 != 0;
    let abs = raw & 0x7fff_ffff;
    if abs & 0x7f80_0000 == 0x7f80_0000 {
        let kind = if abs & 0x007f_ffff != 0 {
            Kind::Nan
        } else {
            Kind::Inf
        };
        return (
            out,
            CResult {
                len: 0,
                decpt: 0,
                negative: sign,
                kind,
            },
        );
    }
    if abs == 0 {
        out[0] = b'0';
        return (
            out,
            CResult {
                len: 1,
                decpt: 1,
                negative: sign,
                kind: Kind::Zero,
            },
        );
    }
    let mut bits = [abs & 0x007f_ffff];
    let mut be = ((abs >> 23) & 0xff) as i32;
    if be != 0 {
        bits[0] |= 0x0080_0000;
    } else {
        be = 1;
    }
    be -= 0x7f + 23;
    let fpi = CFpi {
        nbits: 24,
        emin: 1 - 127 - 24 + 1,
        emax: 254 - 127 - 24 + 1,
        rounding: 1,
        sudden_underflow: 0,
    };
    let mut kind = if sign { 0x09 } else { 0x01 };
    let mut decpt = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = unsafe {
        oracle___gdtoa(
            &fpi,
            be,
            bits.as_mut_ptr(),
            &mut kind,
            mode,
            ndigits,
            &mut decpt,
            &mut end,
        )
    };
    let start = ptr as usize;
    let len = (end as usize).saturating_sub(start);
    assert!(len <= out.len(), "C f32 digit buffer overflow: {len}");
    unsafe {
        core::ptr::copy_nonoverlapping(ptr as *const u8, out.as_mut_ptr(), len);
        oracle___freedtoa(ptr);
    }
    (
        out,
        CResult {
            len,
            decpt,
            negative: sign,
            kind: Kind::Finite,
        },
    )
}

fn c_dtoa_f64(value: f64, mode: i32, ndigits: i32) -> ([u8; 48], CResult) {
    let _guard = c_guard();
    let mut out = [0u8; 48];
    let mut decpt = 0i32;
    let mut sign = 0i32;
    let mut end: *mut c_char = core::ptr::null_mut();
    let ptr = unsafe { oracle___dtoa(value, mode, ndigits, &mut decpt, &mut sign, &mut end) };
    let start = ptr as usize;
    let len = (end as usize).saturating_sub(start);
    let kind = if value == 0.0 {
        Kind::Zero
    } else if decpt == 9999 {
        if len == 8 && unsafe { core::slice::from_raw_parts(ptr as *const u8, len) } == b"Infinity"
        {
            Kind::Inf
        } else {
            Kind::Nan
        }
    } else {
        Kind::Finite
    };
    let mut copy_len = len;
    if kind == Kind::Inf || kind == Kind::Nan {
        copy_len = 0;
    }
    assert!(
        copy_len <= out.len(),
        "C f64 digit buffer overflow: {copy_len}"
    );
    unsafe {
        core::ptr::copy_nonoverlapping(ptr as *const u8, out.as_mut_ptr(), copy_len);
        oracle___freedtoa(ptr);
    }
    (
        out,
        CResult {
            len: copy_len,
            decpt: if decpt == 9999 { 0 } else { decpt },
            negative: sign != 0,
            kind,
        },
    )
}

fn push_i32(out: &mut [u8], pos: &mut usize, value: i32) {
    if value < 0 {
        out[*pos] = b'-';
        *pos += 1;
    }
    let mut v = i64::from(value).unsigned_abs();
    let mut tmp = [0u8; 20];
    let mut n = 0usize;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        out[*pos] = tmp[n];
        *pos += 1;
    }
}

fn build_text(out: &mut [u8], digits: &[u8], decpt: i32, negative: bool) -> usize {
    let mut n = 0usize;
    if negative {
        out[n] = b'-';
        n += 1;
    }
    if digits.is_empty() {
        out[n] = b'0';
        n += 1;
    } else {
        out[n] = digits[0];
        n += 1;
        if digits.len() > 1 {
            out[n] = b'.';
            n += 1;
            out[n..n + digits.len() - 1].copy_from_slice(&digits[1..]);
            n += digits.len() - 1;
        }
    }
    out[n] = b'e';
    n += 1;
    push_i32(out, &mut n, decpt - 1);
    n
}

fn check_shortest(ctx: &mut DtoaCtx, value: f32, text: &mut [u8; 96]) {
    let mut rust_out = [0u8; 32];
    let d = ftoa(
        ctx,
        value,
        DigitMode::Shortest,
        -1,
        Rounding::Nearest,
        &mut rust_out,
    )
    .expect("ftoa shortest");
    let (c_out, c) = c_gdtoa_f32(value, DigitMode::Shortest as i32, -1);
    assert_eq!(d.len, c.len, "shortest len for {value:?}");
    assert_eq!(
        &rust_out[..d.len],
        &c_out[..c.len],
        "shortest digits for {value:?}"
    );
    assert_eq!(d.decpt, c.decpt, "shortest decpt for {value:?}");
    assert_eq!(d.negative, c.negative, "shortest sign for {value:?}");
    assert_eq!(d.kind, c.kind, "shortest kind for {value:?}");
    let n = build_text(text, &rust_out[..d.len], d.decpt, d.negative);
    let parsed = strtof(ctx, &text[..n], &C_NUMERIC, Rounding::Nearest).expect("strtof shortest");
    assert_eq!(parsed.consumed, n, "shortest consumed for {value:?}");
    assert_eq!(
        parsed.value.to_bits(),
        value.to_bits(),
        "shortest round trip for {value:?}"
    );
}

fn check_nine(ctx: &mut DtoaCtx, value: f32, text: &mut [u8; 96]) {
    let mut rust_out = [0u8; 32];
    let d = ftoa(
        ctx,
        value,
        DigitMode::SigDigits,
        9,
        Rounding::Nearest,
        &mut rust_out,
    )
    .expect("ftoa 9 digits");
    let (c_out, c) = c_gdtoa_f32(value, DigitMode::SigDigits as i32, 9);
    assert_eq!(
        &rust_out[..d.len],
        &c_out[..c.len],
        "9-digit digits for {value:?}"
    );
    assert_eq!(d.decpt, c.decpt, "9-digit decpt for {value:?}");
    assert_eq!(d.negative, c.negative, "9-digit sign for {value:?}");
    let n = build_text(text, &rust_out[..d.len], d.decpt, d.negative);
    let parsed = strtof(ctx, &text[..n], &C_NUMERIC, Rounding::Nearest).expect("strtof 9 digits");
    assert_eq!(
        parsed.value.to_bits(),
        value.to_bits(),
        "9-digit round trip for {value:?}"
    );
}

fn check_dtoa_f64(ctx: &mut DtoaCtx, value: f32, text: &mut [u8; 96]) {
    let v = f64::from(value);
    let mut rust_out = [0u8; MAX_DIGITS_F64];
    let d = dtoa(
        ctx,
        v,
        DigitMode::Shortest,
        -1,
        Rounding::Nearest,
        &mut rust_out,
    )
    .expect("dtoa shortest");
    let (c_out, c) = c_dtoa_f64(v, DigitMode::Shortest as i32, -1);
    assert_eq!(
        &rust_out[..d.len],
        &c_out[..c.len],
        "dtoa(f64) digits for {value:?}"
    );
    assert_eq!(d.decpt, c.decpt, "dtoa(f64) decpt for {value:?}");
    assert_eq!(d.negative, c.negative, "dtoa(f64) sign for {value:?}");
    assert_eq!(d.kind, c.kind, "dtoa(f64) kind for {value:?}");
    if d.kind == Kind::Zero {
        return;
    }
    let n = build_text(text, &rust_out[..d.len], d.decpt, d.negative);
    let parsed = strtof(ctx, &text[..n], &C_NUMERIC, Rounding::Nearest).expect("strtof dtoa");
    assert_eq!(
        parsed.value.to_bits(),
        value.to_bits(),
        "dtoa(f64) round trip for {value:?}"
    );
}

fn check_one(ctx: &mut DtoaCtx, bits: u32, text: &mut [u8; 96]) {
    let value = f32::from_bits(bits);
    let sign = bits & 0x8000_0000 != 0;
    if !value.is_finite() {
        let mut out = [0u8; 32];
        let d = ftoa(
            ctx,
            value,
            DigitMode::Shortest,
            -1,
            Rounding::Nearest,
            &mut out,
        )
        .expect("ftoa non-finite");
        let (_, c) = c_gdtoa_f32(value, DigitMode::Shortest as i32, -1);
        assert_eq!(d.kind, c.kind, "non-finite kind for {bits:08x}");
        assert_eq!(d.negative, c.negative, "non-finite sign for {bits:08x}");
        assert_eq!(d.len, 0);
        return;
    }
    if value == 0.0 {
        let mut out = [0u8; 32];
        let d = ftoa(
            ctx,
            value,
            DigitMode::Shortest,
            -1,
            Rounding::Nearest,
            &mut out,
        )
        .expect("ftoa zero");
        assert_eq!(d.len, 1);
        assert_eq!(out[0], b'0');
        assert_eq!(d.decpt, 1);
        assert_eq!(d.negative, sign);
        return;
    }
    // The pattern sweep visits both signs, so the negative twin is covered when its own bit
    // pattern comes up; no second pass over every magnitude is needed.
    let _ = sign;
    check_shortest(ctx, value, text);
    check_nine(ctx, value, text);
    check_dtoa_f64(ctx, value, text);
}

fn parse_shard() -> (u64, u64) {
    let spec = std::env::var("RL_DTOA_SHARD").unwrap_or_else(|_| "0/1".into());
    let mut parts = spec.split('/');
    let index: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .expect("RL_DTOA_SHARD index");
    let count: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .expect("RL_DTOA_SHARD count");
    assert!(count > 0 && index < count, "invalid RL_DTOA_SHARD={spec}");
    (index, count)
}

#[test]
#[ignore = "exhaustive 2^32 run; release only, sharded"]
fn exhaustive_f32_all_patterns() {
    let (shard, shards) = parse_shard();
    let total = 1u64 << 32;
    let start = total * shard / shards;
    let end = total * (shard + 1) / shards;
    let threads = std::env::var("RL_DTOA_THREADS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get() as u64)
                .unwrap_or(1)
        });
    let chunk = (end - start).div_ceil(threads);
    let checked = std::sync::atomic::AtomicU64::new(0);
    let clock = std::time::Instant::now();
    std::thread::scope(|scope| {
        for worker in 0..threads {
            let lo = start + chunk * worker;
            let hi = (lo + chunk).min(end);
            if lo >= hi {
                break;
            }
            let checked = &checked;
            scope.spawn(move || {
                unsafe {
                    fesetround(FE_TONEAREST);
                }
                let mut ctx = DtoaCtx::new();
                let mut text = [0u8; 96];
                for bits in lo..hi {
                    check_one(&mut ctx, bits as u32, &mut text);
                    checked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
    });
    let elapsed = clock.elapsed();
    let n = checked.load(std::sync::atomic::Ordering::Relaxed);
    println!(
        "shard {shard}/{shards}: {n} patterns in {:.1}s with {threads} threads ({:.0} patterns/s)",
        elapsed.as_secs_f64(),
        n as f64 / elapsed.as_secs_f64()
    );
}
