// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The OpenBSD `fnm_test.in` vector table and the NetBSD `t_fnmatch.c` cases.
//!
//! The table is generated data (`tests/data/openbsd_fnm.rs`), not hand-typed. The NetBSD
//! `t_fnmatch.c` cases are seven named tests re-expressed with the FreeBSD flag values
//! (`netbsd:tests/lib/libc/gen/t_fnmatch.c` is STUDY; only its facts are used).

mod common;

use raven_libc_glob::{ByteLocale, Utf8Locale, fnmatch};

include!("data/openbsd_fnm.rs");

const FNM_NOMATCH: i32 = 1;
const FNM_NOESCAPE: u32 = 0x01;
const FNM_PATHNAME: u32 = 0x02;
const FNM_PERIOD: u32 = 0x04;
const FNM_LEADING_DIR: u32 = 0x08;
const FNM_CASEFOLD: u32 = 0x10;

fn m(pat: &str, s: &str, flags: u32) -> i32 {
    fnmatch(pat.as_bytes(), s.as_bytes(), flags, &Utf8Locale)
}

#[test]
fn openbsd_fnm_test_vectors() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for &(pattern, string, flags, expected) in VECTORS {
        if is_known_difference(pattern, string, flags) {
            continue;
        }
        checked += 1;
        let got = fnmatch(pattern, string, flags, &Utf8Locale);
        if got != expected {
            failures.push(format!(
                "pattern {:?} string {:?} flags {flags:#x}: want {expected} got {got}",
                String::from_utf8_lossy(pattern),
                String::from_utf8_lossy(string)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} OpenBSD vectors failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, VECTORS.len() - 2);
}

/// The two OpenBSD vectors whose expected result is not musl's behaviour (the COPY donor).
///
/// `[[:opper:][:alnum:]]` has an unknown class name, which musl's `wctype` maps to `0` and
/// then continues to the next class; musl therefore matches. `[r-T]` is a reversed range;
/// musl's range test requires `wc <= wc2` and never matches, with or without `FNM_CASEFOLD`.
/// Both are OpenBSD implementation choices, not POSIX requirements; the musl result wins
/// because `musl:src/regex/fnmatch.c` is the `COPY` donor (doc 07 4.3).
fn is_known_difference(pattern: &[u8], string: &[u8], flags: u32) -> bool {
    (pattern == b"/bin/[[:opper:][:alnum:]]*" && string == b"/bin/ls" && flags == 0)
        || (pattern == b"te[r-T]t" && string == b"test" && flags == 0x10)
}

#[test]
fn known_openbsd_differences_have_the_musl_result() {
    assert_eq!(
        fnmatch(b"/bin/[[:opper:][:alnum:]]*", b"/bin/ls", 0, &Utf8Locale),
        0
    );
    assert_eq!(fnmatch(b"te[r-T]t", b"test", 0x10, &Utf8Locale), 1);
}

#[test]
fn openbsd_vector_count_is_184() {
    assert_eq!(VECTORS.len(), 184);
}

#[test]
fn netbsd_fnmatch_backslashes() {
    // NetBSD expects a lone trailing `\` not to match a literal backslash; the musl donor
    // treats a backslash with nothing after it as an ordinary character (fnmatch.c:57), so
    // this is a declared donor difference (reported, not hidden).
    assert_eq!(m("\\", "\\", 0), 0);
}

#[test]
fn netbsd_fnmatch_casefold() {
    assert!(m("xxx", "XXX", 0) != 0);
    assert!(m("XXX", "xxx", 0) != 0);
    assert!(m("xxx", "XxX", 0) != 0);
    assert!(m("XxX", "xxx", 0) != 0);
    assert!(m("x*x", "XXX", 0) != 0);
    assert!(m("**x", "XXX", 0) != 0);
    assert!(m("*?x", "XXX", 0) != 0);

    assert_eq!(m("xxx", "XXX", FNM_CASEFOLD), 0);
    assert_eq!(m("XXX", "xxx", FNM_CASEFOLD), 0);
    assert_eq!(m("xxx", "XxX", FNM_CASEFOLD), 0);
    assert_eq!(m("XxX", "xxx", FNM_CASEFOLD), 0);
    assert_eq!(m("x*x", "XXX", FNM_CASEFOLD), 0);
    assert_eq!(m("**x", "XXX", FNM_CASEFOLD), 0);
    assert_eq!(m("*?x", "XXX", FNM_CASEFOLD), 0);
}

#[test]
fn netbsd_fnmatch_leadingdir() {
    assert!(m("", "/*", 0) != 0);
    assert!(m(" ", " /*", 0) != 0);
    assert!(m("x", "x/*", 0) != 0);
    assert!(m("///", "////*", 0) != 0);

    assert_eq!(m("", "/*", FNM_LEADING_DIR), 0);
    assert_eq!(m(" ", " /*", FNM_LEADING_DIR), 0);
    assert_eq!(m("x", "x/*", FNM_LEADING_DIR), 0);
    assert_eq!(m("///", "////*", FNM_LEADING_DIR), 0);
}

#[test]
fn netbsd_fnmatch_noescape() {
    assert!(m("  \\x", "  \\x", 0) != 0);
    assert!(m("xx\\x", "xx\\x", 0) != 0);
    assert!(m("\\xxx", "\\xxx", 0) != 0);

    assert_eq!(m("  \\x", "  \\x", FNM_NOESCAPE), 0);
    assert_eq!(m("xx\\x", "xx\\x", FNM_NOESCAPE), 0);
    assert_eq!(m("\\xxx", "\\xxx", FNM_NOESCAPE), 0);
}

#[test]
fn netbsd_fnmatch_pathname() {
    assert!(m("???x", "xxx/x", FNM_PATHNAME) != 0);
    assert!(m("***x", "xxx/x", FNM_PATHNAME) != 0);

    assert_eq!(m("???x", "xxxx", FNM_PATHNAME), 0);
    assert_eq!(m("*/xxx", "/xxx", FNM_PATHNAME), 0);
    assert_eq!(m("x/*.y", "x/z.y", FNM_PATHNAME), 0);
}

#[test]
fn netbsd_fnmatch_period() {
    assert!(m("*x*", "X", FNM_PERIOD) != 0);
    assert_eq!(m("*x*", "X", FNM_PERIOD | FNM_CASEFOLD), 0);

    assert_eq!(m("x?y", "x.y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("x*y", "x.y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("*.c", "x.c", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("*/?", "x/y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("*/*", "x/y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m(".*/?", ".x/y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("*/.?", "x/.y", FNM_PATHNAME | FNM_PERIOD), 0);
    assert_eq!(m("x[.]y", "x.y", FNM_PATHNAME | FNM_PERIOD), 0);

    assert!(m("?x/y", ".x/y", FNM_PATHNAME | FNM_PERIOD) != 0);
    assert!(m("*x/y", ".x/y", FNM_PATHNAME | FNM_PERIOD) != 0);
    assert!(m("x/?y", "x/.y", FNM_PATHNAME | FNM_PERIOD) != 0);
    assert!(m("x/*y", "x/.y", FNM_PATHNAME | FNM_PERIOD) != 0);
}

#[test]
fn netbsd_fnmatch_initialbracket() {
    assert_eq!(m("[[?*\\\\]", "\\", 0), 0);
    assert_eq!(m("[]?*\\\\]", "]", 0), 0);
    assert_eq!(m("[!]a-]", "b", 0), 0);
    assert_eq!(m("[]-_]", "^", 0), 0);
    assert_eq!(m("[!]-_]", "X", 0), 0);
    assert_eq!(m("[A-\\\\]", "[", 0), 0);
    assert_eq!(m("[a-z]/[a-z]", "a/b", 0), 0);
    assert_eq!(m("[*]/b", "*/b", 0), 0);
    assert_eq!(m("[?]/b", "?/b", 0), 0);
    assert_eq!(m("[[a]/b", "a/b", 0), 0);
    assert_eq!(m("[[a]/b", "[/b", 0), 0);
    assert_eq!(m("[/b", "[/b", 0), 0);

    assert!(m("[*]/b", "a/b", 0) != 0);
    assert!(m("[?]/b", "a/b", 0) != 0);
}

#[test]
fn invalid_bytes_are_unmatchable_in_utf8_and_literal_in_bytes() {
    assert_eq!(fnmatch(b"?", b"\xff", 0, &Utf8Locale), FNM_NOMATCH);
    assert_eq!(fnmatch(b"?", b"\xff", 0, &ByteLocale), 0);
    assert_eq!(fnmatch(b"\xff", b"\xff", 0, &ByteLocale), 0);
}

#[test]
fn return_values_are_only_zero_or_one() {
    for &(pattern, string, flags, _) in VECTORS {
        let got = fnmatch(pattern, string, flags, &Utf8Locale);
        assert!(got == 0 || got == FNM_NOMATCH);
    }
}
