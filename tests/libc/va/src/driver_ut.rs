// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the generated-C driver (`driver.rs`): the generator must emit one reference
//! and one "ours" function per shape, and the C must compile with the system compiler.

use std::path::Path;

use super::*;

#[test]
fn the_generated_source_has_every_shape() {
    let source = generate_c();
    for shape in [
        "i32", "i64", "dbl", "mix6", "mix68", "mix79", "ld", "ptr", "pctn",
    ] {
        assert!(
            source.contains(&format!("ref_{shape}(")),
            "missing reference shape {shape}"
        );
        assert!(
            source.contains(&format!("ours_{shape}(")),
            "missing ours shape {shape}"
        );
    }
    for size in [8, 12, 16, 24, 32] {
        assert!(
            source.contains(&format!("ours_agg{size}(")),
            "missing aggregate shape {size}"
        );
    }
    // The variadic call sites must really be variadic.
    assert!(source.contains("snprintf(b, n, f, (int)v[0])"));
    assert!(source.contains("raven_double_from_bits(v[0])"));
}

#[test]
fn the_workdir_is_under_target() {
    let directory = workdir().expect("workdir");
    assert!(
        directory.to_string_lossy().contains("target")
            || directory.to_string_lossy().contains("tmp"),
        "generated C must live under target/: {directory:?}"
    );
    assert!(
        !directory.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("..")),
        "generated C must not live in the scanned source tree: {directory:?}"
    );
}

#[test]
fn the_driver_compiles_or_skips() {
    match Driver::build() {
        Ok(Some(driver)) => {
            assert!(!driver.sym("ref_i32").is_null());
            assert!(!driver.sym("ours_i32").is_null());
            assert!(driver.sym("does_not_exist").is_null());
            // The function pointer types are the ones the tests use.
            // SAFETY: the symbol has the declared signature.
            let function: Option<
                unsafe extern "C" fn(*const u8, *mut u8, usize, *const i32, i32) -> i32,
            > = unsafe { driver.func("ref_i32") };
            assert!(function.is_some());
        }
        Ok(None) => println!("skipped: clang not found"),
        Err(error) => panic!("driver build failed: {error}"),
    }
}
