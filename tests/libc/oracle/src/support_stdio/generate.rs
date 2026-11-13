// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Deterministic generators for the STDIO differential layers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use crate::rng::Rng;

use super::case::{ScriptCase, ScriptOp, hex};

fn op(op: &str, arg0: i64, arg1: i64, arg2: i64) -> ScriptOp {
    ScriptOp {
        op: op.to_string(),
        arg0,
        arg1,
        arg2,
    }
}

fn base(files: &[Vec<u8>], blobs: &[Vec<u8>], mode: &str, ops: Vec<ScriptOp>) -> ScriptCase {
    ScriptCase {
        files_hex: files.iter().map(|data| hex(data)).collect(),
        blobs_hex: blobs.iter().map(|data| hex(data)).collect(),
        mode: mode.to_string(),
        ops,
    }
}

/// One seeded random script (layer A).
#[must_use]
pub fn random_case(rng: &mut Rng) -> ScriptCase {
    let files: Vec<Vec<u8>> = (0..4)
        .map(|_| {
            let len = rng.below(301) as usize;
            rng.bytes(len)
        })
        .collect();
    let blobs: Vec<Vec<u8>> = (0..3)
        .map(|_| {
            let len = 1 + rng.below(64) as usize;
            rng.bytes(len)
        })
        .collect();
    let mode = rng.pick(&["r", "r+", "w", "w+", "a", "a+"]);
    // R37: `ungetc` on a write-only stream does not set `F_ERR` in our rules; musl's
    // `__toread` does. The oracle excludes that cell, so the generator does not emit it.
    let write_only = !mode.contains('+') && !mode.starts_with('r');
    let mut ops = vec![op("open", 0, 0, 0)];
    let count = 1 + rng.below(39);
    for index in 0..count {
        if index == 0 && rng.below(4) == 0 {
            let kind = rng.below(3) as i64;
            let size = 1 + rng.below(64) as i64;
            ops.push(op("setvbuf", kind, size, 0));
            continue;
        }
        match rng.below(16) {
            0 => {
                let count = 1 + rng.below(64) as i64;
                let size = 1 + rng.below(8) as i64;
                ops.push(op("fread", count, size, 0));
            }
            1 => {
                let blob = rng.below(3) as i64;
                let size = 1 + rng.below(8) as i64;
                let count = 1 + rng.below(8) as i64;
                ops.push(op("fwrite", blob, size, count));
            }
            2 => {
                let count = 1 + rng.below(64) as i64;
                ops.push(op("fgets", count, 0, 0));
            }
            3 => {
                let blob = rng.below(3) as i64;
                ops.push(op("fputs", blob, 0, 0));
            }
            4 => ops.push(op("getc", 0, 0, 0)),
            5 => {
                let byte = rng.below(256) as i64;
                ops.push(op("putc", byte, 0, 0));
            }
            6 if !write_only => {
                let byte = rng.below(256) as i64;
                ops.push(op("ungetc", byte, 0, 0));
            }
            6 => {
                let byte = rng.below(256) as i64;
                ops.push(op("putc", byte, 0, 0));
            }
            7 => {
                let off = rng.below(41) as i64 - 20;
                let whence = rng.below(3) as i64;
                ops.push(op("fseek", off, whence, 0));
            }
            8 => ops.push(op("ftell", 0, 0, 0)),
            9 => ops.push(op("fflush", 0, 0, 0)),
            10 => ops.push(op("rewind", 0, 0, 0)),
            11 => ops.push(op("clearerr", 0, 0, 0)),
            12 => ops.push(op("feof", 0, 0, 0)),
            13 => ops.push(op("ferror", 0, 0, 0)),
            14 => {
                let delim = i64::from(rng.pick(&[b'\n', b',', 0]));
                let capacity = rng.below(65) as i64;
                ops.push(op("getdelim", delim, capacity, 0));
            }
            _ => ops.push(op("close", 0, 0, 0)),
        }
    }
    base(&files, &blobs, mode, ops)
}

/// Buffer-size and request-size edges (layer B).
#[must_use]
pub fn boundary_cases() -> Vec<ScriptCase> {
    let mut cases = Vec::new();
    let sizes = [1i64, 7, 8, 9, 1023, 1024, 1025];
    let file_lens = [0usize, 1, 7, 8, 9, 300, 1024, 1100];
    for &size in &sizes {
        for &kind in &[0i64, 1, 2] {
            for &mode in &["r+", "w+", "a+"] {
                for &file_len in &file_lens {
                    let data: Vec<u8> = (0..file_len).map(|index| (index % 251) as u8).collect();
                    for &delta in &[-1i64, 0, 1] {
                        let count = (size + delta).max(0);
                        let ops = vec![
                            op("open", 0, 0, 0),
                            op("setvbuf", kind, size, 0),
                            op("fread", count, 1, 0),
                            op("fread", 1, count.max(1), 0),
                            op("ftell", 0, 0, 0),
                            op("fseek", 0, 0, 0),
                            op("fgets", count.max(1), 0, 0),
                            op("fwrite", 0, 1, count.max(1)),
                            op("ftell", 0, 0, 0),
                            op("fflush", 0, 0, 0),
                            op("close", 0, 0, 0),
                        ];
                        cases.push(base(&[data.clone()], &[data.clone()], mode, ops));
                        let line_ops = vec![
                            op("open", 0, 0, 0),
                            op("setvbuf", kind, size, 0),
                            op("getdelim", i64::from(b'\n'), (size + delta).max(0), 0),
                            op("getdelim", i64::from(b'\n'), (size + delta).max(0), 0),
                            op("ftell", 0, 0, 0),
                            op("close", 0, 0, 0),
                        ];
                        cases.push(base(&[data.clone()], &[], mode, line_ops));
                        let write_ops = vec![
                            op("open", 0, 0, 0),
                            op("setvbuf", kind, size, 0),
                            op("fwrite", 0, 1, (size + delta).max(0)),
                            op("fwrite", 0, (size + delta).max(1), 1),
                            op("ftell", 0, 0, 0),
                            op("fseek", 0, 1, 0),
                            op("fflush", 0, 0, 0),
                            op("feof", 0, 0, 0),
                            op("ferror", 0, 0, 0),
                            op("close", 0, 0, 0),
                        ];
                        cases.push(base(&[data.clone()], &[data.clone()], mode, write_ops));
                        let mixed_ops = vec![
                            op("open", 0, 0, 0),
                            op("setvbuf", kind, size, 0),
                            op("fgets", (size + delta).max(1), 0, 0),
                            op("getdelim", i64::from(b'\n'), (size + delta).max(0), 0),
                            op("feof", 0, 0, 0),
                            op("clearerr", 0, 0, 0),
                            op("fread", 1, (size + delta).max(1), 0),
                            op("close", 0, 0, 0),
                        ];
                        cases.push(base(&[data.clone()], &[], mode, mixed_ops));
                    }
                }
            }
        }
    }
    cases
}

/// All interleavings of `ungetc`/`getc`/`fseek`/`ftell` of length four (layer D).
#[must_use]
pub fn ungetc_cases() -> Vec<ScriptCase> {
    let alphabet = [
        op("getc", 0, 0, 0),
        op("ungetc", i64::from(b'A'), 0, 0),
        op("ungetc", i64::from(b'B'), 0, 0),
        op("ungetc", -1, 0, 0),
        op("fseek", -1, 1, 0),
        op("fseek", 0, 1, 0),
        op("fseek", 1, 1, 0),
        op("fseek", 0, 0, 0),
        op("ftell", 0, 0, 0),
        op("feof", 0, 0, 0),
        op("getc", 0, 0, 0),
        op("ungetc", i64::from(b'C'), 0, 0),
    ];
    let data = b"abcdef".to_vec();
    let mut cases = Vec::new();
    for a in &alphabet {
        for b in &alphabet {
            for c in &alphabet {
                for d in &alphabet {
                    let ops = vec![
                        op("open", 0, 0, 0),
                        a.clone(),
                        b.clone(),
                        c.clone(),
                        d.clone(),
                        op("ftell", 0, 0, 0),
                        op("close", 0, 0, 0),
                    ];
                    cases.push(base(&[data.clone()], &[], "r+", ops.clone()));
                    cases.push(base(&[b"ab".to_vec()], &[], "r+", ops));
                }
            }
        }
    }
    cases
}
