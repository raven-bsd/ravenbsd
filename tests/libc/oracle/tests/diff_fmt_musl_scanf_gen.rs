// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generated scanf differential matrix (ticket C43 section 7.1, layers H and I).
//!
//! Layer H: the 30 atomic directives of the ticket, concatenated into sequences
//! of length 1, 2 and 3 (30 + 900 + 27,000 formats), with each of the 16 input
//! strings; every case is also followed by a `%n` probe so the consumed count
//! is compared as a destination. Layer I is the same 27,930 formats through
//! `swscanf` with the first 8 ASCII inputs, thinned deterministically to every
//! tenth format (>= 20,000 comparisons).
//!
//! The order is fixed (atom order as listed, sequences by length then
//! lexicographic atom index, inputs in ticket order); no random seed is
//! involved. The test prints the exact format and comparison counts.

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

use core::ffi::c_void;
use std::vec::Vec;

use common::{diff_sscanf, diff_swscanf};
use raven_libc_fmt::ScanResult;
use raven_libc_oracle::support_fmt::OracleVa;

unsafe extern "C" {
    fn free(p: *mut c_void);
}

const CELL: usize = 32;

/// One atomic directive of the ticket's layer H list.
struct Atom {
    text: &'static str,
    /// Number of destination arguments consumed by this atom.
    dests: usize,
    /// True for the allocating `%ms` atom.
    alloc: bool,
    /// The destination image for the NaN-payload normalisation.
    kind: DestKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DestKind {
    Other,
    F32,
    F64,
    F80,
    Alloc,
}

const ATOMS: [Atom; 30] = [
    Atom {
        text: "%d",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%i",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%u",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%o",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%x",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%X",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%c",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%s",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%[a-c]",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%[^,]",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%5d",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%*d",
        dests: 0,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%n",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%f",
        dests: 1,
        alloc: false,
        kind: DestKind::F32,
    },
    Atom {
        text: "%lf",
        dests: 1,
        alloc: false,
        kind: DestKind::F64,
    },
    Atom {
        text: "%Lf",
        dests: 1,
        alloc: false,
        kind: DestKind::F80,
    },
    Atom {
        text: "%a",
        dests: 1,
        alloc: false,
        kind: DestKind::F32,
    },
    Atom {
        text: "%m",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%ms",
        dests: 1,
        alloc: true,
        kind: DestKind::Alloc,
    },
    Atom {
        text: "%p",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%hhd",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%hd",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%ld",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%lld",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%jd",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%zd",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%td",
        dests: 1,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "%%",
        dests: 0,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: " ",
        dests: 0,
        alloc: false,
        kind: DestKind::Other,
    },
    Atom {
        text: "x",
        dests: 0,
        alloc: false,
        kind: DestKind::Other,
    },
];

const INPUTS: [&[u8]; 16] = [
    b"",
    b" ",
    b"12",
    b"-12",
    b"0x1f",
    b"1e3",
    b"inf",
    b"nan(0x1)",
    b"1.5e",
    b"\xe2\x82\xac",
    b"  42abc",
    b"0b101",
    b"+",
    b"-0",
    b"1e+",
    b"0x",
];

/// The first 8 inputs are ASCII, so they can be lifted to wide units.
const WIDE_INPUTS: usize = 8;

struct Built {
    fmt: String,
    dests: usize,
    allocs: Vec<usize>,
    kinds: Vec<DestKind>,
}

/// Concatenate one sequence and append the `%n` consumed-count probe.
fn build(seq: &[usize]) -> Built {
    let mut fmt = String::new();
    let mut dests = 0usize;
    let mut allocs = Vec::new();
    let mut kinds = Vec::new();
    for &i in seq {
        let atom = ATOMS.get(i);
        let Some(atom) = atom else { continue };
        fmt.push_str(atom.text);
        if atom.dests > 0 {
            if atom.alloc {
                allocs.push(dests);
            }
            kinds.push(atom.kind);
            dests = dests.saturating_add(1);
        }
    }
    fmt.push_str("%n");
    kinds.push(DestKind::Other);
    dests = dests.saturating_add(1);
    Built {
        fmt,
        dests,
        allocs,
        kinds,
    }
}

struct GenCase {
    cret: i32,
    result: ScanResult,
    c_cells: Vec<Box<[u8; CELL]>>,
    va: OracleVa,
    rust_cells: Vec<usize>,
    allocs: Vec<usize>,
    kinds: Vec<DestKind>,
}

fn run_generated(input: &[u8], built: &Built) -> GenCase {
    let mut c_cells: Vec<Box<[u8; CELL]>> =
        (0..built.dests).map(|_| Box::new([0u8; CELL])).collect();
    let c_ptrs: Vec<*mut c_void> = c_cells
        .iter_mut()
        .map(|b| b.as_mut_ptr() as *mut c_void)
        .collect();
    let mut va = OracleVa::new();
    let mut rust_cells = Vec::new();
    for _ in 0..built.dests {
        let off = va.cell(CELL);
        va.word(off as u64);
        rust_cells.push(off);
    }
    let input_str = input;
    let (cret, result) = diff_sscanf(input_str, &built.fmt, &c_ptrs, &mut va);
    GenCase {
        cret,
        result,
        c_cells,
        va,
        rust_cells,
        allocs: built.allocs.clone(),
        kinds: built.kinds.clone(),
    }
}

fn expected_ret(result: &ScanResult) -> i32 {
    if result.return_eof {
        -1
    } else {
        result.assigned
    }
}

fn c_string(ptr: *mut c_void) -> Vec<u8> {
    if ptr.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    unsafe {
        while *((ptr as *const u8).add(i)) != 0 {
            out.push(*((ptr as *const u8).add(i)));
            i = i.saturating_add(1);
        }
    }
    out
}

fn is_nan_image(bytes: &[u8], kind: DestKind) -> bool {
    match kind {
        DestKind::F32 => {
            let b = u32::from_le_bytes([
                bytes.first().copied().unwrap_or(0),
                bytes.get(1).copied().unwrap_or(0),
                bytes.get(2).copied().unwrap_or(0),
                bytes.get(3).copied().unwrap_or(0),
            ]);
            (b & 0x7f80_0000) == 0x7f80_0000 && (b & 0x007f_ffff) != 0
        }
        DestKind::F64 => {
            let mut a = [0u8; 8];
            for (i, slot) in a.iter_mut().enumerate() {
                *slot = bytes.get(i).copied().unwrap_or(0);
            }
            let b = u64::from_le_bytes(a);
            (b & 0x7ff0_0000_0000_0000) == 0x7ff0_0000_0000_0000 && (b & 0x000f_ffff_ffff_ffff) != 0
        }
        DestKind::F80 => {
            let sig = {
                let mut a = [0u8; 8];
                for (i, slot) in a.iter_mut().enumerate() {
                    *slot = bytes.get(i).copied().unwrap_or(0);
                }
                u64::from_le_bytes(a)
            };
            let exp = u16::from_le_bytes([
                bytes.get(8).copied().unwrap_or(0),
                bytes.get(9).copied().unwrap_or(0),
            ]) & 0x7fff;
            exp == 0x7fff && sig != 0x8000_0000_0000_0000
        }
        _ => false,
    }
}

fn nan_sign(bytes: &[u8], kind: DestKind) -> bool {
    match kind {
        DestKind::F32 => bytes.get(3).copied().unwrap_or(0) & 0x80 != 0,
        DestKind::F64 => bytes.get(7).copied().unwrap_or(0) & 0x80 != 0,
        DestKind::F80 => bytes.get(9).copied().unwrap_or(0) & 0x80 != 0,
        _ => false,
    }
}

/// `nan(n-char-sequence)` has an implementation-defined payload: musl
/// canonicalises it, C42's `strtof`/`strtold` preserve it. The sign and the
/// NaN class are still compared; only the payload bits are normalised.
fn nan_images_equal(c: &[u8], r: &[u8], kind: DestKind) -> bool {
    is_nan_image(c, kind) && is_nan_image(r, kind) && nan_sign(c, kind) == nan_sign(r, kind)
}

fn compare_generated(case: &GenCase, what: &str) {
    assert_eq!(
        case.cret,
        expected_ret(&case.result),
        "{what}: return value (C {} vs Rust assigned {} eof {})",
        case.cret,
        case.result.assigned,
        case.result.return_eof
    );
    for (i, cell) in case.c_cells.iter().enumerate() {
        let off = case.rust_cells.get(i).copied().unwrap_or(0);
        if case.allocs.contains(&i) {
            let c_ptr = unsafe { *(cell.as_ptr() as *const *mut c_void) };
            let r_ptr = case.va.read_ptr(off);
            let c_text = c_string(c_ptr);
            let r_text = case
                .va
                .arena
                .get(r_ptr..)
                .unwrap_or(&[])
                .iter()
                .copied()
                .take_while(|&b| b != 0)
                .collect::<Vec<u8>>();
            assert_eq!(c_text, r_text, "{what}: %m text at destination {i}");
            if !c_ptr.is_null() {
                unsafe {
                    free(c_ptr);
                }
            }
        } else {
            let rust = case
                .va
                .arena
                .get(off..off.saturating_add(CELL))
                .unwrap_or(&[]);
            let kind = case.kinds.get(i).copied().unwrap_or(DestKind::Other);
            if cell.as_slice() != rust && !nan_images_equal(cell.as_slice(), rust, kind) {
                panic!(
                    "{what}: destination {i} differs (kind {kind:?}) C {:02x?} Rust {:02x?}",
                    cell.as_slice(),
                    rust
                );
            }
        }
    }
}

fn layer_h_case(seq: &[usize], input: &[u8]) -> (usize, usize) {
    let built = build(seq);
    let case = run_generated(input, &built);
    let what = format!(
        "layer H {:?} {:?}",
        built.fmt,
        String::from_utf8_lossy(input)
    );
    compare_generated(&case, &what);
    (1, built.dests)
}

#[test]
fn layer_h_generated() {
    let n = ATOMS.len();
    let mut formats = 0usize;
    let mut comparisons = 0usize;
    let mut dests = 0usize;

    for a in 0..n {
        let seq = [a];
        for input in INPUTS {
            let (f, d) = layer_h_case(&seq, input);
            formats = formats.saturating_add(f);
            comparisons = comparisons.saturating_add(f);
            dests = dests.saturating_add(d);
        }
    }
    for a in 0..n {
        for b in 0..n {
            let seq = [a, b];
            for input in INPUTS {
                let (f, d) = layer_h_case(&seq, input);
                formats = formats.saturating_add(f);
                comparisons = comparisons.saturating_add(f);
                dests = dests.saturating_add(d);
            }
        }
    }
    for a in 0..n {
        for b in 0..n {
            for c in 0..n {
                let seq = [a, b, c];
                for input in INPUTS {
                    let (f, d) = layer_h_case(&seq, input);
                    formats = formats.saturating_add(f);
                    comparisons = comparisons.saturating_add(f);
                    dests = dests.saturating_add(d);
                }
            }
        }
    }
    println!(
        "fmt-musl-scanf layer H: {formats} cases ({} formats x {} inputs), {dests} destinations, 0 mismatches",
        formats / INPUTS.len(),
        INPUTS.len()
    );
    assert_eq!(formats, 30 * 16 + 30 * 30 * 16 + 30 * 30 * 30 * 16);
}

/// The wide layer I: same atoms, first 8 inputs, every tenth format.
#[test]
fn layer_i_generated() {
    let n = ATOMS.len();
    let mut format_index = 0usize;
    let mut formats = 0usize;
    let mut comparisons = 0usize;
    let mut dests = 0usize;

    let mut run = |seq: &[usize]| {
        let selected = format_index % 10 == 0;
        format_index = format_index.saturating_add(1);
        if !selected {
            return;
        }
        let built = build(seq);
        let units: Vec<u32> = built.fmt.chars().map(|c| c as u32).collect();
        for input in INPUTS.iter().take(WIDE_INPUTS) {
            let wide: Vec<u32> = input.iter().map(|&b| u32::from(b)).collect();
            let mut c_cells: Vec<Box<[u8; CELL]>> =
                (0..built.dests).map(|_| Box::new([0u8; CELL])).collect();
            let c_ptrs: Vec<*mut c_void> = c_cells
                .iter_mut()
                .map(|b| b.as_mut_ptr() as *mut c_void)
                .collect();
            let mut va = OracleVa::new();
            let mut rust_cells = Vec::new();
            for _ in 0..built.dests {
                let off = va.cell(CELL);
                va.word(off as u64);
                rust_cells.push(off);
            }
            let (cret, result) = diff_swscanf(&wide, &units, &c_ptrs, &mut va);
            let case = GenCase {
                cret,
                result,
                c_cells,
                va,
                rust_cells,
                allocs: built.allocs.clone(),
                kinds: built.kinds.clone(),
            };
            let what = format!(
                "layer I {:?} {:?}",
                built.fmt,
                String::from_utf8_lossy(input)
            );
            compare_generated(&case, &what);
            comparisons = comparisons.saturating_add(1);
            dests = dests.saturating_add(built.dests);
        }
        formats = formats.saturating_add(1);
    };

    for a in 0..n {
        run(&[a]);
    }
    for a in 0..n {
        for b in 0..n {
            run(&[a, b]);
        }
    }
    for a in 0..n {
        for b in 0..n {
            for c in 0..n {
                run(&[a, b, c]);
            }
        }
    }
    println!(
        "fmt-musl-scanf layer I: {formats} formats, {comparisons} comparisons, {dests} destinations, 0 mismatches"
    );
    assert!(comparisons >= 20_000);
}
