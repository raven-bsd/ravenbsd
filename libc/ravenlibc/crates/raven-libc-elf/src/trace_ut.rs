// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The format strings are fixed literals.

//! The `ldd` line formatter.

use super::*;

fn entry(name: &[u8], path: &[u8], bias: u64) -> TraceEntry {
    TraceEntry {
        name: name.to_vec(),
        path: path.to_vec(),
        bias,
        progname: b"/bin/true".to_vec(),
        argv0: b"/bin/app".to_vec(),
    }
}

#[test]
fn the_default_library_line_uses_the_first_format() {
    let line = format_trace(None, None, &entry(b"libc.so.1", b"/lib/libc.so.1", 0x1000));
    assert_eq!(line, b"\tlibc.so.1 => /lib/libc.so.1 (0x1000)\n");
}

#[test]
fn the_default_non_library_line_uses_the_second_format() {
    let line = format_trace(None, None, &entry(b"app", b"/bin/app", 0x20));
    assert_eq!(line, b"\tapp (0x20)\n");
}

#[test]
fn program_name_argv_and_percent_are_substituted() {
    let format = b"%A %a %o %% %p %x";
    let line = format_trace(
        Some(format),
        Some(format),
        &entry(b"libz.so.1", b"/lib/libz.so.1", 0),
    );
    assert_eq!(line, b"/bin/true /bin/app libz.so.1 % /lib/libz.so.1 0x0");
}

#[test]
fn an_unknown_conversion_is_kept_literally() {
    let line = format_trace(Some(b"%z%o"), Some(b"%z%o"), &entry(b"libz.so.1", b"/z", 0));
    assert_eq!(line, b"%zlibz.so.1");
}

#[test]
fn backslash_escapes_are_expanded() {
    let line = format_trace(
        Some(b"a\\tb\\nc"),
        Some(b"a\\tb\\nc"),
        &entry(b"libz.so.1", b"/z", 0),
    );
    assert_eq!(line, b"a\tb\nc");
}

#[test]
fn a_custom_format_is_used_verbatim() {
    let line = format_trace(
        Some(b"%o=%p\\n"),
        Some(b"%o=%p\\n"),
        &entry(b"libz.so.1", b"/q", 0),
    );
    assert_eq!(line, b"libz.so.1=/q\n");
}

#[test]
fn the_bias_is_lower_case_hexadecimal() {
    let line = format_trace(
        Some(b"%x"),
        Some(b"%x"),
        &entry(b"libz.so.1", b"/z", 0xdead_beef),
    );
    assert_eq!(line, b"0xdeadbeef");
}

#[test]
fn a_trailing_percent_is_kept() {
    let line = format_trace(Some(b"%"), Some(b"%"), &entry(b"libz.so.1", b"/z", 0));
    assert_eq!(line, b"%");
}

#[test]
fn a_non_library_name_uses_the_second_format_even_when_the_first_is_set() {
    let line = format_trace(Some(b"one"), Some(b"two"), &entry(b"app", b"/bin/app", 0));
    assert_eq!(line, b"two");
}

#[test]
fn every_default_format_is_the_documented_one() {
    assert_eq!(DEFAULT_FMT1, b"\t%o => %p (%x)\n");
    assert_eq!(DEFAULT_FMT2, b"\t%o (%x)\n");
}
