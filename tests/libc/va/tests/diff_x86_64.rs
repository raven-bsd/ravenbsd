// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The x86_64 differential: our trampolines against the host glibc, through a C driver
//! generated at test time (layers A-I of C59 section 7.1).
//!
//! The generator emits one reference and one "ours" function per argument SHAPE; the format
//! and the values travel as parameters, so the calling convention is fixed by the shape and
//! the conversion by the format string. `RD-new 9` in the report records this deviation from
//! the ticket's one-function-per-case wording (the K03 corpus keeps one function per case).

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/driver.rs"]
mod driver;
#[path = "../src/table_ut.rs"]
mod table;
#[path = "../src/targets.rs"]
mod targets;

use std::ffi::c_void;

use driver::{Case, CaseArg, CaseDriver, Driver, ScanCase, ScanDriver};
use raven_libc_oracle::rng::Rng;

unsafe extern "C" {
    fn t_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
    fn t_sscanf(input: *const u8, fmt: *const u8, ...) -> i32;
    fn t_rec(selector: usize, ap: *mut u8) -> i32;
}

type RefI32 = unsafe extern "C" fn(*const u8, *mut u8, usize, *const i32, i32) -> i32;
type OursI32 = unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const i32, i32) -> i32;
type RefI64 = unsafe extern "C" fn(*const u8, *mut u8, usize, *const i64, i32) -> i32;
type OursI64 = unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const i64, i32) -> i32;
type RefDbl = unsafe extern "C" fn(*const u8, *mut u8, usize, *const u64, i32) -> i32;
type OursDbl = unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const u64, i32) -> i32;
type RefMix = unsafe extern "C" fn(*const u8, *mut u8, usize, *const i64, *const u64) -> i32;
type OursMix =
    unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const i64, *const u64) -> i32;
type RefLd = unsafe extern "C" fn(*const u8, *mut u8, usize, *const i64, i32, *const u8) -> i32;
type OursLd =
    unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const i64, i32, *const u8) -> i32;
type RefPtr = unsafe extern "C" fn(*const u8, *mut u8, usize, *const c_void) -> i32;
type OursPtr = unsafe extern "C" fn(usize, *const u8, *mut u8, usize, *const c_void) -> i32;
type RefPctN = unsafe extern "C" fn(*const u8, *mut u8, usize, i32, *mut i32) -> i32;
type OursPctN = unsafe extern "C" fn(usize, *const u8, *mut u8, usize, i32, *mut i32) -> i32;
type OursAgg = unsafe extern "C" fn(usize, *const u8) -> i32;

/// The shape functions of one loaded driver.
struct Lib {
    _driver: Driver,
    ref_i32: RefI32,
    ours_i32: OursI32,
    ref_i64: RefI64,
    ours_i64: OursI64,
    ref_dbl: RefDbl,
    ours_dbl: OursDbl,
    ref_mix6: RefMix,
    ours_mix6: OursMix,
    ref_mix68: RefMix,
    ours_mix68: OursMix,
    ref_mix79: RefMix,
    ours_mix79: OursMix,
    ref_ld: RefLd,
    ours_ld: OursLd,
    ref_ptr: RefPtr,
    ours_ptr: OursPtr,
    ref_pctn: RefPctN,
    ours_pctn: OursPctN,
    ours_agg8: OursAgg,
    ours_agg12: OursAgg,
    ours_agg16: OursAgg,
    ours_agg24: OursAgg,
    ours_agg32: OursAgg,
}

impl Lib {
    fn load() -> Option<Self> {
        let driver = match Driver::build() {
            Ok(Some(driver)) => driver,
            Ok(None) => {
                println!("skipped: clang not found");
                return None;
            }
            Err(error) => panic!("driver build failed: {error}"),
        };
        // SAFETY: every symbol below is generated with the matching signature.
        unsafe {
            Some(Self {
                ref_i32: driver.func("ref_i32").expect("ref_i32"),
                ours_i32: driver.func("ours_i32").expect("ours_i32"),
                ref_i64: driver.func("ref_i64").expect("ref_i64"),
                ours_i64: driver.func("ours_i64").expect("ours_i64"),
                ref_dbl: driver.func("ref_dbl").expect("ref_dbl"),
                ours_dbl: driver.func("ours_dbl").expect("ours_dbl"),
                ref_mix6: driver.func("ref_mix6").expect("ref_mix6"),
                ours_mix6: driver.func("ours_mix6").expect("ours_mix6"),
                ref_mix68: driver.func("ref_mix68").expect("ref_mix68"),
                ours_mix68: driver.func("ours_mix68").expect("ours_mix68"),
                ref_mix79: driver.func("ref_mix79").expect("ref_mix79"),
                ours_mix79: driver.func("ours_mix79").expect("ours_mix79"),
                ref_ld: driver.func("ref_ld").expect("ref_ld"),
                ours_ld: driver.func("ours_ld").expect("ours_ld"),
                ref_ptr: driver.func("ref_ptr").expect("ref_ptr"),
                ours_ptr: driver.func("ours_ptr").expect("ours_ptr"),
                ref_pctn: driver.func("ref_pctn").expect("ref_pctn"),
                ours_pctn: driver.func("ours_pctn").expect("ours_pctn"),
                ours_agg8: driver.func("ours_agg8").expect("ours_agg8"),
                ours_agg12: driver.func("ours_agg12").expect("ours_agg12"),
                ours_agg16: driver.func("ours_agg16").expect("ours_agg16"),
                ours_agg24: driver.func("ours_agg24").expect("ours_agg24"),
                ours_agg32: driver.func("ours_agg32").expect("ours_agg32"),
                _driver: driver,
            })
        }
    }

    fn trampoline(&self) -> usize {
        t_snprintf as *const () as usize
    }
}

/// Compare one formatted call: return value and every buffer byte.
fn compare<FR, FO>(format: &[u8], reference: FR, ours: FO) -> bool
where
    FR: FnOnce(*mut u8, usize) -> i32,
    FO: FnOnce(*mut u8, usize) -> i32,
{
    let mut reference_buffer = [0u8; 512];
    let mut ours_buffer = [0u8; 512];
    let reference_length = reference(reference_buffer.as_mut_ptr(), reference_buffer.len());
    let ours_length = ours(ours_buffer.as_mut_ptr(), ours_buffer.len());
    if reference_length != ours_length || reference_buffer != ours_buffer {
        println!(
            "MISMATCH {format:?}: ref {reference_length} {:?} / ours {ours_length} {:?}",
            String::from_utf8_lossy(&reference_buffer[..64]),
            String::from_utf8_lossy(&ours_buffer[..64])
        );
        return false;
    }
    true
}

fn c_format(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

// ---------------------------------------------------------------------------------------
// Layer A: integer shapes
// ---------------------------------------------------------------------------------------

#[test]
fn layer_a_integer_shapes() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let pool = [0i32, 1, -1, i32::MIN, i32::MAX, 42, -42, 1_234_567];
    let conversions = [
        "%d", "%i", "%u", "%x", "%o", "%c", "%8d", "%-6d", "%08x", "%.5d",
    ];
    let mut rng = Rng::seed_from_u64(0xA11);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for count in 0..=12usize {
        for _ in 0..160 {
            let format = build_format(&mut rng, count, &conversions);
            let mut values = [0i32; 12];
            for slot in values.iter_mut().take(count) {
                *slot = rng.pick(&pool);
            }
            let c = c_format(&format);
            let ok;
            // SAFETY: the shape passes `count` ints and the format uses at most that many.
            unsafe {
                ok = compare(
                    c.as_slice(),
                    |buffer, n| (lib.ref_i32)(c.as_ptr(), buffer, n, values.as_ptr(), count as i32),
                    |buffer, n| {
                        (lib.ours_i32)(
                            ours_fn,
                            c.as_ptr(),
                            buffer,
                            n,
                            values.as_ptr(),
                            count as i32,
                        )
                    },
                );
            }
            if !ok {
                mismatches += 1;
            }
            cases += 1;
        }
    }
    println!("layer A: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
    assert!(cases >= 2_000);
}

#[test]
fn layer_a_long_shapes() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let pool = [
        0i64,
        1,
        -1,
        i64::MIN,
        i64::MAX,
        9_000_000_000,
        -9_000_000_000,
    ];
    let conversions = [
        "%ld", "%lu", "%lld", "%llx", "%zu", "%jd", "%8ld", "%-20lld",
    ];
    let mut rng = Rng::seed_from_u64(0xA12);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for count in 1..=12usize {
        for _ in 0..120 {
            let format = build_format(&mut rng, count, &conversions);
            let mut values = [0i64; 12];
            for slot in values.iter_mut().take(count) {
                *slot = rng.pick(&pool);
            }
            let c = c_format(&format);
            let ok;
            // SAFETY: the shape passes `count` longs and the format uses at most that many.
            unsafe {
                ok = compare(
                    c.as_slice(),
                    |buffer, n| (lib.ref_i64)(c.as_ptr(), buffer, n, values.as_ptr(), count as i32),
                    |buffer, n| {
                        (lib.ours_i64)(
                            ours_fn,
                            c.as_ptr(),
                            buffer,
                            n,
                            values.as_ptr(),
                            count as i32,
                        )
                    },
                );
            }
            if !ok {
                mismatches += 1;
            }
            cases += 1;
        }
    }
    println!("layer A (long): {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
}

fn build_format(rng: &mut Rng, count: usize, conversions: &[&str]) -> String {
    let mut format = String::new();
    for index in 0..count {
        if index != 0 {
            format.push(' ');
        }
        format.push_str(rng.pick(conversions));
    }
    if count == 0 {
        format.push_str("literal");
    }
    format
}

// ---------------------------------------------------------------------------------------
// Layer B: float shapes
// ---------------------------------------------------------------------------------------

#[test]
fn layer_b_float_shapes() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let pool = [
        0.0f64,
        -0.0,
        1.5,
        -2.25,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        0.1,
        9.995,
        123_456_789.0,
        f64::MAX,
        f64::MIN_POSITIVE,
        1.0 / 3.0,
        1e22,
    ];
    let conversions = [
        "%f", "%e", "%g", "%a", "%F", "%E", "%G", "%A", "%.0f", "%.17g", "%12.4e",
    ];
    let mut rng = Rng::seed_from_u64(0xB22);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for count in 0..=12usize {
        for _ in 0..240 {
            let format = build_format(&mut rng, count, &conversions);
            let mut values = [0u64; 12];
            for slot in values.iter_mut().take(count) {
                *slot = rng.pick(&pool).to_bits();
            }
            let c = c_format(&format);
            let ok;
            // SAFETY: the shape passes `count` doubles by bit image.
            unsafe {
                ok = compare(
                    c.as_slice(),
                    |buffer, n| (lib.ref_dbl)(c.as_ptr(), buffer, n, values.as_ptr(), count as i32),
                    |buffer, n| {
                        (lib.ours_dbl)(
                            ours_fn,
                            c.as_ptr(),
                            buffer,
                            n,
                            values.as_ptr(),
                            count as i32,
                        )
                    },
                );
            }
            if !ok {
                mismatches += 1;
            }
            cases += 1;
        }
    }
    println!("layer B: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
    assert!(cases >= 3_000);
}

#[test]
fn layer_b_mixed_register_boundaries() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let mut rng = Rng::seed_from_u64(0xB23);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for _ in 0..300 {
        let mut ints = [0i64; 7];
        for slot in ints.iter_mut() {
            *slot = rng.next_u64() as i64 % 1000;
        }
        let mut doubles = [0u64; 9];
        for slot in doubles.iter_mut() {
            *slot = ((rng.next_u64() % 100_000) as f64 / 7.0).to_bits();
        }
        let format = "%d:%f %d:%f %d:%f %d:%f %d:%f %d:%f";
        let c = c_format(format);
        let six = 6usize;
        // SAFETY: the shape passes six ints and six doubles.
        let ok = unsafe {
            compare(
                c.as_slice(),
                |buffer, n| (lib.ref_mix6)(c.as_ptr(), buffer, n, ints.as_ptr(), doubles.as_ptr()),
                |buffer, n| {
                    (lib.ours_mix6)(
                        ours_fn,
                        c.as_ptr(),
                        buffer,
                        n,
                        ints.as_ptr(),
                        doubles.as_ptr(),
                    )
                },
            )
        };
        if !ok {
            mismatches += 1;
        }
        cases += 1;

        let format = "%d %d %d %d %d %d %f %f %f %f %f %f %f %f";
        let c = c_format(format);
        // SAFETY: the shape passes six ints and eight doubles.
        let ok = unsafe {
            compare(
                c.as_slice(),
                |buffer, n| (lib.ref_mix68)(c.as_ptr(), buffer, n, ints.as_ptr(), doubles.as_ptr()),
                |buffer, n| {
                    (lib.ours_mix68)(
                        ours_fn,
                        c.as_ptr(),
                        buffer,
                        n,
                        ints.as_ptr(),
                        doubles.as_ptr(),
                    )
                },
            )
        };
        if !ok {
            mismatches += 1;
        }
        cases += 1;

        let format = "%d %d %d %d %d %d %d %f %f %f %f %f %f %f %f %f";
        let c = c_format(format);
        // SAFETY: the shape passes seven ints and nine doubles.
        let ok = unsafe {
            compare(
                c.as_slice(),
                |buffer, n| (lib.ref_mix79)(c.as_ptr(), buffer, n, ints.as_ptr(), doubles.as_ptr()),
                |buffer, n| {
                    (lib.ours_mix79)(
                        ours_fn,
                        c.as_ptr(),
                        buffer,
                        n,
                        ints.as_ptr(),
                        doubles.as_ptr(),
                    )
                },
            )
        };
        if !ok {
            mismatches += 1;
        }
        cases += 1;
        let _ = six;
    }
    println!("layer B (mixed): {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
}

// ---------------------------------------------------------------------------------------
// Layer C: long double at every position
// ---------------------------------------------------------------------------------------

#[test]
fn layer_c_long_double_shapes() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let pool: [u128; 10] = [
        0,
        0x3fff_c000_0000_0000_0000, // 1.5
        0xc000_9000_0000_0000_0000, // -2.25
        0x3fff_f000_0000_0000_0000, // 1.0
        0x4000_c90f_daa2_2168_0000,
        0x3fff_8000_0000_0000_0000,
        0x7fff_8000_0000_0000_0000, // inf
        0x7fff_c000_0000_0000_0000, // nan
        0x4000_921f_b544_42d1_8000, // pi (approx)
        0x3fff_a000_0000_0000_0000,
    ];
    let conversions = ["%Lf", "%Le", "%Lg", "%La", "%.0Lf", "%.17Lg"];
    let mut rng = Rng::seed_from_u64(0xC33);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for position in 0..=6usize {
        for _ in 0..240 {
            let mut ints = [0i64; 6];
            for slot in ints.iter_mut() {
                *slot = rng.next_u64() as i64 % 100;
            }
            let value = rng.pick(&pool);
            let image = value.to_le_bytes();
            let formula = rng.pick(&conversions);
            let mut format = String::new();
            for index in 0..=6usize {
                if index == position {
                    if index != 0 {
                        format.push(' ');
                    }
                    format.push_str(formula);
                    continue;
                }
                if !format.is_empty() {
                    format.push(' ');
                }
                format.push_str("%d");
            }
            let c = c_format(&format);
            // SAFETY: the shape passes six ints and one long double at `position`.
            let ok = unsafe {
                compare(
                    c.as_slice(),
                    |buffer, n| {
                        (lib.ref_ld)(
                            c.as_ptr(),
                            buffer,
                            n,
                            ints.as_ptr(),
                            position as i32,
                            image.as_ptr(),
                        )
                    },
                    |buffer, n| {
                        (lib.ours_ld)(
                            ours_fn,
                            c.as_ptr(),
                            buffer,
                            n,
                            ints.as_ptr(),
                            position as i32,
                            image.as_ptr(),
                        )
                    },
                )
            };
            if !ok {
                mismatches += 1;
            }
            cases += 1;
        }
    }
    println!("layer C: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
    assert!(cases >= 1_500);
}

// ---------------------------------------------------------------------------------------
// Layer D: pointers, strings and %n
// ---------------------------------------------------------------------------------------

#[test]
fn layer_d_pointers_strings_and_count() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let strings = [
        "",
        "a",
        "abcdef",
        "a rather long string that will be truncated by the precision",
    ];
    let formats = [
        "%s", "%10s", "%-10s|", "%.3s", "%10.5s", "%p", "%20p", "%-20p|",
    ];
    let mut rng = Rng::seed_from_u64(0xD44);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for _ in 0..1_000 {
        let format = rng.pick(&formats);
        let text = rng.pick(&strings);
        let mut storage = text.as_bytes().to_vec();
        storage.push(0);
        let c = c_format(format);
        let pointer = storage.as_ptr() as *const c_void;
        // SAFETY: the shape passes one pointer; `%p` prints its value, `%s` reads it.
        let ok = unsafe {
            compare(
                c.as_slice(),
                |buffer, n| (lib.ref_ptr)(c.as_ptr(), buffer, n, pointer),
                |buffer, n| (lib.ours_ptr)(ours_fn, c.as_ptr(), buffer, n, pointer),
            )
        };
        if !ok {
            mismatches += 1;
        }
        cases += 1;
    }
    // NULL `%s` prints `(null)` in both; NULL `%p` is the documented glibc difference.
    let c = c_format("%s");
    // SAFETY: `%s` of NULL is handled by the engine before it dereferences.
    let ok = unsafe {
        compare(
            c.as_slice(),
            |buffer, n| (lib.ref_ptr)(c.as_ptr(), buffer, n, core::ptr::null()),
            |buffer, n| (lib.ours_ptr)(ours_fn, c.as_ptr(), buffer, n, core::ptr::null()),
        )
    };
    cases += 1;
    if !ok {
        mismatches += 1;
    }
    // `%n` stores into the same int through both sides.
    for format in ["%d%n", "%d %n", "%5d%n", "prefix %d%n"] {
        for value in [0i32, -1, 12345, i32::MAX] {
            let c = c_format(format);
            let mut reference_out = -1i32;
            let mut ours_out = -1i32;
            let mut reference_buffer = [0u8; 128];
            let mut ours_buffer = [0u8; 128];
            // SAFETY: the shape passes one int and the `%n` destination.
            let reference = unsafe {
                (lib.ref_pctn)(
                    c.as_ptr(),
                    reference_buffer.as_mut_ptr(),
                    reference_buffer.len(),
                    value,
                    &mut reference_out,
                )
            };
            // SAFETY: as above.
            let ours = unsafe {
                (lib.ours_pctn)(
                    ours_fn,
                    c.as_ptr(),
                    ours_buffer.as_mut_ptr(),
                    ours_buffer.len(),
                    value,
                    &mut ours_out,
                )
            };
            assert_eq!(ours, reference, "%n return value for {format:?}");
            assert_eq!(ours_buffer, reference_buffer, "%n output for {format:?}");
            assert_eq!(ours_out, reference_out, "%n stored count for {format:?}");
            cases += 1;
        }
    }
    println!("layer D: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
    assert!(cases >= 1_000);
}

// ---------------------------------------------------------------------------------------
// Layer E: integer-class structs by value
// ---------------------------------------------------------------------------------------

#[test]
fn layer_e_struct_by_value() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = t_rec as *const () as usize;
    let sizes = [8usize, 12, 16, 24, 32];
    let mut rng = Rng::seed_from_u64(0xE55);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for size in sizes {
        let shape: OursAgg = match size {
            8 => lib.ours_agg8,
            12 => lib.ours_agg12,
            16 => lib.ours_agg16,
            24 => lib.ours_agg24,
            _ => lib.ours_agg32,
        };
        for position in 0..=20usize {
            for _ in 0..5 {
                let mut bytes = vec![0u8; size];
                for byte in &mut bytes {
                    *byte = rng.next_u64() as u8;
                }
                targets::reset_aggregates();
                // SAFETY: the shape passes the struct image by value; the trampoline's target
                // reads exactly `size` bytes of integer-class aggregate.
                let result = unsafe { shape(ours_fn, bytes.as_ptr()) };
                let recorded = targets::take_aggregates();
                assert_eq!(result, 0, "aggregate {size} at position {position}");
                assert_eq!(recorded.len(), 1, "aggregate {size} was not recorded");
                assert_eq!(recorded[0].0, size);
                if recorded[0].1 != bytes {
                    println!(
                        "MISMATCH aggregate {size} at position {position}: {:?} != {:?}",
                        recorded[0].1, bytes
                    );
                    mismatches += 1;
                }
                cases += 1;
            }
        }
    }
    println!("layer E: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
    assert!(cases >= 500);
}

// ---------------------------------------------------------------------------------------
// Layer F: every table row (the row itself is tested in tests/table_rows.rs)
// ---------------------------------------------------------------------------------------

#[test]
fn layer_f_named_count_sweep() {
    // The 40 rows of the table are instantiated with the test prefix in `table_ut.rs`; the
    // per-row named-count test lives in `tests/table_rows.rs`. Here we only assert the count
    // so that the layer total in the report is honest.
    let rows = 40usize;
    println!("layer F: {rows} rows (see tests/table_rows.rs)");
    assert_eq!(rows, raven_libc_va_test::va::table::VARIADIC_ROWS);
}

// ---------------------------------------------------------------------------------------
// Layer G: random shapes and formats
// ---------------------------------------------------------------------------------------

#[test]
fn layer_g_random() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    let seeds: u64 = std::env::var("RL_DIFF_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    let per_seed: usize = std::env::var("RL_DIFF_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000);
    let mut rng = Rng::seed_from_u64(0x677);
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for _ in 0..seeds {
        for _ in 0..per_seed {
            match rng.below(4) {
                0 => {
                    let count = rng.below(13) as usize;
                    let conversions = [
                        "%d", "%i", "%u", "%x", "%o", "%c", "%8d", "%-6d", "%08x", "%.5d",
                    ];
                    let format = build_format(&mut rng, count, &conversions);
                    let mut values = [0i32; 12];
                    for slot in values.iter_mut().take(count) {
                        *slot = rng.next_u64() as i32;
                    }
                    let c = c_format(&format);
                    // SAFETY: the shape passes `count` ints.
                    let ok = unsafe {
                        compare(
                            c.as_slice(),
                            |buffer, n| {
                                (lib.ref_i32)(c.as_ptr(), buffer, n, values.as_ptr(), count as i32)
                            },
                            |buffer, n| {
                                (lib.ours_i32)(
                                    ours_fn,
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    values.as_ptr(),
                                    count as i32,
                                )
                            },
                        )
                    };
                    if !ok {
                        mismatches += 1;
                    }
                }
                1 => {
                    let count = rng.below(13) as usize;
                    let conversions = ["%f", "%e", "%g", "%a", "%.3f", "%.17g", "%12.4e", "%#g"];
                    let format = build_format(&mut rng, count, &conversions);
                    let mut values = [0u64; 12];
                    for slot in values.iter_mut().take(count) {
                        *slot = (rng.next_u64() as f64 / 7.0).to_bits();
                    }
                    let c = c_format(&format);
                    // SAFETY: the shape passes `count` doubles.
                    let ok = unsafe {
                        compare(
                            c.as_slice(),
                            |buffer, n| {
                                (lib.ref_dbl)(c.as_ptr(), buffer, n, values.as_ptr(), count as i32)
                            },
                            |buffer, n| {
                                (lib.ours_dbl)(
                                    ours_fn,
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    values.as_ptr(),
                                    count as i32,
                                )
                            },
                        )
                    };
                    if !ok {
                        mismatches += 1;
                    }
                }
                2 => {
                    let mut ints = [0i64; 7];
                    for slot in ints.iter_mut() {
                        *slot = rng.next_u64() as i64;
                    }
                    let mut doubles = [0u64; 9];
                    for slot in doubles.iter_mut() {
                        *slot = ((rng.next_u64() % 1_000_000) as f64 / 13.0).to_bits();
                    }
                    let format = "%d:%f %d:%f %d:%f %d:%f %d:%f %d:%f";
                    let c = c_format(format);
                    // SAFETY: the shape passes six ints and six doubles.
                    let ok = unsafe {
                        compare(
                            c.as_slice(),
                            |buffer, n| {
                                (lib.ref_mix6)(
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    ints.as_ptr(),
                                    doubles.as_ptr(),
                                )
                            },
                            |buffer, n| {
                                (lib.ours_mix6)(
                                    ours_fn,
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    ints.as_ptr(),
                                    doubles.as_ptr(),
                                )
                            },
                        )
                    };
                    if !ok {
                        mismatches += 1;
                    }
                }
                _ => {
                    let mut ints = [0i64; 7];
                    for slot in ints.iter_mut() {
                        *slot = rng.next_u64() as i64;
                    }
                    let mut doubles = [0u64; 9];
                    for slot in doubles.iter_mut() {
                        *slot = ((rng.next_u64() % 1_000_000) as f64 / 31.0).to_bits();
                    }
                    let format = "%d %d %d %d %d %d %d %f %f %f %f %f %f %f %f %f";
                    let c = c_format(format);
                    // SAFETY: the shape passes seven ints and nine doubles.
                    let ok = unsafe {
                        compare(
                            c.as_slice(),
                            |buffer, n| {
                                (lib.ref_mix79)(
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    ints.as_ptr(),
                                    doubles.as_ptr(),
                                )
                            },
                            |buffer, n| {
                                (lib.ours_mix79)(
                                    ours_fn,
                                    c.as_ptr(),
                                    buffer,
                                    n,
                                    ints.as_ptr(),
                                    doubles.as_ptr(),
                                )
                            },
                        )
                    };
                    if !ok {
                        mismatches += 1;
                    }
                }
            }
            cases += 1;
        }
    }
    println!("layer G: {cases} cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
}

// ---------------------------------------------------------------------------------------
// The `%al` rule and the stack alignment (C59 section 7.2)
// ---------------------------------------------------------------------------------------

#[test]
fn al_rule_and_stack_alignment() {
    let Some(lib) = Lib::load() else { return };
    let ours_fn = lib.trampoline();
    // `%al` is zero for an integer-only call and non-zero for a float call; the golden text
    // proves the stub tests it. What this test adds at run time is both stack parities with
    // the `movaps` block executing: 7 integers + 9 doubles is an odd number of stack slots
    // (rsp at the stub entry is 8 mod 16 for those callers), and layer A covers the even
    // integer-only case with `%al = 0`.
    let mut ints = [0i64; 7];
    for (index, slot) in ints.iter_mut().enumerate() {
        *slot = index as i64 + 1;
    }
    let mut doubles = [0u64; 9];
    for (index, slot) in doubles.iter_mut().enumerate() {
        *slot = (1.5 + index as f64).to_bits();
    }
    let mut mismatches = 0usize;
    for format in [
        "%d %d %d %d %d %d %d %f %f %f %f %f %f %f %f %f",
        "%f %d %d %d %d %d %d %d %f %f %f %f %f %f %f %f",
    ] {
        let c = c_format(format);
        // SAFETY: the shape passes seven ints and nine doubles; both formats read at most
        // that many, in an order the shape's argument list covers.
        let ok = unsafe {
            compare(
                c.as_slice(),
                |buffer, n| (lib.ref_mix79)(c.as_ptr(), buffer, n, ints.as_ptr(), doubles.as_ptr()),
                |buffer, n| {
                    (lib.ours_mix79)(
                        ours_fn,
                        c.as_ptr(),
                        buffer,
                        n,
                        ints.as_ptr(),
                        doubles.as_ptr(),
                    )
                },
            )
        };
        if !ok {
            mismatches += 1;
        }
    }
    println!("%al/alignment: 2 cases, {mismatches} mismatches");
    assert_eq!(mismatches, 0);
}

// ---------------------------------------------------------------------------------------
// Layer I: scanf shapes
// ---------------------------------------------------------------------------------------

#[test]
fn layer_i_scan_shapes() {
    let mut rng = Rng::seed_from_u64(0x199);
    let atoms: [(&str, usize, bool); 12] = [
        ("%d", 1, false),
        ("%d %d", 2, false),
        ("%d %d %d", 3, false),
        ("%d%n", 1, true),
        ("%d %d%n", 2, true),
        ("%d %d %d%n", 3, true),
        ("%3d", 1, false),
        ("%i", 1, false),
        ("%x", 1, false),
        ("%o", 1, false),
        ("%u", 1, false),
        ("%d,%d", 2, false),
    ];
    // Integer overflow in `scanf` is undefined by C: glibc wraps (2^63 -> 0) while the musl
    // engine saturates (-> -1), so the out-of-range inputs are a documented exclusion
    // (C43's scanf engine follows musl; see the report).
    let inputs = [
        "42", " 42", "42abc", "-7", "0x1f", "1e3", "", "  ", "0", "+5", "12 34", "12 34 56", "abc",
        "12,34",
    ];
    let mut cases = Vec::new();
    for _ in 0..1_000 {
        let (format, ints, count) = rng.pick(&atoms);
        let input = rng.pick(&inputs);
        cases.push(ScanCase {
            input: input.to_string(),
            fmt: format.to_string(),
            ints,
            count,
        });
    }
    let driver = match ScanDriver::build(&cases) {
        Ok(Some(driver)) => driver,
        Ok(None) => {
            println!("skipped: clang not found");
            return;
        }
        Err(error) => panic!("scan driver build failed: {error}"),
    };
    let ours_fn = t_sscanf as *const () as usize;
    let mut mismatches = 0usize;
    for (index, case) in cases.iter().enumerate() {
        let mut input = case.input.as_bytes().to_vec();
        input.push(0);
        let (reference, ours) = driver.compare(
            index,
            ours_fn,
            &input,
            &format!("{} on {:?}", case.fmt, case.input),
        );
        if reference != ours {
            println!(
                "MISMATCH scan {:?} on {:?}: ref {reference} ours {ours}",
                case.fmt, case.input
            );
            mismatches += 1;
        }
    }
    println!("layer I: {} cases, {mismatches} mismatches", cases.len());
    assert_eq!(mismatches, 0);
}

// ---------------------------------------------------------------------------------------
// Layer H: the K03 corpus (its own test file, counted here for the total)
// ---------------------------------------------------------------------------------------

#[test]
fn layer_h_k03_corpus_count() {
    let corpus = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/k03.toml"),
    )
    .expect("corpus");
    let cases = corpus
        .lines()
        .filter(|line| line.trim() == "[[case]]")
        .count();
    println!("layer H: {cases} K03 cases (see tests/k03_corpus.rs)");
    assert_eq!(cases, 57);
}

// ---------------------------------------------------------------------------------------
// A heterogeneous corpus through the per-case driver (the exact ticket mechanism)
// ---------------------------------------------------------------------------------------

#[test]
fn the_per_case_driver_matches_glibc() {
    let cases: Vec<Case> = vec![
        Case::new("hello", vec![]),
        Case::new(
            "%d:%f:%s",
            vec![
                CaseArg::I32(7),
                CaseArg::F64(1.5f64.to_bits()),
                CaseArg::Str("x".into()),
            ],
        ),
        Case::new(
            "%lld %p",
            vec![CaseArg::I64(i64::MIN), CaseArg::Ptr(0x1234)],
        ),
        Case::new("%s", vec![CaseArg::Null]),
        Case::new("%c%c", vec![CaseArg::I32(65), CaseArg::I32(66)]),
    ];
    let driver = match CaseDriver::build(&cases) {
        Ok(Some(driver)) => driver,
        Ok(None) => {
            println!("skipped: clang not found");
            return;
        }
        Err(error) => panic!("driver build failed: {error}"),
    };
    let ours = t_snprintf as *const () as usize;
    for index in 0..cases.len() {
        let mut buffer = [0u8; 128];
        let capacity = buffer.len();
        let (reference, actual) = driver.compare(index, ours, &mut buffer, capacity);
        assert_eq!(reference, actual, "case {index}");
    }
}
