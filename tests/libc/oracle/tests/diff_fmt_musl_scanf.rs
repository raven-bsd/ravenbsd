// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for the musl scanf engine.
//!
//! Every case compares the return value and every destination byte. `%n` is
//! part of the compared cell; `%m` compares the allocated text and frees the C
//! block. The wide cases use musl's `swscanf` instantiation.

#![cfg(feature = "oracle-fmt-musl-scanf")]
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

use common::{diff_sscanf, diff_swscanf};
use raven_libc_fmt::ScanResult;
use raven_libc_oracle::support_fmt::OracleVa;

unsafe extern "C" {
    fn free(p: *mut c_void);
}

const CELL: usize = 32;

struct CaseResult {
    cret: i32,
    result: ScanResult,
    c_cells: Vec<Box<[u8; CELL]>>,
    va: OracleVa,
    rust_cells: Vec<usize>,
}

fn run(input: &str, fmt: &str, ndest: usize) -> CaseResult {
    let mut c_cells: Vec<Box<[u8; CELL]>> = (0..ndest).map(|_| Box::new([0u8; CELL])).collect();
    let c_ptrs: Vec<*mut c_void> = c_cells
        .iter_mut()
        .map(|b| b.as_mut_ptr() as *mut c_void)
        .collect();
    let mut va = OracleVa::new();
    let mut rust_cells = Vec::new();
    for _ in 0..ndest {
        let off = va.cell(CELL);
        va.word(off as u64);
        rust_cells.push(off);
    }
    let (cret, result) = diff_sscanf(input.as_bytes(), fmt, &c_ptrs, &mut va);
    CaseResult {
        cret,
        result,
        c_cells,
        va,
        rust_cells,
    }
}

fn expected_ret(result: &ScanResult) -> i32 {
    if result.return_eof {
        -1
    } else {
        result.assigned
    }
}

fn compare(case: &CaseResult, what: &str) {
    assert_eq!(
        case.cret,
        expected_ret(&case.result),
        "{what}: return value"
    );
    for (i, cell) in case.c_cells.iter().enumerate() {
        let off = case.rust_cells[i];
        let rust = case
            .va
            .arena
            .get(off..off.saturating_add(CELL))
            .unwrap_or(&[]);
        assert_eq!(cell.as_slice(), rust, "{what}: destination {i} differs");
    }
}

#[test]
fn atoms_agree() {
    let cases: [(&str, &str, usize); 22] = [
        ("42", "%d", 1),
        ("-42", "%d", 1),
        ("  42abc", "%d", 1),
        ("0x1f", "%x", 1),
        ("0x1f", "%i", 1),
        ("0755", "%i", 1),
        ("755", "%o", 1),
        ("12", "%u", 1),
        ("42", "%hhd", 1),
        ("70000", "%hd", 1),
        ("9000000000", "%ld", 1),
        ("9000000000", "%lld", 1),
        ("42", "%jd", 1),
        ("42", "%zd", 1),
        ("42", "%td", 1),
        ("123456", "%5d", 1),
        ("12", "%*d", 0),
        ("a", "%c", 1),
        ("ab", "%2c", 1),
        ("abc", "%s", 1),
        ("abc,de", "%[^,]", 1),
        ("abc", "%[a-c]", 1),
    ];
    for (input, fmt, n) in cases {
        let case = run(input, fmt, n);
        compare(&case, &format!("{input:?} {fmt:?}"));
    }
}

#[test]
fn percent_n_and_literals_agree() {
    let mut case = run("12ab", "%d%n", 2);
    compare(&case, "%d%n");
    assert_eq!(case.va.read(case.rust_cells[1], 4), 2);
    case = run("xy", "xy", 0);
    compare(&case, "literal");
    case = run("x", "y", 0);
    compare(&case, "literal mismatch");
    case = run("%", "%%", 0);
    compare(&case, "%%");
}

#[test]
fn eof_and_matching_failure_agree() {
    for (input, fmt, n) in [("", "%d", 1), ("", "%c", 1), ("x", "%d", 1)] {
        let case = run(input, fmt, n);
        compare(&case, &format!("{input:?} {fmt:?}"));
    }
}

#[test]
fn floats_agree() {
    for fmt in ["%e", "%f", "%g", "%a", "%lf", "%le", "%lg"] {
        for input in ["1.5", "-0.25", "1e3", "inf", "nan"] {
            let case = run(input, fmt, 1);
            compare(&case, &format!("{input:?} {fmt:?}"));
        }
    }
}

#[test]
fn scanset_ranges_and_inversion() {
    for (input, fmt) in [
        ("abcd", "%[a-c]"),
        ("ab,cd", "%[^,]"),
        ("]x", "%[]]"),
        ("-x", "%[-a]"),
        ("a-cx", "%[a\\-c]"),
    ] {
        let case = run(input, fmt, 1);
        compare(&case, &format!("{input:?} {fmt:?}"));
    }
}

#[test]
fn percent_m_allocation_agrees() {
    let case = run("abc def", "%ms", 1);
    assert_eq!(case.cret, expected_ret(&case.result));
    // The C cell holds the allocated pointer; the Rust cell the arena offset.
    let c_ptr = unsafe { *(case.c_cells[0].as_ptr() as *const *mut c_char) };
    let r_ptr = case.va.read_ptr(case.rust_cells[0]);
    let c_bytes = unsafe {
        let mut n = 0usize;
        while *c_ptr.add(n) != 0 {
            n = n.saturating_add(1);
        }
        core::slice::from_raw_parts(c_ptr as *const u8, n).to_vec()
    };
    let r_bytes = case
        .va
        .arena
        .get(r_ptr..r_ptr.saturating_add(c_bytes.len()))
        .unwrap_or(&[])
        .to_vec();
    assert_eq!(c_bytes, r_bytes);
    unsafe {
        free(c_ptr as *mut c_void);
    }
}

#[test]
fn wide_atoms_agree() {
    let format = [b'%' as u32, b'd' as u32];
    let mut c_cell = Box::new([0u8; CELL]);
    let c_ptrs = [c_cell.as_mut_ptr() as *mut c_void];
    let mut va = OracleVa::new();
    let off = va.cell(CELL);
    va.word(off as u64);
    let input: Vec<u32> = " 42".chars().map(|c| c as u32).collect();
    let (cret, result) = diff_swscanf(&input, &format, &c_ptrs, &mut va);
    assert_eq!(cret, expected_ret(&result));
    assert_eq!(c_cell[0..4], va.arena[off..off + 4]);
}

#[test]
fn wide_string_agrees() {
    let format: Vec<u32> = "%ls".chars().map(|c| c as u32).collect();
    let mut c_cell = Box::new([0u8; CELL]);
    let c_ptrs = [c_cell.as_mut_ptr() as *mut c_void];
    let mut va = OracleVa::new();
    let off = va.cell(CELL);
    va.word(off as u64);
    let input: Vec<u32> = "abc".chars().map(|c| c as u32).collect();
    let (cret, result) = diff_swscanf(&input, &format, &c_ptrs, &mut va);
    assert_eq!(cret, expected_ret(&result));
    assert_eq!(c_cell[0..16], va.arena[off..off + 16]);
}
