// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers of the regex vector drivers.
//!
//! The `.in` format is the NetBSD `tests/lib/libc/regex/README` format; the `.dat` format is
//! the AT&T `testregex` format described in `tests/data/att/README`. Both drivers are written
//! from those READMEs; the donor `main.c`/`t_regex_att.c` were read for behaviour only and no
//! line of them is reproduced (they are BSD-4, STUDY).

#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Serialise the memory-measuring tests of one integration test binary.
static MEMORY_LOCK: Mutex<()> = Mutex::new(());

/// Take the memory-measurement lock; hold the guard for the whole test.
pub fn serial() -> MutexGuard<'static, ()> {
    MEMORY_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

use raven_libc_regex::consts::{
    REG_BACKR, REG_EXTENDED, REG_GNU, REG_ICASE, REG_LARGE, REG_NEWLINE, REG_NOSPEC, REG_NOSUB,
    REG_NOTBOL, REG_NOTEOL, REG_PEND, REG_STARTEND, REG_TRACE,
};
use raven_libc_regex::{RegMatch, regatoi};

/// The donor driver's `NSUBS`.
pub const NSUBS: usize = 10;

/// Read one file below `tests/data`.
pub fn read_data(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Split `.in` fields: one or more tabs separate fields (the donor's `split` with "\t\t").
pub fn split_tabs(line: &str) -> Vec<&str> {
    line.split('\t').filter(|field| !field.is_empty()).collect()
}

/// Apply the `.in` `N S T Z` substitutions to a pattern or subject.
pub fn fixstr(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .map(|&byte| match byte {
            b'N' => b'\n',
            b'S' => b' ',
            b'T' => b'\t',
            b'Z' => 0,
            other => other,
        })
        .collect()
}

/// Decode a text field of the `.in` format (`""` is empty).
pub fn text_field(field: &str) -> Vec<u8> {
    if field == "\"\"" {
        Vec::new()
    } else {
        field.as_bytes().to_vec()
    }
}

/// `options('c'|'e', flags)` from the donor driver, for one compile variant.
pub fn flags_for(field: &str, extended: bool, exec: bool) -> u32 {
    let mut value = if exec {
        0
    } else if extended {
        REG_EXTENDED
    } else {
        0
    };
    for byte in field.bytes() {
        if exec {
            match byte {
                b'^' => value |= REG_NOTBOL,
                b'$' => value |= REG_NOTEOL,
                b'#' => value |= REG_STARTEND,
                b't' => value |= REG_TRACE,
                b'l' => value |= REG_LARGE,
                b'r' => value |= REG_BACKR,
                _ => {}
            }
        } else {
            match byte {
                b'b' => value &= !REG_EXTENDED,
                b'i' => value |= REG_ICASE,
                b's' => value |= REG_NOSUB,
                b'n' => value |= REG_NEWLINE,
                b'm' => {
                    value &= !REG_EXTENDED;
                    value |= REG_NOSPEC;
                }
                b'p' => value |= REG_PEND,
                b'g' => value |= REG_GNU,
                _ => {}
            }
        }
    }
    value
}

/// A `strunvis`-style decoder for the AT&T `$` flag lines.
pub fn unvis(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(&byte) = input.get(at) {
        at += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(&escape) = input.get(at) else {
            out.push(b'\\');
            break;
        };
        at += 1;
        let value = match escape {
            b'n' => 10,
            b't' => 9,
            b'r' => 13,
            b'b' => 8,
            b'a' => 7,
            b'f' => 12,
            b'v' => 11,
            b'\\' => 92,
            b'x' => {
                let mut value = 0u8;
                let mut digits = 0;
                while digits < 2 {
                    let Some(&hex) = input.get(at) else { break };
                    let Some(digit) = hex_digit(hex) else { break };
                    value = value.wrapping_mul(16).wrapping_add(digit);
                    at += 1;
                    digits += 1;
                }
                value
            }
            b'0'..=b'7' => {
                let mut value = escape.wrapping_sub(b'0');
                let mut digits = 1;
                while digits < 3 {
                    let Some(&octal) = input.get(at) else { break };
                    if !(b'0'..=b'7').contains(&octal) {
                        break;
                    }
                    value = value.wrapping_mul(8).wrapping_add(octal.wrapping_sub(b'0'));
                    at += 1;
                    digits += 1;
                }
                value
            }
            other => other,
        };
        out.push(value);
    }
    out
}

/// One hexadecimal digit, or `None`.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(byte.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

/// How one expected (sub)match is written in the `.in` format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expected {
    /// No match (`-`).
    NoMatch,
    /// The literal text, optionally a null match at the position before a suffix.
    Text {
        /// Bytes the match must equal (raw field form).
        text: Vec<u8>,
        /// The `@suffix` of the `.in` format.
        null_at: Option<Vec<u8>>,
    },
}

/// Parse one expected `.in` field.
pub fn parse_expected(field: &str) -> Expected {
    if field == "-" {
        return Expected::NoMatch;
    }
    if let Some(rest) = field.strip_prefix('@') {
        return Expected::Text {
            text: Vec::new(),
            null_at: Some(rest.as_bytes().to_vec()),
        };
    }
    Expected::Text {
        text: text_field(field),
        null_at: None,
    }
}

/// `check`: compare one `RegMatch` against an expectation.
///
/// `subject` is the raw subject field of the vector line: the donor compares the raw field
/// (whose `N S T Z` letters occupy the same one-byte positions as the fixed copy the match
/// ran on), so an embedded NUL can be written in the data without truncating the check.
pub fn check(subject: &[u8], slot: &RegMatch, expected: Option<&Expected>) -> Result<(), String> {
    if slot.rm_so > slot.rm_eo
        || (slot.rm_so == -1 && slot.rm_eo != -1)
        || (slot.rm_so != -1 && slot.rm_eo == -1)
        || (slot.rm_so != -1 && slot.rm_so < 0)
        || (slot.rm_eo != -1 && slot.rm_eo < 0)
    {
        return Err(format!("start {} end {}", slot.rm_so, slot.rm_eo));
    }
    let Some(expected) = expected else {
        return if slot.rm_so == -1 {
            Ok(())
        } else {
            Err("matched but should not have".to_string())
        };
    };
    match expected {
        Expected::NoMatch => {
            if slot.rm_so == -1 {
                Ok(())
            } else {
                Err("did not stay unmatched".to_string())
            }
        }
        Expected::Text { text, null_at } => {
            if slot.rm_so == -1 {
                return Err("did not match".to_string());
            }
            if slot.rm_eo > subject.len() as i64 {
                return Err(format!(
                    "start {} end {}, past end of string",
                    slot.rm_so, slot.rm_eo
                ));
            }
            let start = slot.rm_so as usize;
            let end = slot.rm_eo as usize;
            let actual = subject.get(start..end).unwrap_or(&[]);
            if actual != text.as_slice() {
                return Err(format!(
                    "matched {:?} instead of {:?}",
                    String::from_utf8_lossy(actual),
                    String::from_utf8_lossy(text)
                ));
            }
            if let Some(at) = null_at {
                if slot.rm_so != slot.rm_eo {
                    return Err("expected a null match".to_string());
                }
                if at.is_empty() {
                    if start != subject.len() {
                        return Err("null match not at end of string".to_string());
                    }
                } else if !subject.get(start..).unwrap_or(&[]).starts_with(at) {
                    return Err(format!(
                        "null match at wrong place (wanted {:?})",
                        String::from_utf8_lossy(at)
                    ));
                }
            }
            Ok(())
        }
    }
}

/// The expected error code of a `.in` `C` line: field 2 is the name minus `REG_`.
pub fn error_code(name: &str) -> Option<i32> {
    let mut full = b"REG_".to_vec();
    full.extend_from_slice(name.as_bytes());
    match regatoi(&full) {
        0 => None,
        code => Some(code),
    }
}

/// One parsed `.in` line.
#[derive(Clone, Debug)]
pub struct InLine {
    /// Pattern field, raw.
    pub pattern: Vec<u8>,
    /// Flag letters.
    pub flags: String,
    /// Subject field, raw.
    pub subject: Vec<u8>,
    /// Expected whole match.
    pub expected: Option<Expected>,
    /// Expected subexpressions.
    pub subexpr: Option<Vec<Expected>>,
}

/// Parse one `.in` line; `None` for a comment or a blank line.
pub fn parse_in_line(line: &str) -> Option<Result<InLine, String>> {
    if line.starts_with('#') || line.is_empty() {
        return None;
    }
    let fields = split_tabs(line);
    if fields.len() < 3 {
        return Some(Err(format!("bad input line: {line:?}")));
    }
    let pattern = text_field(fields[0]);
    let flags = fields[1].to_string();
    let subject = text_field(fields[2]);
    let expected = fields.get(3).map(|field| parse_expected(field));
    let subexpr = fields.get(4).map(|field| {
        if field.is_empty() {
            vec![Expected::Text {
                text: Vec::new(),
                null_at: None,
            }]
        } else {
            field.split(',').map(parse_expected).collect()
        }
    });
    Some(Ok(InLine {
        pattern,
        flags,
        subject,
        expected,
        subexpr,
    }))
}

/// Record and print the per-file counts.
#[derive(Default)]
pub struct Counts {
    /// Lines in the file.
    pub lines: usize,
    /// Lines executed.
    pub executed: usize,
    /// Lines skipped.
    pub skipped: usize,
    /// Reason for each skipped line.
    pub reasons: Vec<String>,
}

impl Counts {
    /// Print the donor driver's summary line.
    pub fn report(&self, file: &str, mode: &str) {
        println!(
            "{file} [{mode}]: {} lines, {} executed, {} skipped",
            self.lines, self.executed, self.skipped
        );
        for reason in &self.reasons {
            println!("  skipped: {reason}");
        }
    }
}

/// Bytes currently allocated through [`CountingAllocator`].
static CURRENT: AtomicUsize = AtomicUsize::new(0);
/// Highest [`CURRENT`] since the last [`reset_peak`].
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// [`CURRENT`] at the last [`reset_peak`].
static BASELINE: AtomicUsize = AtomicUsize::new(0);

/// The seven `t_exhaust.c` pattern shapes, re-implemented from their behaviour.
pub const EXHAUST_MAXSIZE: usize = 9999;
/// The compile flags of each shape (p6 is a BRE).
pub const EXHAUST_FLAGS: [u32; 7] = [1, 1, 1, 1, 1, 1, 0];

/// Repeat `unit` `count` times.
fn repeat(unit: &[u8], count: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..count {
        out.extend_from_slice(unit);
    }
    out
}

/// Build one exhaust shape; `index` is 0..=6 and `len` the generator length.
pub fn exhaust_shape(index: usize, len: usize) -> Vec<u8> {
    match index {
        0 => {
            let mut out = b"(".to_vec();
            out.extend(repeat(b"\\(", len));
            out.push(b')');
            out
        }
        1 => {
            let mut out = b"(".to_vec();
            out.extend(repeat(b"\\(", 60));
            out.extend(repeat(b"(.*)", len));
            out.push(b')');
            out
        }
        2 => exhaust_ps(b"((.*){0,255}", b")", len),
        3 => exhaust_ps(b"(.\\{0,}", b")", len),
        4 => exhaust_ps(b"((.*){1,255}", b")", len),
        5 => exhaust_ps(b"(", b"){1,100}", len),
        _ => {
            let mut out = repeat(b"(?:(.*)|", len);
            out.extend_from_slice(b"(.*)");
            out.extend(repeat(b")", len));
            out
        }
    }
}

/// The donor's `ps`: `(.?)` then the marker and the suffix, both `len` times.
fn exhaust_ps(marker: &[u8], suffix: &[u8], len: usize) -> Vec<u8> {
    let mut out = b"(.?)".to_vec();
    out.extend(repeat(marker, len));
    out.extend(repeat(suffix, len));
    out
}

/// A pass-through allocator that records the live bytes and their peak.
///
/// Test-only (an integration test crate); the shipped crate never allocates through it.
pub struct CountingAllocator;

// SAFETY: every method forwards exactly the layout it received to `System`; the counters only
// observe successful allocations and the matching deallocation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller of the global allocator passes a valid layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let current = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(current, Ordering::SeqCst);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::SeqCst);
        // SAFETY: `pointer` came from `System.alloc` with the same layout.
        unsafe { System.dealloc(pointer, layout) };
    }
}

/// Start a new peak measurement at the current live-byte count.
pub fn reset_peak() {
    let current = CURRENT.load(Ordering::SeqCst);
    BASELINE.store(current, Ordering::SeqCst);
    PEAK.store(current, Ordering::SeqCst);
}

/// Peak live bytes since the last [`reset_peak`].
pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::SeqCst)
        .saturating_sub(BASELINE.load(Ordering::SeqCst))
}

/// Live bytes right now.
pub fn live_bytes() -> usize {
    CURRENT.load(Ordering::SeqCst)
}
