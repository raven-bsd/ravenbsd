// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The native differential: the file the `qemu-user` lane runs (C59 section 7.2).
//!
//! On the x86_64 host it runs natively and must pass. The lane for aarch64/riscv64 is
//! commented in the ticket because neither `qemu-aarch64`/`qemu-riscv64` nor the
//! `riscv64gc-unknown-linux-gnu` target is installed on the reference machine; when a machine
//! has them, the same file is built with
//!
//! ```text
//! CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
//! CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER="qemu-aarch64 -L /usr/aarch64-linux-gnu" \
//! RUSTFLAGS='--cfg raven_va_verified_aarch64' \
//! cargo test -p raven-libc-va-test --target aarch64-unknown-linux-gnu --test diff_native
//! ```
//!
//! The generated C driver is compiled for the host; a cross run needs the clang
//! `--target=<arch>-linux-gnu --sysroot=/usr/<arch>-linux-gnu` variant, which the ticket
//! specifies and which cannot be exercised here.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/driver.rs"]
mod driver;
#[path = "../src/table_ut.rs"]
mod table;
#[path = "../src/targets.rs"]
mod targets;

use driver::{Case, CaseArg, CaseDriver};

unsafe extern "C" {
    fn t_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
}

#[test]
fn native_differential() {
    if !cfg!(target_arch = "x86_64") {
        println!(
            "skipped: the qemu-user lane needs a cross-compiled C driver; see the ticket's commented commands"
        );
        return;
    }
    let cases = vec![
        Case::new("hello", vec![]),
        Case::new("%d", vec![CaseArg::I32(0)]),
        Case::new("%d", vec![CaseArg::I32(-42)]),
        Case::new("%s", vec![CaseArg::Str("world".into())]),
        Case::new("%s", vec![CaseArg::Null]),
        Case::new("%.2f", vec![CaseArg::F64(1234.5678f64.to_bits())]),
        Case::new("%f", vec![CaseArg::F64(f64::INFINITY.to_bits())]),
        Case::new("%Lf", vec![CaseArg::LD(0x3fff_c000_0000_0000_0000u128)]),
        Case::new(
            "%d %d %d %d %d %d %d %f %f",
            vec![
                CaseArg::I32(1),
                CaseArg::I32(2),
                CaseArg::I32(3),
                CaseArg::I32(4),
                CaseArg::I32(5),
                CaseArg::I32(6),
                CaseArg::I32(7),
                CaseArg::F64(1.25f64.to_bits()),
                CaseArg::F64(2.75f64.to_bits()),
            ],
        ),
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
        let mut buffer = [0u8; 256];
        let capacity = buffer.len();
        let (reference, actual) = driver.compare(index, ours, &mut buffer, capacity);
        assert_eq!(reference, actual, "native case {index}");
    }
    println!("native differential: {} cases, 0 mismatches", cases.len());
}
