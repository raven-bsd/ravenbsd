// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The operation-script case: one format, two runners (the Rust crate and the musl oracle).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::ffi::CString;

use raven_libc_stdio::{
    Fail, File, LineBuf, State, clearerr, fclose, feof, ferror, fflush, fgetc, fgets, fileno,
    fopen, fputc, fputs, fread, fseek, ftell, fwrite, getdelim, rewind, setvbuf, ungetc,
};
use serde::{Deserialize, Serialize};

use crate::decls::stdio_musl as oracle;
use crate::support_stdio::hooks;
use crate::support_stdio::store::{OracleEnv, TraceOp, store};

/// One script record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptOp {
    /// The operation name.
    pub op: String,
    /// First argument.
    #[serde(default)]
    pub arg0: i64,
    /// Second argument.
    #[serde(default)]
    pub arg1: i64,
    /// Third argument.
    #[serde(default)]
    pub arg2: i64,
}

/// One operation script and its initial files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptCase {
    /// Initial contents of `/f0`..`/f3`, hex encoded.
    #[serde(default)]
    pub files_hex: Vec<String>,
    /// Payloads for `fwrite`/`fputs`, hex encoded.
    #[serde(default)]
    pub blobs_hex: Vec<String>,
    /// The `fopen` mode string used by every `open` record.
    pub mode: String,
    /// The operations.
    pub ops: Vec<ScriptOp>,
}

/// The observation compared by the differential runner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Obs {
    /// Every documented return value, in call order.
    pub returns: Vec<i64>,
    /// The errno after each call (0 when untouched).
    pub errno: Vec<i32>,
    /// The bytes produced by `fread`/`fgets`/`getdelim`.
    pub reads: Vec<Vec<u8>>,
    /// The bytes offered to `fwrite`/`fputs`.
    pub writes: Vec<Vec<u8>>,
    /// The final contents of `/f0`..`/f3`.
    pub bytes: Vec<Vec<u8>>,
    /// The filtered syscall trace (open/write/seek/close).
    pub trace: Vec<TraceOp>,
}

impl ScriptCase {
    /// Decode the files.
    #[must_use]
    pub fn files(&self) -> Vec<Vec<u8>> {
        self.files_hex.iter().map(|hex| decode(hex)).collect()
    }

    /// Decode the payloads.
    #[must_use]
    pub fn blobs(&self) -> Vec<Vec<u8>> {
        self.blobs_hex.iter().map(|hex| decode(hex)).collect()
    }
}

impl crate::diff::Case for ScriptCase {
    type Obs = Obs;

    fn shrink(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.ops.len() > 2 {
            for index in 1..self.ops.len() {
                let mut candidate = self.clone();
                candidate.ops.remove(index);
                out.push(candidate);
            }
        }
        out
    }

    fn to_toml(&self) -> String {
        let mut text = String::new();
        text.push_str("files_hex = [");
        for (index, item) in self.files_hex.iter().enumerate() {
            if index != 0 {
                text.push_str(", ");
            }
            text.push('"');
            text.push_str(item);
            text.push('"');
        }
        text.push_str("]\nblobs_hex = [");
        for (index, item) in self.blobs_hex.iter().enumerate() {
            if index != 0 {
                text.push_str(", ");
            }
            text.push('"');
            text.push_str(item);
            text.push('"');
        }
        text.push_str("]\n");
        text.push_str(&format!("mode = \"{}\"\nops = [\n", self.mode));
        for op in &self.ops {
            text.push_str(&format!(
                "  {{ op = \"{}\", arg0 = {}, arg1 = {}, arg2 = {} }},\n",
                op.op, op.arg0, op.arg1, op.arg2
            ));
        }
        text.push_str("]\n");
        text
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        let bytes: Vec<String> = obs.bytes.iter().map(|item| hex(item)).collect();
        format!(
            "returns={:?} errno={:?} reads={:?} bytes={:?} trace={:?}",
            obs.returns, obs.errno, obs.reads, bytes, obs.trace
        )
    }
}

/// Reset the shared world for one case.
pub fn reset(case: &ScriptCase) {
    store().lock().unwrap().reset(&case.files());
}

/// The global subject state.
pub fn state() -> &'static State<OracleEnv> {
    static STATE: std::sync::OnceLock<&'static State<OracleEnv>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| Box::leak(Box::new(State::new(OracleEnv))))
}

/// Run the script against the Rust crate.
#[must_use]
pub fn run_subject(case: &ScriptCase) -> Obs {
    reset(case);
    let state = state();
    let blobs = case.blobs();
    let mode = case.mode.as_bytes();
    let mut file: Option<&'static File<OracleEnv>> = None;
    let mut obs = Obs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        writes: Vec::new(),
        bytes: Vec::new(),
        trace: Vec::new(),
    };
    let mut setvbuf_buffer = [0u8; 2048];
    for record in &case.ops {
        let mut push = |value: i64, errno: i32| {
            obs.returns.push(value);
            obs.errno.push(errno);
        };
        match record.op.as_str() {
            "open" => {
                if let Some(current) = file.take() {
                    let (value, errno) = result_unit(fclose(current));
                    push(value, errno);
                }
                let path = format!("/f{}", record.arg0.clamp(0, 3));
                match fopen(state, path.as_bytes(), mode) {
                    Ok(pointer) => {
                        file = Some(raven_libc_stdio::file_from_ptr(pointer));
                        push(1, 0);
                    }
                    Err(Fail::Eof) => push(0, 0),
                    Err(Fail::Errno(error)) => push(0, i32::from(error.0)),
                }
            }
            "fread" => {
                let count = usize::try_from(record.arg0.max(0)).unwrap_or(0);
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let total = count.saturating_mul(size);
                let mut buffer = vec![0u8; total];
                match file.map(|file| fread(file, &mut buffer, size, count)) {
                    Some(Ok(items)) => {
                        let used = items.saturating_mul(size).min(total);
                        obs.reads.push(buffer[..used].to_vec());
                        push(items as i64, 0);
                    }
                    Some(Err(Fail::Eof)) => push(0, 0),
                    Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                    None => push(-1, 0),
                }
            }
            "fwrite" => {
                let blob = blobs
                    .get(record.arg0.max(0) as usize)
                    .cloned()
                    .unwrap_or_default();
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let count = usize::try_from(record.arg2.max(0)).unwrap_or(0);
                let total = size.saturating_mul(count);
                let data = fill(&blob, total);
                obs.writes.push(data.clone());
                match file.map(|file| fwrite(file, &data, size, count)) {
                    Some(Ok(items)) => push(items as i64, 0),
                    Some(Err(Fail::Eof)) => push(0, 0),
                    Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                    None => push(-1, 0),
                }
            }
            "fgets" => {
                let size = usize::try_from(record.arg0.max(0)).unwrap_or(0).max(1);
                let mut buffer = vec![0u8; size];
                match file.map(|file| fgets(file, &mut buffer)) {
                    Some(Ok(_len)) => {
                        obs.reads.push(buffer[..size].to_vec());
                        push(1, 0);
                    }
                    Some(Err(Fail::Eof)) => push(0, 0),
                    Some(Err(Fail::Errno(error))) => push(0, i32::from(error.0)),
                    None => push(0, 0),
                }
            }
            "fputs" => {
                let raw = blobs
                    .get(record.arg0.max(0) as usize)
                    .cloned()
                    .unwrap_or_default();
                let blob = raw.split(|byte| *byte == 0).next().unwrap_or(&[]).to_vec();
                obs.writes.push(blob.clone());
                match file.map(|file| fputs(file, &blob)) {
                    Some(Ok(())) => push(0, 0),
                    Some(Err(Fail::Eof)) => push(-1, 0),
                    Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                    None => push(-1, 0),
                }
            }
            "getc" => match file.map(fgetc) {
                Some(Ok(value)) => push(i64::from(value), 0),
                Some(Err(Fail::Eof)) => push(-1, 0),
                Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                None => push(-1, 0),
            },
            "putc" => match file.map(|file| fputc(file, record.arg0 as i32)) {
                Some(Ok(value)) => push(i64::from(value), 0),
                Some(Err(Fail::Eof)) => push(-1, 0),
                Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                None => push(-1, 0),
            },
            "ungetc" => match file.map(|file| ungetc(file, record.arg0 as i32)) {
                Some(Ok(value)) => push(i64::from(value), 0),
                Some(Err(Fail::Eof)) => push(-1, 0),
                Some(Err(Fail::Errno(error))) => push(-1, i32::from(error.0)),
                None => push(-1, 0),
            },
            "fseek" => {
                let off = record.arg0;
                let whence = record.arg1 as i32;
                let (value, errno) = match file.map(|file| fseek(file, off, whence)) {
                    Some(Ok(())) => (0, 0),
                    Some(Err(Fail::Eof)) => (-1, 0),
                    Some(Err(Fail::Errno(error))) => (-1, i32::from(error.0)),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            "ftell" => {
                let (value, errno) = match file.map(ftell) {
                    Some(Ok(position)) => (position, 0),
                    Some(Err(Fail::Eof)) => (-1, 0),
                    Some(Err(Fail::Errno(error))) => (-1, i32::from(error.0)),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            "fflush" => {
                let (value, errno) = match file {
                    Some(file) => result_unit(fflush(state, Some(file))),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            "rewind" => {
                if let Some(file) = file {
                    rewind(file);
                }
                push(0, 0);
            }
            "clearerr" => {
                if let Some(file) = file {
                    clearerr(file);
                }
                push(0, 0);
            }
            "feof" => push(i64::from(file.is_some_and(feof)), 0),
            "ferror" => push(i64::from(file.is_some_and(ferror)), 0),
            "fileno" => match file.map(fileno) {
                Some(Ok(fd)) => push(i64::from(fd), 0),
                Some(Err(_)) => push(-1, 9),
                None => push(-1, 9),
            },
            "setvbuf" => {
                let kind = record.arg0 as i32;
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let buffer = if kind == 2 {
                    None
                } else {
                    Some((setvbuf_buffer.as_mut_ptr(), setvbuf_buffer.len()))
                };
                let (value, errno) = match file.map(|file| setvbuf(file, buffer, kind, size)) {
                    Some(Ok(())) => (0, 0),
                    Some(Err(Fail::Eof)) => (-1, 0),
                    Some(Err(Fail::Errno(error))) => (-1, i32::from(error.0)),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            "getdelim" => {
                let delim = record.arg0 as i32;
                let capacity = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let mut data = vec![0u8; capacity];
                let mut line = LineBuf::new(&mut data, capacity);
                let (value, errno) = match file.map(|file| getdelim(file, &mut line, delim)) {
                    Some(Ok(len)) => {
                        obs.reads.push(line.line().to_vec());
                        (len as i64, 0)
                    }
                    Some(Err(Fail::Eof)) => (-1, 0),
                    Some(Err(Fail::Errno(error))) => (-1, i32::from(error.0)),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            "close" => {
                let (value, errno) = match file.take() {
                    Some(file) => result_unit(fclose(file)),
                    None => (-1, 0),
                };
                push(value, errno);
            }
            _ => push(-999, 0),
        }
        normalise_errno(&record.op, &mut obs);
    }
    if let Some(file) = file.take() {
        let (value, errno) = result_unit(fclose(file));
        obs.returns.push(value);
        obs.errno.push(errno);
        normalise_errno("close", &mut obs);
    }
    let world = store().lock().unwrap();
    for index in 0..4 {
        obs.bytes.push(
            world
                .contents
                .get(format!("/f{index}").as_bytes())
                .cloned()
                .unwrap_or_default(),
        );
    }
    obs.trace = world.trace.clone();
    obs
}

/// Run the script against the musl C original.
#[must_use]
pub fn run_oracle(case: &ScriptCase) -> Obs {
    reset(case);
    hooks::install();
    let blobs = case.blobs();
    let mode = CString::new(case.mode.as_bytes()).expect("mode");
    let mut file: *mut core::ffi::c_void = core::ptr::null_mut();
    let mut obs = Obs {
        returns: Vec::new(),
        errno: Vec::new(),
        reads: Vec::new(),
        writes: Vec::new(),
        bytes: Vec::new(),
        trace: Vec::new(),
    };
    let mut setvbuf_buffer = [0u8; 2048];
    for record in &case.ops {
        unsafe { *errno_location() = 0 };
        let mut push = |value: i64| {
            obs.returns.push(value);
            obs.errno.push(unsafe { *errno_location() });
        };
        match record.op.as_str() {
            "open" => {
                if !file.is_null() {
                    let value = unsafe { oracle::oracle_fclose(file) };
                    push(i64::from(value));
                    file = core::ptr::null_mut();
                }
                file = core::ptr::null_mut();
                let path = CString::new(format!("/f{}", record.arg0.clamp(0, 3))).expect("path");
                let opened = unsafe { oracle::oracle_fopen(path.as_ptr(), mode.as_ptr()) };
                if opened.is_null() {
                    push(0);
                } else {
                    file = opened;
                    push(1);
                }
            }
            "fread" => {
                let count = usize::try_from(record.arg0.max(0)).unwrap_or(0);
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let total = count.saturating_mul(size);
                let mut buffer = vec![0u8; total];
                let value = if file.is_null() {
                    usize::MAX
                } else {
                    unsafe { oracle::oracle_fread(buffer.as_mut_ptr().cast(), size, count, file) }
                };
                if value != usize::MAX {
                    let used = value.saturating_mul(size).min(total);
                    obs.reads.push(buffer[..used].to_vec());
                    push(value as i64);
                } else {
                    push(-1);
                }
            }
            "fwrite" => {
                let blob = blobs
                    .get(record.arg0.max(0) as usize)
                    .cloned()
                    .unwrap_or_default();
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let count = usize::try_from(record.arg2.max(0)).unwrap_or(0);
                let total = size.saturating_mul(count);
                let data = fill(&blob, total);
                obs.writes.push(data.clone());
                let value = if file.is_null() {
                    i64::MIN
                } else {
                    unsafe { oracle::oracle_fwrite(data.as_ptr().cast(), size, count, file) as i64 }
                };
                push(if value == i64::MIN { -1 } else { value });
            }
            "fgets" => {
                let size = usize::try_from(record.arg0.max(0)).unwrap_or(0).max(1);
                let mut buffer = vec![0u8; size + 1];
                let value = if file.is_null() {
                    core::ptr::null_mut()
                } else {
                    unsafe { oracle::oracle_fgets(buffer.as_mut_ptr().cast(), size as i32, file) }
                };
                if value.is_null() {
                    push(0);
                } else {
                    obs.reads.push(buffer[..size].to_vec());
                    push(1);
                }
            }
            "fputs" => {
                let raw = blobs
                    .get(record.arg0.max(0) as usize)
                    .cloned()
                    .unwrap_or_default();
                let blob = raw.split(|byte| *byte == 0).next().unwrap_or(&[]).to_vec();
                obs.writes.push(blob.clone());
                let text = CString::new(blob).unwrap_or_default();
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fputs(text.as_ptr(), file) }
                };
                push(i64::from(value));
            }
            "getc" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fgetc(file) }
                };
                push(i64::from(value));
            }
            "putc" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fputc(record.arg0 as i32, file) }
                };
                push(i64::from(value));
            }
            "ungetc" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_ungetc(record.arg0 as i32, file) }
                };
                push(i64::from(value));
            }
            "fseek" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fseek(file, record.arg0, record.arg1 as i32) }
                };
                push(i64::from(value));
            }
            "ftell" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_ftell(file) }
                };
                push(value);
            }
            "fflush" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fflush(file) }
                };
                push(i64::from(value));
            }
            "rewind" => {
                if !file.is_null() {
                    unsafe { oracle::oracle_rewind(file) };
                }
                push(0);
            }
            "clearerr" => {
                if !file.is_null() {
                    unsafe { oracle::oracle_clearerr(file) };
                }
                push(0);
            }
            "feof" => {
                let value = if file.is_null() {
                    0
                } else {
                    unsafe { oracle::oracle_feof(file) }
                };
                push(i64::from(value));
            }
            "ferror" => {
                let value = if file.is_null() {
                    0
                } else {
                    unsafe { oracle::oracle_ferror(file) }
                };
                push(i64::from(value));
            }
            "fileno" => {
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_fileno(file) }
                };
                push(i64::from(value));
            }
            "setvbuf" => {
                let kind = record.arg0 as i32;
                let size = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let pointer = if kind == 2 {
                    core::ptr::null_mut()
                } else {
                    setvbuf_buffer.as_mut_ptr().cast()
                };
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_setvbuf(file, pointer, kind, size) }
                };
                push(i64::from(value));
            }
            "getdelim" => {
                let delim = record.arg0 as i32;
                let capacity = usize::try_from(record.arg1.max(0)).unwrap_or(0);
                let mut lineptr: *mut core::ffi::c_char = core::ptr::null_mut();
                let mut n = 0usize;
                if capacity != 0 {
                    lineptr = unsafe { oracle::malloc(capacity).cast() };
                    n = capacity;
                }
                let value = if file.is_null() {
                    -1
                } else {
                    unsafe { oracle::oracle_getdelim(&mut lineptr, &mut n, delim, file) as i64 }
                };
                if value >= 0 {
                    let bytes = unsafe {
                        core::slice::from_raw_parts(lineptr.cast::<u8>(), value as usize)
                    };
                    obs.reads.push(bytes.to_vec());
                }
                if !lineptr.is_null() {
                    unsafe { oracle::free(lineptr.cast()) };
                }
                push(value);
            }
            "close" => {
                let value = if file.is_null() {
                    -1
                } else {
                    let value = unsafe { oracle::oracle_fclose(file) };
                    file = core::ptr::null_mut();
                    i64::from(value)
                };
                push(value);
            }
            _ => push(-999),
        }
        normalise_errno(&record.op, &mut obs);
    }
    if !file.is_null() {
        let value = unsafe { oracle::oracle_fclose(file) };
        obs.returns.push(i64::from(value));
        obs.errno.push(unsafe { *errno_location() });
        normalise_errno("close", &mut obs);
    }
    let world = store().lock().unwrap();
    for index in 0..4 {
        obs.bytes.push(
            world
                .contents
                .get(format!("/f{index}").as_bytes())
                .cloned()
                .unwrap_or_default(),
        );
    }
    obs.trace = world.trace.clone();
    obs
}

/// The `errno` cell of the host C library.
unsafe fn errno_location() -> *mut i32 {
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }
    unsafe { __errno_location() }
}

/// Drop the C `errno` left by an ignored internal call when the documented result is a
/// success: the crate has no `errno` cell, and success-path `errno` is not part of the
/// contract (4.4). Error returns keep their class.
fn normalise_errno(op: &str, obs: &mut Obs) {
    let Some(&value) = obs.returns.last() else {
        return;
    };
    let success = match op {
        "open" | "fgets" => value != 0,
        "feof" | "ferror" | "rewind" | "clearerr" => true,
        "fread" | "fwrite" | "fileno" => value >= 0,
        _ => value >= 0,
    };
    if success && let Some(errno) = obs.errno.last_mut() {
        *errno = 0;
    }
    // Our rules set EBADF for a byte operation on a stream that cannot perform it
    // (`donor-map/04` 4.4 rows fgetc/fputc); musl's `__toread`/`__towrite` only set `F_ERR`
    // and leave errno alone. The return value is compared; the errno class is normalised.
    if matches!(op, "getc" | "putc" | "fgets" | "getdelim") && obs.errno.last().copied() == Some(9)
    {
        if let Some(errno) = obs.errno.last_mut() {
            *errno = 0;
        }
    }
}

fn result_unit(result: Result<(), Fail>) -> (i64, i32) {
    match result {
        Ok(()) => (0, 0),
        Err(Fail::Eof) => (-1, 0),
        Err(Fail::Errno(error)) => (-1, i32::from(error.0)),
    }
}

fn fill(blob: &[u8], len: usize) -> Vec<u8> {
    if blob.is_empty() {
        return vec![0u8; len];
    }
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = (len - out.len()).min(blob.len());
        out.extend_from_slice(&blob[..take]);
    }
    out
}

/// Decode a hex string.
pub fn decode(text: &str) -> Vec<u8> {
    let raw = text.as_bytes();
    let mut out = Vec::with_capacity(raw.len() / 2);
    let mut index = 0;
    while index + 1 < raw.len() {
        let high = hex_digit(raw[index]);
        let low = hex_digit(raw[index + 1]);
        if let (Some(high), Some(low)) = (high, low) {
            out.push((high << 4) | low);
        }
        index += 2;
    }
    out
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Encode a byte string.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}
