// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The soft-float lane (H-20/CR-21, RD-new 1).
//!
//! Built with `RUSTFLAGS='--cfg raven_va_softfloat'`:
//!
//! ```text
//! RUSTFLAGS='--cfg raven_va_softfloat' cargo test -p raven-libc-va-test --test softfloat
//! ```
//!
//! Under the cfg the x86_64 stub omits the XMM spill and the `%al` test, stores
//! `fp_offset = 176`, and the reader takes every `double` from the OVERFLOW area. The test
//! compiles the same generated C cases with `clang -msoft-float -mno-sse -mno-mmx` (the
//! caller that pushes doubles on the stack, OBSERVED in C59 section 2.6(b)) and compares the
//! result with the host glibc reference computed by a normal (hard-float) driver.
//!
//! "a soft-float function cannot call the hard-float glibc `snprintf` correctly", so the
//! soft-float object calls ONLY our trampoline; the expected strings come from the hard-float
//! driver of the same case.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/driver.rs"]
mod driver;
#[path = "../src/table_ut.rs"]
mod table;
#[path = "../src/targets.rs"]
mod targets;

use driver::{Case, CaseArg, CaseDriver, SoftDriver};

unsafe extern "C" {
    fn t_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
}

fn double(value: f64) -> CaseArg {
    CaseArg::F64(value.to_bits())
}

/// The cases of the soft-float lane: integers, doubles at every register/stack position,
/// long doubles and mixes.
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for count in 0..=9usize {
        let format = vec!["%f"; count].join(" ");
        let format = if count == 0 {
            "literal".to_string()
        } else {
            format
        };
        let values = (0..count)
            .map(|index| double(1.5 + index as f64 * 0.25))
            .collect();
        cases.push(Case::new(&format, values));
    }
    cases.push(Case::new(
        "%d %f %d %f %s",
        vec![
            CaseArg::I32(1),
            double(2.5),
            CaseArg::I32(3),
            double(4.5),
            CaseArg::Str("tail".into()),
        ],
    ));
    cases.push(Case::new(
        "%Lf %d",
        vec![CaseArg::LD(0x3fff_c000_0000_0000_0000u128), CaseArg::I32(7)],
    ));
    cases.push(Case::new(
        "%d %d %d %d %d %d %f %f",
        vec![
            CaseArg::I32(1),
            CaseArg::I32(2),
            CaseArg::I32(3),
            CaseArg::I32(4),
            CaseArg::I32(5),
            CaseArg::I32(6),
            double(7.5),
            double(8.75),
        ],
    ));
    for value in [0.0f64, -0.0, 1.5, -2.25, f64::INFINITY, f64::NAN, 1.0 / 3.0] {
        cases.push(Case::new("%.17g", vec![double(value)]));
        cases.push(Case::new("%e", vec![double(value)]));
        cases.push(Case::new("%a", vec![double(value)]));
    }
    cases
}

#[test]
fn the_soft_float_cfg_selects_the_stack_rule() {
    if !cfg!(raven_va_softfloat) {
        println!("skipped: build with RUSTFLAGS='--cfg raven_va_softfloat'");
        return;
    }
    let cases = cases();
    let reference = match CaseDriver::build(&cases) {
        Ok(Some(driver)) => driver,
        Ok(None) => {
            println!("skipped: clang not found");
            return;
        }
        Err(error) => panic!("hard-float driver failed: {error}"),
    };
    let soft = match SoftDriver::build(&cases) {
        Ok(Some(driver)) => driver,
        Ok(None) => {
            println!("skipped: clang not found");
            return;
        }
        Err(error) => panic!("soft-float driver failed: {error}"),
    };
    let ours = t_snprintf as *const () as usize;
    let mut compared = 0usize;
    for (index, case) in cases.iter().enumerate() {
        let mut expected = [0u8; 256];
        let capacity = expected.len();
        let expected_length = reference.reference(index, &mut expected, capacity);
        let mut actual = [0u8; 256];
        let actual_length = soft.run(index, ours, &mut actual, capacity);
        assert_eq!(
            actual_length, expected_length,
            "soft-float case {index} ({:?}): return value",
            case.fmt
        );
        assert_eq!(
            actual, expected,
            "soft-float case {index} ({:?}): bytes",
            case.fmt
        );
        compared += 1;
    }
    println!("soft-float lane: {compared} cases, 0 mismatches");
    assert!(compared >= 20);
}
