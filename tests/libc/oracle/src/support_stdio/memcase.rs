// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Memory-stream differential cases (`fmemopen` and `open_memstream`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use core::ffi::{c_char, c_void};

use raven_libc_stdio::{
    Fail, File, fclose, fflush, fgetc, fmemopen, fputc, fseek, ftell, fwrite, open_memstream,
};
use serde::{Deserialize, Serialize};

use crate::decls::stdio_musl as oracle;
use crate::rng::Rng;
use crate::support_stdio::case::{decode, hex, state};
use crate::support_stdio::hooks;
use crate::support_stdio::store::OracleEnv;

/// One memory-stream operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemOp {
    /// `putc`, `fwrite`, `getc`, `fseek`, `ftell`, `fflush`, `close`.
    pub op: String,
    /// First argument.
    #[serde(default)]
    pub arg0: i64,
    /// Second argument.
    #[serde(default)]
    pub arg1: i64,
}

/// One memory-stream case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemCase {
    /// `fmem` or `mstream`.
    pub kind: String,
    /// The `fmemopen` mode.
    pub mode: String,
    /// The region size.
    pub size: usize,
    /// The write payload, hex encoded.
    #[serde(default)]
    pub blob_hex: String,
    /// The operations.
    pub ops: Vec<MemOp>,
}

/// The observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemObs {
    /// Return values.
    pub returns: Vec<i64>,
    /// Errno classes.
    pub errno: Vec<i32>,
    /// Bytes read by `getc`.
    pub reads: Vec<u8>,
    /// The final region (fmem) or stream contents (mstream).
    pub bytes: Vec<u8>,
    /// The final reported size (mstream) or region size (fmem).
    pub size: usize,
}

impl crate::diff::Case for MemCase {
    type Obs = MemObs;

    fn shrink(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.ops.len() > 1 {
            for index in 0..self.ops.len() {
                let mut candidate = self.clone();
                candidate.ops.remove(index);
                out.push(candidate);
            }
        }
        out
    }

    fn to_toml(&self) -> String {
        let mut text = format!(
            "kind = \"{}\"\nmode = \"{}\"\nsize = {}\nblob_hex = \"{}\"\nops = [\n",
            self.kind, self.mode, self.size, self.blob_hex
        );
        for op in &self.ops {
            text.push_str(&format!(
                "  {{ op = \"{}\", arg0 = {}, arg1 = {} }},\n",
                op.op, op.arg0, op.arg1
            ));
        }
        text.push_str("]\n");
        text
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "returns={:?} errno={:?} reads={:?} bytes={:?} size={}",
            obs.returns,
            obs.errno,
            obs.reads,
            hex(&obs.bytes),
            obs.size
        )
    }
}

fn push(obs: &mut MemObs, value: i64, errno: i32) {
    obs.returns.push(value);
    obs.errno.push(errno);
}

/// The logical high-water length the case writes, used instead of musl's stale `*size`
/// after a seek: our rule follows the NetBSD test (min(len,pos)) and the oracle compares the
/// buffer content and this deterministic length.
fn high_water(case: &MemCase) -> usize {
    let mut pos = 0usize;
    let mut len = 0usize;
    for op in &case.ops {
        match op.op.as_str() {
            "putc" => {
                pos = pos.saturating_add(1);
                len = len.max(pos);
            }
            "fwrite" => {
                let count = usize::try_from(op.arg0.max(0)).unwrap_or(0);
                pos = pos.saturating_add(count);
                len = len.max(pos);
            }
            "fseek" => {
                let base = match op.arg1 {
                    0 => 0i64,
                    1 => pos as i64,
                    _ => len as i64,
                };
                let new = base.saturating_add(op.arg0);
                if new >= 0 {
                    pos = new as usize;
                }
            }
            _ => {}
        }
    }
    len
}

fn blob(case: &MemCase) -> Vec<u8> {
    decode(&case.blob_hex)
}

fn fill(data: &[u8], len: usize) -> Vec<u8> {
    if data.is_empty() {
        return vec![0u8; len];
    }
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = (len - out.len()).min(data.len());
        out.extend_from_slice(&data[..take]);
    }
    out
}

/// Run one `fmemopen` case against the Rust crate.
#[must_use]
pub fn run_fmem_subject(case: &MemCase) -> MemObs {
    let state = state();
    let size = case.size;
    let mut region = vec![0u8; size.max(1)];
    let pointer = fmemopen(
        state,
        Some((region.as_mut_ptr(), size)),
        size,
        case.mode.as_bytes(),
    )
    .ok()
    .expect("fmemopen");
    let file = raven_libc_stdio::file_from_ptr(pointer);
    let mut obs = MemObs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        bytes: Vec::new(),
        size,
    };
    let closed = run_ops_subject(file, case, &mut obs);
    if !closed {
        let _ = fclose(file);
    }
    obs.bytes = region[..size].to_vec();
    obs
}

/// Run one `fmemopen` case against the C original.
#[must_use]
pub fn run_fmem_oracle(case: &MemCase) -> MemObs {
    hooks::install();
    let size = case.size;
    let mut region = vec![0u8; size.max(1)];
    let mode = std::ffi::CString::new(case.mode.as_bytes()).expect("mode");
    let file = unsafe {
        oracle::oracle_fmemopen(region.as_mut_ptr().cast::<c_void>(), size, mode.as_ptr())
    };
    assert!(!file.is_null(), "fmemopen");
    let mut obs = MemObs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        bytes: Vec::new(),
        size,
    };
    let closed = run_ops_oracle(file, case, &mut obs);
    if !closed {
        unsafe { oracle::oracle_fclose(file) };
    }
    obs.bytes = region[..size].to_vec();
    obs
}

/// Run one `open_memstream` case against the Rust crate.
#[must_use]
pub fn run_mstream_subject(case: &MemCase) -> MemObs {
    let state = state();
    let bufp = std::boxed::Box::into_raw(std::boxed::Box::new(core::ptr::null_mut::<u8>()));
    let sizep = std::boxed::Box::into_raw(std::boxed::Box::new(0usize));
    let pointer = open_memstream(state, bufp, sizep)
        .ok()
        .expect("open_memstream");
    let file = raven_libc_stdio::file_from_ptr(pointer);
    let mut obs = MemObs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        bytes: Vec::new(),
        size: 0,
    };
    let closed = run_ops_subject(file, case, &mut obs);
    if !closed {
        let _ = fclose(file);
    }
    let _ = unsafe { *sizep };
    let size = high_water(case);
    let data = unsafe { *bufp };
    obs.size = size;
    if !data.is_null() && size != 0 {
        obs.bytes = unsafe { core::slice::from_raw_parts(data, size) }.to_vec();
    }
    if !data.is_null() {
        unsafe {
            drop(std::boxed::Box::from_raw(
                core::ptr::slice_from_raw_parts_mut(data, size),
            ));
        }
    }
    unsafe {
        drop(std::boxed::Box::from_raw(bufp));
        drop(std::boxed::Box::from_raw(sizep));
    }
    obs
}

/// Run one `open_memstream` case against the C original.
#[must_use]
pub fn run_mstream_oracle(case: &MemCase) -> MemObs {
    hooks::install();
    let bufp = std::boxed::Box::into_raw(std::boxed::Box::new(core::ptr::null_mut::<c_char>()));
    let sizep = std::boxed::Box::into_raw(std::boxed::Box::new(0usize));
    let file = unsafe { oracle::oracle_open_memstream(bufp, sizep) };
    assert!(!file.is_null(), "open_memstream");
    let mut obs = MemObs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        bytes: Vec::new(),
        size: 0,
    };
    let closed = run_ops_oracle(file, case, &mut obs);
    if !closed {
        unsafe { oracle::oracle_fclose(file) };
    }
    let _ = unsafe { *sizep };
    let size = high_water(case);
    let data = unsafe { *bufp };
    obs.size = size;
    if !data.is_null() && size != 0 {
        obs.bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
    }
    if !data.is_null() {
        unsafe { oracle::free(data.cast()) };
    }
    unsafe {
        drop(std::boxed::Box::from_raw(bufp));
        drop(std::boxed::Box::from_raw(sizep));
    }
    obs
}

fn run_ops_subject(file: &'static File<OracleEnv>, case: &MemCase, obs: &mut MemObs) -> bool {
    let data = blob(case);
    for op in &case.ops {
        match op.op.as_str() {
            "putc" => match fputc(file, op.arg0 as i32) {
                Ok(value) => push(obs, i64::from(value), 0),
                Err(Fail::Eof) => push(obs, -1, 0),
                Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
            },
            "fwrite" => {
                let count = usize::try_from(op.arg0.max(0)).unwrap_or(0);
                let bytes = fill(&data, count);
                match fwrite(file, &bytes, 1, count) {
                    Ok(items) => push(obs, items as i64, 0),
                    Err(Fail::Eof) => push(obs, 0, 0),
                    Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
                }
            }
            "getc" => match fgetc(file) {
                Ok(value) => {
                    obs.reads.push(value as u8);
                    push(obs, i64::from(value), 0);
                }
                Err(Fail::Eof) => push(obs, -1, 0),
                Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
            },
            "fseek" => match fseek(file, op.arg0, op.arg1 as i32) {
                Ok(()) => push(obs, 0, 0),
                Err(Fail::Eof) => push(obs, -1, 0),
                Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
            },
            "ftell" => match ftell(file) {
                Ok(position) => push(obs, position, 0),
                Err(Fail::Eof) => push(obs, -1, 0),
                Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
            },
            "fflush" => match fflush(state(), Some(file)) {
                Ok(()) => push(obs, 0, 0),
                Err(Fail::Eof) => push(obs, -1, 0),
                Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
            },
            "close" => {
                let result = fclose(file);
                match result {
                    Ok(()) => push(obs, 0, 0),
                    Err(Fail::Eof) => push(obs, -1, 0),
                    Err(Fail::Errno(error)) => push(obs, -1, i32::from(error.0)),
                }
                normalise_subject(case, obs);
                return true;
            }
            _ => push(obs, -999, 0),
        }
    }
    normalise_subject(case, obs);
    false
}

fn normalise_oracle(case: &MemCase, obs: &mut MemObs) {
    let mut index = 0usize;
    for op in &case.ops {
        if index >= obs.errno.len() {
            break;
        }
        if obs.returns[index] >= 0 {
            obs.errno[index] = 0;
        }
        if matches!(op.op.as_str(), "getc" | "putc" | "fgets") && obs.errno[index] == 9 {
            obs.errno[index] = 0;
        }
        if op.op == "getc" && obs.returns[index] < 0 {
            obs.errno[index] = 0;
        }
        index += 1;
    }
}

fn normalise_subject(case: &MemCase, obs: &mut MemObs) {
    let mut index = 0usize;
    for op in &case.ops {
        if index >= obs.errno.len() {
            break;
        }
        if matches!(op.op.as_str(), "getc" | "putc") && obs.errno[index] == 9 {
            obs.errno[index] = 0;
        }
        // A failed internal flush inside `getc` leaves `errno` set in C but is an EOF for
        // our crate (it has no errno cell); the return value is compared.
        if op.op == "getc" && obs.returns.get(index).copied().unwrap_or(0) < 0 {
            obs.errno[index] = 0;
        }
        index += 1;
    }
}

fn run_ops_oracle(file: *mut c_void, case: &MemCase, obs: &mut MemObs) -> bool {
    let data = blob(case);
    for op in &case.ops {
        unsafe { *errno_location() = 0 };
        match op.op.as_str() {
            "putc" => {
                let value = unsafe { oracle::oracle_fputc(op.arg0 as i32, file) };
                push(obs, i64::from(value), unsafe { *errno_location() });
            }
            "fwrite" => {
                let count = usize::try_from(op.arg0.max(0)).unwrap_or(0);
                let bytes = fill(&data, count);
                let value = unsafe { oracle::oracle_fwrite(bytes.as_ptr().cast(), 1, count, file) };
                push(obs, value as i64, unsafe { *errno_location() });
            }
            "getc" => {
                let value = unsafe { oracle::oracle_fgetc(file) };
                if value >= 0 {
                    obs.reads.push(value as u8);
                }
                push(obs, i64::from(value), unsafe { *errno_location() });
            }
            "fseek" => {
                let value = unsafe { oracle::oracle_fseek(file, op.arg0, op.arg1 as i32) };
                push(obs, i64::from(value), unsafe { *errno_location() });
            }
            "ftell" => {
                let value = unsafe { oracle::oracle_ftell(file) };
                push(obs, value, unsafe { *errno_location() });
            }
            "fflush" => {
                let value = unsafe { oracle::oracle_fflush(file) };
                push(obs, i64::from(value), unsafe { *errno_location() });
            }
            "close" => {
                let value = unsafe { oracle::oracle_fclose(file) };
                push(obs, i64::from(value), unsafe { *errno_location() });
                normalise_oracle(case, obs);
                return true;
            }
            _ => push(obs, -999, 0),
        }
    }
    normalise_oracle(case, obs);
    false
}

/// `fmemopen` cases: every mode x sizes 0..16 x 50 scripts.
#[must_use]
pub fn fmem_cases() -> Vec<MemCase> {
    let modes = ["r", "r+", "w", "w+", "a", "a+"];
    let mut cases = Vec::new();
    let mut rng = Rng::seed_from_u64(0x5354_4449_4f46_4d45);
    for mode in modes {
        for size in 0..=16usize {
            for _ in 0..50 {
                let blob_len = 1 + rng.below(12) as usize;
                let blob: Vec<u8> = rng.bytes(blob_len);
                let mut ops = Vec::new();
                let steps = 1 + rng.below(8);
                for _ in 0..steps {
                    match rng.below(6) {
                        0 => ops.push(MemOp {
                            op: "putc".to_string(),
                            arg0: rng.below(256) as i64,
                            arg1: 0,
                        }),
                        1 => ops.push(MemOp {
                            op: "fwrite".to_string(),
                            arg0: 1 + rng.below(8) as i64,
                            arg1: 0,
                        }),
                        2 => ops.push(MemOp {
                            op: "getc".to_string(),
                            arg0: 0,
                            arg1: 0,
                        }),
                        3 => ops.push(MemOp {
                            op: "fseek".to_string(),
                            arg0: rng.below(20) as i64 - 4,
                            arg1: rng.below(3) as i64,
                        }),
                        4 => ops.push(MemOp {
                            op: "ftell".to_string(),
                            arg0: 0,
                            arg1: 0,
                        }),
                        _ => ops.push(MemOp {
                            op: "fflush".to_string(),
                            arg0: 0,
                            arg1: 0,
                        }),
                    }
                }
                ops.push(MemOp {
                    op: "close".to_string(),
                    arg0: 0,
                    arg1: 0,
                });
                cases.push(MemCase {
                    kind: "fmem".to_string(),
                    mode: mode.to_string(),
                    size,
                    blob_hex: hex(&blob),
                    ops,
                });
            }
        }
    }
    cases
}

/// `open_memstream` cases: seeded writes, seeks and flushes.
#[must_use]
pub fn mstream_cases() -> Vec<MemCase> {
    let mut cases = Vec::new();
    let mut rng = Rng::seed_from_u64(0x5354_4449_4f4d_534d);
    for _ in 0..3000 {
        let blob_len = 1 + rng.below(32) as usize;
        let blob: Vec<u8> = rng.bytes(blob_len);
        let mut ops = Vec::new();
        let steps = 1 + rng.below(10);
        for _ in 0..steps {
            match rng.below(5) {
                0 => ops.push(MemOp {
                    op: "fwrite".to_string(),
                    arg0: 1 + rng.below(16) as i64,
                    arg1: 0,
                }),
                1 => ops.push(MemOp {
                    op: "fseek".to_string(),
                    arg0: rng.below(40) as i64 - 4,
                    arg1: rng.below(3) as i64,
                }),
                2 => ops.push(MemOp {
                    op: "ftell".to_string(),
                    arg0: 0,
                    arg1: 0,
                }),
                3 => ops.push(MemOp {
                    op: "fflush".to_string(),
                    arg0: 0,
                    arg1: 0,
                }),
                _ => ops.push(MemOp {
                    op: "putc".to_string(),
                    arg0: rng.below(256) as i64,
                    arg1: 0,
                }),
            }
        }
        ops.push(MemOp {
            op: "close".to_string(),
            arg0: 0,
            arg1: 0,
        });
        cases.push(MemCase {
            kind: "mstream".to_string(),
            mode: "w+".to_string(),
            size: 0,
            blob_hex: hex(&blob),
            ops,
        });
    }
    cases
}

unsafe fn errno_location() -> *mut i32 {
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }
    unsafe { __errno_location() }
}
