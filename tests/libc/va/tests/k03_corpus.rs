// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The K03 corpus (57 cases) through the real `t_snprintf` trampoline and the real engine,
//! compared with the host glibc through a generated C driver.
//!
//! The corpus is data (`corpus/k03.toml`) extracted from the project's own K03 spike; the
//! spike's printf is not reused (it has `core::fmt` and `f64`).

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/driver.rs"]
mod driver;

use driver::{Case, CaseArg, CaseDriver};

// The one expansion of the variadic table with the test prefix; also gives `t_snprintf`.
#[path = "../src/table_ut.rs"]
mod table;
#[path = "../src/targets.rs"]
mod targets;

unsafe extern "C" {
    fn t_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
}

/// One parsed corpus entry.
struct Entry {
    id: usize,
    group: String,
    case: Case,
    exclude: Option<String>,
}

fn parse_args(text: &str) -> Vec<CaseArg> {
    let mut args = Vec::new();
    let trimmed = text.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.trim().is_empty() {
        return args;
    }
    for item in trimmed.split("\", \"") {
        let item = item.trim().trim_matches('"');
        let Some((class, value)) = item.split_once(':') else {
            panic!("bad argument `{item}`");
        };
        let argument = match class {
            "i32" => CaseArg::I32(value.parse().expect("i32")),
            "u32" => CaseArg::U32(value.parse().expect("u32")),
            "i64" => CaseArg::I64(value.parse().expect("i64")),
            "u64" => CaseArg::U64(value.parse().expect("u64")),
            "f64" => {
                let bits = value.trim_start_matches("0x");
                CaseArg::F64(u64::from_str_radix(bits, 16).expect("f64 bits"))
            }
            "str" => CaseArg::Str(value.to_string()),
            "null" => match value {
                "str" => CaseArg::Null,
                "ptr" => CaseArg::Ptr(0),
                other => panic!("bad null class `{other}`"),
            },
            "ptr" => {
                let bits = value.trim_start_matches("0x");
                CaseArg::Ptr(u64::from_str_radix(bits, 16).expect("ptr") as usize)
            }
            other => panic!("unknown argument class `{other}`"),
        };
        args.push(argument);
    }
    args
}

fn load_corpus() -> Vec<Entry> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/k03.toml");
    let text = std::fs::read_to_string(&path).expect("corpus/k03.toml");
    let mut entries = Vec::new();
    let mut id = 0usize;
    let mut group = String::new();
    let mut fmt = String::new();
    let mut args: Vec<CaseArg> = Vec::new();
    let mut exclude = None;
    let flush = |id: &mut usize,
                 group: &mut String,
                 fmt: &mut String,
                 args: &mut Vec<CaseArg>,
                 exclude: &mut Option<String>,
                 entries: &mut Vec<Entry>| {
        if *id != 0 {
            entries.push(Entry {
                id: *id,
                group: group.clone(),
                case: Case {
                    fmt: fmt.clone(),
                    args: core::mem::take(args),
                },
                exclude: exclude.take(),
            });
            *id = 0;
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("[[case]]") {
            flush(
                &mut id,
                &mut group,
                &mut fmt,
                &mut args,
                &mut exclude,
                &mut entries,
            );
            continue;
        }
        if let Some(value) = line.strip_prefix("id = ") {
            id = value.parse().expect("id");
        } else if let Some(value) = line.strip_prefix("group = ") {
            group = value.trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("fmt = ") {
            fmt = value.trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("args = ") {
            args = parse_args(value);
        } else if let Some(value) = line.strip_prefix("exclude = ") {
            exclude = Some(value.trim_matches('"').to_string());
        }
    }
    flush(
        &mut id,
        &mut group,
        &mut fmt,
        &mut args,
        &mut exclude,
        &mut entries,
    );
    entries
}

#[test]
fn the_corpus_has_the_fifty_seven_k03_cases() {
    let entries = load_corpus();
    assert_eq!(entries.len(), 57);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, index + 1);
    }
    let excluded = entries
        .iter()
        .filter(|entry| entry.exclude.is_some())
        .count();
    assert_eq!(
        excluded, 1,
        "the `%p` NULL case is the one documented exclusion"
    );
}

#[test]
fn k03_differential_through_the_real_trampoline() {
    let entries = load_corpus();
    let cases: Vec<Case> = entries.iter().map(|entry| entry.case.clone()).collect();
    let driver = match CaseDriver::build(&cases) {
        Ok(Some(driver)) => driver,
        Ok(None) => {
            println!("skipped: clang not found");
            return;
        }
        Err(error) => panic!("driver build failed: {error}"),
    };
    let ours = t_snprintf as *const () as usize;
    let mut ran = 0usize;
    let mut skipped = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        if let Some(reason) = &entry.exclude {
            println!(
                "case {} ({}): excluded from the glibc comparison: {reason}",
                entry.id, entry.group
            );
            skipped += 1;
            continue;
        }
        let mut buffer = [0u8; 256];
        let capacity = buffer.len();
        let (reference, actual) = driver.compare(index, ours, &mut buffer, capacity);
        assert_eq!(
            actual, reference,
            "case {} ({:?}): return value",
            entry.id, entry.case.fmt
        );
        ran += 1;
    }
    assert_eq!(ran + skipped, 57);
    println!("k03 corpus: {ran} compared, {skipped} excluded");
}

#[test]
fn k03_truncation_and_zero_capacity() {
    // The spike's two non-`check!` cases: a 4-byte buffer and `n = 0` with a NULL buffer.
    let format = b"value=%d\0";
    let mut ours = [0u8; 4];
    let mut reference = [0u8; 4];
    // SAFETY: the format and the argument match; both buffers are writable.
    let ours_len = unsafe { t_snprintf(ours.as_mut_ptr(), ours.len(), format.as_ptr(), 12345i32) };
    // SAFETY: the host snprintf has the same contract.
    let reference_len = unsafe {
        raven_libc_va_test_ref_snprintf(
            reference.as_mut_ptr(),
            reference.len(),
            format.as_ptr(),
            12345i32,
        )
    };
    assert_eq!(ours_len, reference_len);
    assert_eq!(ours, reference);
    // SAFETY: `n = 0` with a NULL buffer is permitted.
    let ours_len = unsafe { t_snprintf(core::ptr::null_mut(), 0, format.as_ptr(), 12345i32) };
    // SAFETY: as above.
    let reference_len = unsafe {
        raven_libc_va_test_ref_snprintf(core::ptr::null_mut(), 0, format.as_ptr(), 12345i32)
    };
    assert_eq!(ours_len, reference_len);
    assert_eq!(ours_len, 11);
}

unsafe extern "C" {
    /// The host glibc `snprintf` (the oracle of the truncation case).
    #[link_name = "snprintf"]
    fn raven_libc_va_test_ref_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
}
