// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::NoScanFloat;
use crate::test_support::{FakeScanFloat, RecordingSource, SliceVa};

struct Run {
    result: Result<ScanResult, FmtErr>,
    va: SliceVa,
    src: RecordingSource<u8>,
    cells: Vec<usize>,
}

fn run(input: &str, fmt: &str, cells: usize) -> Run {
    let mut va = SliceVa::new();
    let mut ptrs = Vec::new();
    for _ in 0..cells {
        let p = va.cell(8);
        va.ptr(p);
        ptrs.push(p);
    }
    let mut src = RecordingSource::new(input.as_bytes().to_vec());
    let opts = ScanOptions::default();
    let result = vscan(&mut src, fmt.as_bytes(), &mut va, &opts, &FakeScanFloat);
    Run {
        result,
        va,
        src,
        cells: ptrs,
    }
}

#[test]
fn integers_and_strings() {
    let r = run("42 abc", "%d %s", 2);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 2);
    assert_eq!(res.consumed, 6);
    assert!(!res.return_eof);
    assert_eq!(r.va.read_cell(r.cells[0], 4), 42);
    let p = r.cells[1];
    assert_eq!(r.va.arena.get(p..p + 3), Some(b"abc".as_slice()));
    assert_eq!(r.va.arena.get(p + 3), Some(&0));
}

#[test]
fn width_never_reads_one_more_character() {
    let r = run("123456", "%5d", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert_eq!(res.consumed, 5);
    assert_eq!(r.va.read_cell(r.cells[0], 4), 12345);
    assert_eq!(r.src.pos, 5);
}

#[test]
fn char_does_not_skip_whitespace_and_uses_a_default_width_of_one() {
    let r = run(" a", "%c", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 1), b' ' as u64);
    let r = run("ab", "%2c", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 1), b'a' as u64);
    assert_eq!(r.va.read_cell(r.cells[0] + 1, 1), b'b' as u64);
}

#[test]
fn char_requires_the_full_width() {
    let r = run("a", "%2c", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 0);
    assert!(!res.return_eof);
}

#[test]
fn scansets_and_inversion() {
    let r = run("abcd", "%[a-c]", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    let p = r.cells[0];
    assert_eq!(r.va.arena.get(p..p + 3), Some(b"abc".as_slice()));
    let r = run("ab,cd", "%[^,]", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    let p = r.cells[0];
    assert_eq!(r.va.arena.get(p..p + 2), Some(b"ab".as_slice()));
    let r = run("]x", "%[]]", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    let p = r.cells[0];
    assert_eq!(r.va.arena.get(p), Some(&(b']' as u8)));
}

#[test]
fn suppression_does_not_assign() {
    let r = run("1 2", "%*d %d", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 4), 2);
}

#[test]
fn percent_n_counts_without_assigning() {
    let mut va = SliceVa::new();
    let a = va.cell(8);
    let n = va.cell(8);
    va.ptr(a).ptr(n);
    let mut src = RecordingSource::new(b"12ab".to_vec());
    let opts = ScanOptions::default();
    let res = vscan(&mut src, b"%d%n", &mut va, &opts, &FakeScanFloat).unwrap();
    assert_eq!(res.assigned, 1);
    assert_eq!(va.read_cell(a, 4), 12);
    assert_eq!(va.read_cell(n, 4), 2);
    assert_eq!(res.consumed, 2);
}

#[test]
fn percent_m_allocates() {
    let mut va = SliceVa::new();
    let dest = va.cell(8);
    va.ptr(dest);
    let mut src = RecordingSource::new(b"abc def".to_vec());
    let opts = ScanOptions::default();
    let res = vscan(&mut src, b"%ms", &mut va, &opts, &FakeScanFloat).unwrap();
    assert_eq!(res.assigned, 1);
    let p = va.read_ptr(dest);
    assert_eq!(va.arena.get(p..p + 3), Some(b"abc".as_slice()));
    assert_eq!(va.arena.get(p + 3), Some(&0));
}

#[test]
fn percent_m_allocation_failure_is_nomem() {
    let mut va = SliceVa::new();
    let dest = va.cell(8);
    va.ptr(dest);
    va.fail_alloc = true;
    let mut src = RecordingSource::new(b"abc".to_vec());
    let opts = ScanOptions::default();
    assert_eq!(
        vscan(&mut src, b"%ms", &mut va, &opts, &FakeScanFloat),
        Err(FmtErr::NoMem)
    );
}

#[test]
fn floats_store_the_requested_width() {
    let r = run("1.5", "%f", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert!(matches!(
        r.va.stores.first(),
        Some(crate::test_support::StoreEvent::Float(_, FloatKind::F32, _))
    ));
    let r = run("1.5", "%lf", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert!(matches!(
        r.va.stores.first(),
        Some(crate::test_support::StoreEvent::Float(_, FloatKind::F64, _))
    ));
    let r = run("1.5", "%Lf", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 1);
    assert!(matches!(
        r.va.stores.first(),
        Some(crate::test_support::StoreEvent::Float(
            _,
            FloatKind::LongDouble,
            _
        ))
    ));
}

#[test]
fn no_scan_float_is_a_matching_failure() {
    let mut va = SliceVa::new();
    let dest = va.cell(8);
    va.ptr(dest);
    let mut src = RecordingSource::new(b"1.5".to_vec());
    let opts = ScanOptions::default();
    let res = vscan(&mut src, b"%f", &mut va, &opts, &NoScanFloat).unwrap();
    assert_eq!(res.assigned, 0);
    assert!(!res.return_eof);
}

#[test]
fn integer_bases_and_pointers() {
    let r = run("0x1f", "%x", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 4), 31);
    let r = run("0755", "%i", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 4), 0o755);
    let r = run("0x1f", "%p", 1);
    assert_eq!(r.result.unwrap().assigned, 1);
    assert_eq!(r.va.read_cell(r.cells[0], 8), 0x1f);
}

#[test]
fn literal_and_percent_mismatches() {
    let r = run("a", "b", 0);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 0);
    assert!(!res.return_eof);
    let r = run("%", "%%", 0);
    assert_eq!(r.result.unwrap().assigned, 0);
    assert_eq!(r.src.pos, 1);
}

#[test]
fn end_of_input_before_a_conversion_is_eof() {
    let r = run("", "%d", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 0);
    assert!(res.return_eof);
    let r = run("", "%c", 1);
    assert!(r.result.unwrap().return_eof);
}

#[test]
fn positional_scanf_is_a_format_failure() {
    let r = run("42", "%1$d", 1);
    let res = r.result.unwrap();
    assert_eq!(res.assigned, 0);
    assert!(res.return_eof);
}

#[test]
fn a_bad_length_is_a_format_failure() {
    let r = run("1", "%yd", 1);
    let res = r.result.unwrap();
    assert!(res.return_eof);
}

#[test]
fn short_lengths_truncate() {
    let r = run("300", "%hhd", 1);
    assert_eq!(r.va.read_cell(r.cells[0], 1), 44);
    let r = run("70000", "%hd", 1);
    assert_eq!(r.va.read_cell(r.cells[0], 2), 4464);
}

#[test]
fn wide_input_stores_wide_strings() {
    let mut va = SliceVa::new();
    let d = va.cell(8);
    let w = va.cell(16);
    va.ptr(d).ptr(w);
    let mut src = RecordingSource::new(vec![b'4' as u32, b'2' as u32, b' ' as u32, 0x20ac]);
    let opts = ScanOptions::default();
    let res = vscan::<u32, _, _, _>(
        &mut src,
        &[
            b'%' as u32,
            b'd' as u32,
            b' ' as u32,
            b'%' as u32,
            b'l' as u32,
            b's' as u32,
        ],
        &mut va,
        &opts,
        &FakeScanFloat,
    )
    .unwrap();
    assert_eq!(res.assigned, 2);
    assert_eq!(va.read_cell(d, 4), 42);
    assert_eq!(va.read_cell(w, 4), 0x20ac);
}

#[test]
fn consumed_counts_are_exact_for_whitespace() {
    let r = run("  42", "%d", 1);
    let res = r.result.unwrap();
    assert_eq!(res.consumed, 4);
    assert_eq!(res.assigned, 1);
}
